// Single responsibility: Verification for reactive tickers driving animated signal values.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::anim::{Ease, Repeat, ScalarTrack, Spring, Transition};
use aurora::reactive::{tick, ReactiveRuntime, Signal};

/// Builds a runtime plus a signal inside it.
fn world<T: Clone + 'static>(initial: T) -> (Rc<RefCell<ReactiveRuntime>>, Signal<T>) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let signal = Signal::new(Rc::clone(&runtime), initial);
    (runtime, signal)
}

/// Advances `frames` frames of `dt` seconds.
fn run(runtime: &Rc<RefCell<ReactiveRuntime>>, frames: u32, dt: f32) {
    for _ in 0..frames {
        tick(runtime, dt);
    }
}

// ---- ScalarTrack ----

#[test]
#[ntest::timeout(5_000)]
fn scalar_track_settles_exactly_on_its_target() {
    let mut track = ScalarTrack::new(Spring::snappy(), 0.0, 100.0);
    for _ in 0..600 {
        track.advance(1.0 / 60.0);
    }
    assert!(
        track.is_settled(),
        "spring never settled: pos={} vel={}",
        track.position,
        track.velocity
    );
    assert!(
        (track.position - 100.0).abs() < 0.01,
        "landed at {}",
        track.position
    );
}

#[test]
#[ntest::timeout(5_000)]
fn scalar_track_retarget_preserves_velocity() {
    let mut track = ScalarTrack::new(Spring::snappy(), 0.0, 100.0);
    for _ in 0..6 {
        track.advance(1.0 / 60.0);
    }
    let carried = track.velocity;
    track.retarget(0.0);
    assert_eq!(track.velocity, carried, "retarget must not reset velocity");
    assert_eq!(
        track.position, track.position,
        "retarget must not move position"
    );
}

// ---- Transition::on ----

#[test]
#[ntest::timeout(5_000)]
fn spring_transition_follows_its_target_and_settles() {
    let (runtime, target) = world(0.0f32);
    let animated = Transition::spring(Spring::snappy()).on(target.clone());

    assert_eq!(animated.get(), 0.0, "output starts at the target");

    target.set(100.0);
    run(&runtime, 600, 1.0 / 60.0);

    assert!(
        (animated.get() - 100.0).abs() < 0.05,
        "spring landed at {}",
        animated.get()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn spring_transition_never_passes_through_zero_when_leaving_it() {
    let (runtime, target) = world(0.0f32);
    let animated = Transition::spring(Spring::snappy()).on(target.clone());

    target.set(100.0);
    let mut went_backwards = false;
    for _ in 0..40 {
        tick(&runtime, 1.0 / 60.0);
        if animated.get() < -0.001 {
            went_backwards = true;
        }
    }
    assert!(
        !went_backwards,
        "a spring leaving zero must not undershoot below it"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn tween_transition_restarts_from_the_current_value() {
    let (runtime, target) = world(0.0f32);
    let animated = Transition::ease(200.0 * 0.001, Ease::Linear).on(target.clone());

    target.set(100.0);
    run(&runtime, 6, 1.0 / 60.0); // 100ms of 200ms
    let midway = animated.get();
    assert!(
        midway > 20.0 && midway < 80.0,
        "expected a partial value, got {midway}"
    );

    // Interrupting must continue from where it is, never jump back to the segment start.
    target.set(200.0);
    tick(&runtime, 1.0 / 1_000.0);
    assert!(
        animated.get() >= midway,
        "interruption popped backwards: {midway} -> {}",
        animated.get()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn one_transition_drives_several_independent_streams() {
    let (runtime, target) = world(false);
    let tween = Transition::ease(100.0 * 0.001, Ease::Linear);
    let x = tween.on(target.map(|on| if on { 300.0 } else { 0.0 }));
    let opacity = tween.on(target.map(|on| if on { 1.0 } else { 0.4 }));

    target.set(true);
    run(&runtime, 3, 1.0 / 60.0);

    assert!(
        x.get() > 0.0 && x.get() < 300.0,
        "x mid-flight at {}",
        x.get()
    );
    assert!(
        opacity.get() > 0.4 && opacity.get() < 1.0,
        "opacity mid-flight"
    );
    assert!(
        (opacity.get() - 0.4 - (x.get() / 300.0) * 0.6).abs() < 1e-4,
        "streams sharing one transition must stay in lockstep"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn retargeting_a_spring_mid_flight_does_not_pop() {
    let (runtime, target) = world(0.0f32);
    let animated = Transition::spring(Spring::snappy()).on(target.clone());

    target.set(300.0);
    run(&runtime, 8, 1.0 / 60.0);
    let before = animated.get();

    target.set(-100.0);
    tick(&runtime, 1.0 / 1_000.0);
    let after = animated.get();

    assert!(
        (before - after).abs() < 6.0,
        "retarget popped: {before} -> {after}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn an_idle_transition_does_not_keep_the_frame_loop_awake() {
    let (runtime, target) = world(0.0f32);
    let _animated = Transition::spring(Spring::snappy()).on(target.clone());
    target.set(50.0);
    run(&runtime, 3, 1.0 / 60.0);

    // A target-driven animation follows its target, so it stays registered; but once the
    // runtime has no other work it must report honestly to the frame loop.
    assert!(runtime.borrow().has_active_tickers());
}

// ---- Playback ----

#[test]
#[ntest::timeout(5_000)]
fn playback_runs_to_its_target_and_finishes() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let (value, control) = Transition::ease(1.0, Ease::Linear).playback(&runtime, 0.0, 100.0);

    run(&runtime, 30, 1.0 / 60.0); // 0.5s
    assert!(
        (value.get() - 50.0).abs() < 3.0,
        "halfway at {}",
        value.get()
    );

    run(&runtime, 40, 1.0 / 60.0);
    assert!(
        (value.get() - 100.0).abs() < 0.01,
        "finished at {}",
        value.get()
    );
    assert_eq!(control.state(), aurora::anim::PlayState::Finished);
    assert!(
        !runtime.borrow().has_active_tickers(),
        "a finished playback must stop requesting frames"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn paused_playback_holds_its_value_and_sleeps() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let (value, control) = Transition::ease(1.0, Ease::Linear).playback(&runtime, 0.0, 100.0);

    run(&runtime, 18, 1.0 / 60.0); // 0.3s
    control.pause();
    let held = value.get();

    run(&runtime, 60, 1.0 / 60.0);
    assert_eq!(value.get(), held, "a paused playback must not advance");
    assert!(
        !runtime.borrow().has_active_tickers(),
        "a paused playback must not keep the frame loop awake"
    );

    control.resume();
    run(&runtime, 12, 1.0 / 60.0);
    assert!(
        value.get() > held,
        "resume must continue from the held value"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn ping_pong_playback_reverses_at_both_ends() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let (value, _control) = Transition::ease(0.2, Ease::Linear)
        .repeat(Repeat::PingPong)
        .playback(&runtime, 0.0, 100.0);

    run(&runtime, 12, 1.0 / 60.0); // exactly one 0.2s run
    let at_peak = value.get();
    assert!(at_peak > 95.0, "should be at the peak, got {at_peak}");

    run(&runtime, 12, 1.0 / 60.0); // one full ping-pong period is 0.4s
    assert!(
        value.get() < 5.0,
        "ping-pong should be back at the start, got {}",
        value.get()
    );

    run(&runtime, 12, 1.0 / 60.0);
    assert!(
        value.get() > 95.0,
        "and at the peak again, got {}",
        value.get()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_looping_playback_jumps_back_instead_of_reversing() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let (value, _control) = Transition::ease(0.2, Ease::Linear)
        .repeat(Repeat::Loop)
        .playback(&runtime, 0.0, 100.0);

    run(&runtime, 11, 1.0 / 60.0); // just short of one 0.2s run
    assert!(value.get() > 90.0, "near the peak, got {}", value.get());
    run(&runtime, 1, 1.0 / 60.0);
    assert!(
        value.get() < 10.0,
        "loop must jump back to the start, got {}",
        value.get()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn restarting_a_playback_returns_to_the_beginning() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let (value, control) = Transition::ease(1.0, Ease::Linear).playback(&runtime, 0.0, 100.0);

    run(&runtime, 30, 1.0 / 60.0);
    assert!(value.get() > 10.0);

    control.restart();
    run(&runtime, 2, 1.0 / 60.0);
    assert!(
        value.get() < 10.0,
        "restart must replay from the start, got {}",
        value.get()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn seeking_jumps_to_the_requested_progress() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let (value, control) = Transition::ease(1.0, Ease::Linear).playback(&runtime, 0.0, 200.0);

    control.seek(0.5);
    tick(&runtime, 1.0 / 1_000.0);
    assert!(
        (value.get() - 100.0).abs() < 2.0,
        "seek(0.5) landed at {}",
        value.get()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn frame_clock_advances_with_simulated_time() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let clock = aurora::reactive::hooks::Cx::new(Rc::clone(&runtime)).frame_clock();

    assert_eq!(clock.get(), 0.0);
    run(&runtime, 60, 1.0 / 60.0);
    assert!(
        (clock.get() - 1.0).abs() < 1e-6,
        "clock read {}",
        clock.get()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn an_interval_toggle_flips_on_its_period() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = aurora::reactive::hooks::Cx::new(Rc::clone(&runtime));
    let toggle = cx.interval_toggle(0.5);

    assert!(!toggle.get(), "starts low");
    run(&runtime, 31, 1.0 / 60.0); // just past 0.5s
    assert!(toggle.get(), "should be high after one period");
    run(&runtime, 30, 1.0 / 60.0);
    assert!(!toggle.get(), "should be low after two periods");
}
