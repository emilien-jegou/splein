// Single responsibility: Gesture velocity hand-off from a release into a spring simulation.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::anim::{ScalarTrack, Spring, Transition};
use aurora::dsl::{group, IntoElement};
use aurora::foundation::Color;
use aurora::input::{PointerEvent, PointerPhase};
use aurora::reactive::{tick, ReactiveRuntime, Signal};
use aurora::runtime::Engine;

const DT: f32 = 1.0 / 60.0;

/// Steps `count` frames at a fixed delta, returning how far the value travelled.
///
/// Travel is the honest measure of what a hand-off changes: a spring given a velocity leaves
/// immediately, while one starting from rest eases away. It does not depend on how much a
/// given preset happens to overshoot.
fn travel(out: &Signal<f32>, frames: usize) -> f32 {
    let runtime = out.runtime();
    let mut path = 0.0f32;
    let mut previous = out.get();
    for _ in 0..frames {
        tick(&runtime, DT);
        let now = out.get();
        path += (now - previous).abs();
        previous = now;
    }
    path
}

/// How far past `home` the value travelled in the direction it was heading.
///
/// Measured as signed distance in the direction of travel, not absolute distance: absolute
/// distance would just report the starting amplitude, which says nothing about overshoot.
fn overshoot_past(out: &Signal<f32>, home: f32, heading_down: bool, frames: usize) -> f32 {
    let runtime = out.runtime();
    let mut worst = 0.0f32;
    for _ in 0..frames {
        tick(&runtime, DT);
        let past = if heading_down {
            home - out.get()
        } else {
            out.get() - home
        };
        worst = worst.max(past);
    }
    worst
}

/// Settles a spring at `rest`, then sends it to `home`, optionally with a handed velocity.
///
/// Returns the travel it covered and the furthest it went past `home`.
fn flick(spring: Spring, rest: f32, home: f32, velocity: Option<f32>) -> (f32, f32) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let target = Signal::new(Rc::clone(&runtime), rest);
    let out = Transition::spring(spring).on(target.clone());

    // Settle first so every run begins from identical state.
    travel(&out, 240);
    assert!(
        (out.get() - rest).abs() < 1.0,
        "the spring must settle before the flick is measured, got {}",
        out.get()
    );

    match velocity {
        Some(v) => target.set_with_velocity(home, v),
        None => target.set(home),
    }
    (
        travel(&out, 120),
        overshoot_past(&out, home, home < rest, 0),
    )
}

#[test]
#[ntest::timeout(30_000)]
fn a_handed_velocity_makes_the_spring_leave_immediately() {
    let (rest_path, _) = flick(Spring::snappy(), 200.0, 0.0, None);
    let (flick_path, _) = flick(Spring::snappy(), 200.0, 0.0, Some(-2400.0));
    assert!(
        flick_path > rest_path,
        "a spring handed a velocity must travel further than one from rest; \
         rest {rest_path:.1} vs flick {flick_path:.1}"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn the_velocity_is_what_carries_an_underdamped_spring_past_its_target() {
    // A critically damped spring converges monotonically no matter how much velocity it is
    // given, so overshoot only distinguishes the two cases under a spring that would
    // overshoot anyway. That makes this the sharpest proof the hand-off reaches the
    // simulation rather than merely being stored.
    let bouncy = Spring::bouncy();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let target = Signal::new(Rc::clone(&runtime), 200.0f32);
    let flicked = Transition::spring(bouncy).on(target.clone());
    travel(&flicked, 240);
    target.set_with_velocity(0.0, -2400.0);
    let flick_over = overshoot_past(&flicked, 0.0, true, 120);

    let runtime2 = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let target2 = Signal::new(Rc::clone(&runtime2), 200.0f32);
    let rest = Transition::spring(bouncy).on(target2.clone());
    travel(&rest, 240);
    target2.set(0.0);
    let rest_over = overshoot_past(&rest, 0.0, true, 120);

    assert!(
        flick_over > rest_over + 1.0,
        "a flick must carry an underdamped spring further past its target; \
         from rest {rest_over:.1} vs flicked {flick_over:.1}"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_plain_write_leaves_the_spring_at_rest() {
    let (path, over) = flick(Spring::snappy(), 200.0, 0.0, None);
    assert!(
        (path - 200.0).abs() < 2.0,
        "a spring from rest should travel the distance once, got {path:.1}"
    );
    assert!(
        over < 1.0,
        "a critically damped spring must not overshoot, got {over:.1}"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn the_hand_off_is_consumed_exactly_once() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let target = Signal::new(Rc::clone(&runtime), 0.0f32);
    let out = Transition::spring(Spring::bouncy()).on(target.clone());
    travel(&out, 240);

    target.set_with_velocity(100.0, -1800.0);
    // One long observation window: if the hand-off were re-applied every frame the spring
    // would be driven back past its target indefinitely instead of settling.
    travel(&out, 400);
    let settled = out.get();
    travel(&out, 60);
    assert!(
        (out.get() - settled).abs() < 1.0,
        "a one-shot hand-off must let the spring settle; it was {settled:.1}, now {}",
        out.get()
    );
    assert!(
        (out.get() - 100.0).abs() < 1.0,
        "and it must settle at the target, got {}",
        out.get()
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_non_finite_hand_off_cannot_poison_the_simulation() {
    let mut track = ScalarTrack::new(Spring::snappy(), 0.0, 100.0);
    track.adopt_velocity(f32::NAN);
    assert!(
        track.velocity.is_finite(),
        "a non-finite hand-off must be ignored, got {}",
        track.velocity
    );
    track.adopt_velocity(f32::INFINITY);
    assert!(track.velocity.is_finite(), "infinity must be ignored too");
    track.adopt_velocity(-500.0);
    assert_eq!(track.velocity, -500.0, "a real hand-off must be adopted");
}

#[test]
#[ntest::timeout(30_000)]
fn a_drag_release_hands_its_measured_velocity_to_a_spring() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    // Where the card rests, and the value a release retargets it to.
    let home = Signal::new(Rc::clone(&runtime), 40.0f32);
    let x = Transition::spring(Spring::snappy()).on(home.clone());
    let (dragging, releasing) = (home.clone(), home.clone());

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([group()
                .key("card")
                .width(120.0)
                .height(80.0)
                .fill(Color::BLACK)
                .x(x.clone())
                .on_drag_step(move |info| {
                    dragging.set_with_velocity(info.position.x, info.velocity.x)
                })
                .on_drag_end(move |info| releasing.set_with_velocity(0.0, info.velocity.x))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    engine.advance(DT);
    engine.frame();

    let send = |engine: &mut Engine, x: f32, y: f32, phase: PointerPhase| {
        engine.queue_pointer_event(PointerEvent {
            position: aurora::foundation::Point::new(x, y),
            button: aurora::input::PointerButton::Primary,
            phase,
            click_count: 0,
            shift: false,
            alt: false,
            ctrl: false,
            meta: false,
        });
        engine.advance(DT);
        engine.frame();
    };

    // The card rests at x=40, so press inside it and flick right at ~60px per frame.
    send(&mut engine, 60.0, 40.0, PointerPhase::Down);
    send(&mut engine, 76.0, 40.0, PointerPhase::Move);
    for step in 1..=6 {
        send(
            &mut engine,
            76.0 + 60.0 * step as f32,
            40.0,
            PointerPhase::Drag,
        );
    }
    assert!(
        x.get() > 250.0,
        "the drag must have carried the card with the pointer, it is at {}",
        x.get()
    );

    // Release while still moving: the handler retargets home and hands over the velocity.
    send(&mut engine, 76.0 + 60.0 * 7.0, 40.0, PointerPhase::Up);
    assert_eq!(home.get(), 0.0, "release must retarget the card home");

    // The hand-off makes the card leave immediately instead of easing off at its own pace.
    let mut start = x.get();
    let mut travel_after_release = 0.0f32;
    for _ in 0..4 {
        engine.advance(DT);
        engine.frame();
        travel_after_release += (x.get() - start).abs();
        start = x.get();
    }
    assert!(
        travel_after_release > 10.0,
        "a released flick must move away quickly, moved {travel_after_release:.1} in four frames"
    );

    for _ in 0..120 {
        engine.advance(DT);
        engine.frame();
    }
    assert!(
        x.get().abs() < 1.0,
        "the card must still settle at home, ended at {}",
        x.get()
    );
}
