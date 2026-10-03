// Single responsibility: TDD verification for motion easing, tweening, and spring mathematics.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::motion::{Ease, Spring, SpringState, Tween};
use aurora::reactive::{ReactiveRuntime, Signal};

/// Advances a spring one sixtieth of a second at a time for `seconds`.
fn settle(mut state: SpringState, seconds: f32) -> SpringState {
    for _ in 0..(seconds * 60.0) as u32 {
        state.advance(1.0 / 60.0);
    }
    state
}

/// Integrates a spring for `frames` frames while tracking how far it travels past its target.
fn peak_position(mut state: SpringState, frames: u32) -> f32 {
    let mut peak = f32::MIN;
    for _ in 0..frames {
        state.advance(1.0 / 60.0);
        peak = peak.max(state.position);
    }
    peak
}

// ---- Easing ----

#[test]
fn ease_endpoints_are_exact() {
    let eases = [
        Ease::Linear,
        Ease::InCubic,
        Ease::OutCubic,
        Ease::InOutCubic,
        Ease::OutQuint,
        Ease::InOutQuint,
        Ease::OutExpo,
    ];
    for ease in eases {
        assert!(
            (ease.sample(0.0)).abs() < 1e-6,
            "{ease:?} must start at zero, got {}",
            ease.sample(0.0)
        );
        assert!(
            (ease.sample(1.0) - 1.0).abs() < 1e-6,
            "{ease:?} must end at one, got {}",
            ease.sample(1.0)
        );
    }
}

#[test]
fn linear_ease_is_the_identity() {
    assert_eq!(Ease::Linear.sample(0.37), 0.37);
}

#[test]
fn out_eases_front_load_their_progress() {
    assert!(Ease::OutCubic.sample(0.25) > 0.25, "OutCubic must lead");
    assert!(
        Ease::OutQuint.sample(0.25) > Ease::OutCubic.sample(0.25),
        "OutQuint must lead OutCubic"
    );
    assert!(Ease::InCubic.sample(0.25) < 0.25, "InCubic must trail");
}

#[test]
fn in_ease_mirrors_its_out_counterpart() {
    for t in [0.0, 0.1, 0.35, 0.5, 0.75, 1.0] {
        let mirrored = 1.0 - Ease::OutCubic.sample(1.0 - t);
        assert!(
            (Ease::InCubic.sample(t) - mirrored).abs() < 1e-6,
            "InCubic must mirror OutCubic at {t}"
        );
    }
}

#[test]
fn in_out_eases_cross_the_midpoint() {
    assert!((Ease::InOutCubic.sample(0.5) - 0.5).abs() < 1e-6);
    assert!((Ease::InOutQuint.sample(0.5) - 0.5).abs() < 1e-6);
}

#[test]
fn ease_clamps_progress_outside_the_unit_range() {
    assert_eq!(Ease::OutCubic.sample(-3.0), 0.0);
    assert_eq!(Ease::OutCubic.sample(4.0), 1.0);
}

// ---- Tweening ----

#[test]
fn tween_lerps_across_its_duration() {
    let tween = Tween::new(0.4, Ease::Linear);
    assert_eq!(tween.lerp(0.0, -10.0, 30.0), -10.0);
    assert_eq!(tween.lerp(0.2, -10.0, 30.0), 10.0);
    assert_eq!(tween.lerp(0.4, -10.0, 30.0), 30.0);
}

#[test]
fn tween_fraction_clamps_before_start_and_after_end() {
    let tween = Tween::new(0.25, Ease::OutCubic);
    assert_eq!(tween.fraction(-1.0), 0.0);
    assert_eq!(tween.fraction(9.0), 1.0);
}

#[test]
fn tween_completes_at_its_duration() {
    let tween = Tween::new(0.3, Ease::Linear);
    assert!(!tween.is_complete(0.29));
    assert!(tween.is_complete(0.3));
}

#[test]
fn zero_duration_tween_completes_immediately() {
    let tween = Tween::new(0.0, Ease::Linear);
    assert!(tween.is_complete(0.0));
    assert_eq!(tween.progress(0.0), 1.0);
}

// ---- Springs ----

#[test]
fn spring_settles_on_its_target() {
    let settled = settle(SpringState::new(Spring::snappy(), 0.0, 1.0), 3.0);
    assert!(
        settled.is_settled(),
        "snappy must rest, position={} velocity={}",
        settled.position,
        settled.velocity
    );
    assert!((settled.position - 1.0).abs() < 0.01);
}

#[test]
fn overdamped_spring_never_crosses_its_target() {
    let peak = peak_position(SpringState::new(Spring::smooth(), 0.0, 1.0), 180);
    assert!(peak <= 1.01, "smooth must not overshoot, peaked at {peak}");
}

#[test]
fn underdamped_spring_overshoots_its_target() {
    let peak = peak_position(SpringState::new(Spring::bouncy(), 0.0, 1.0), 180);
    assert!(peak > 1.05, "bouncy must overshoot, peaked at {peak}");
}

#[test]
fn damping_ratio_classifies_each_preset() {
    assert!(Spring::snappy().damping_ratio() < 1.0, "snappy overshoots");
    assert!(
        Spring::bouncy().damping_ratio() < 0.6,
        "bouncy overshoots clearly"
    );
    assert!(
        Spring::smooth().damping_ratio() > 1.0,
        "smooth never overshoots"
    );
    assert!((Spring::new(400.0, 40.0, 1.0).damping_ratio() - 1.0).abs() < 1e-6);
}

#[test]
fn retarget_preserves_position_and_velocity() {
    let mut state = SpringState::new(Spring::snappy(), 0.0, 400.0);
    for _ in 0..10 {
        state.advance(1.0 / 60.0);
    }

    let (position, velocity) = (state.position, state.velocity);
    assert!(velocity.abs() > 1.0, "spring must be mid-flight, got {velocity}");

    state.retarget(100.0);
    assert_eq!(state.position, position, "retarget must not pop position");
    assert_eq!(state.velocity, velocity, "retarget must not pop velocity");
    assert_eq!(state.target, 100.0);
}

#[test]
fn launch_velocity_is_imparted_to_the_simulation() {
    let spring = Spring::snappy().with_velocity(100.0);
    assert_eq!(SpringState::new(spring, 0.0, 1.0).velocity, 100.0);
}

#[test]
fn settled_spring_stops_moving() {
    let mut state = settle(SpringState::new(Spring::snappy(), 0.0, 1.0), 3.0);
    assert!(state.is_settled());

    let resting = state.position;
    for _ in 0..60 {
        state.advance(1.0 / 60.0);
    }
    assert!((state.position - resting).abs() < 1e-4, "settled spring drifted");
}

#[test]
fn a_long_frame_delta_cannot_explode_the_spring() {
    let mut state = SpringState::new(Spring::snappy(), 0.0, 1.0);
    state.advance(5.0);

    assert!(
        state.position.is_finite() && state.velocity.is_finite(),
        "a hitch must stay finite"
    );
    assert!(
        state.position.abs() < 4.0,
        "a hitch must not teleport the spring, got {}",
        state.position
    );
}

// ---- Reactive derivation ----

#[test]
fn signal_map_derives_updated_values() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let source = Signal::new(Rc::clone(&runtime), 10.0f32);
    let doubled = source.map(|value| value * 2.0);

    assert_eq!(doubled.get(), 20.0);
    source.set(21.0);
    assert_eq!(doubled.get(), 42.0);
}
