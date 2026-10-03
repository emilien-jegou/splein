// Single responsibility: TDD verification for motion controllers, retargeting, and the frame clock.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::motion::{Ease, Millis, MotionVector, Spring};
use aurora::runtime::{Engine, FrameReport};
use aurora::tree::DirtyFlags;

/// Builds a white root hosting one keyed red card at the top-left corner.
fn keyed_card() -> impl IntoElement {
    group()
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .children([group().key("card").width(100.0).height(100.0).fill(Color::RED)])
}

/// Runs one sixtieth of a second frames until the animation settles or `limit` elapses.
fn run_frames(engine: &mut Engine, limit: u32) {
    for _ in 0..limit {
        if !engine.has_active_motion() {
            break;
        }
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
}

#[test]
fn tween_interpolates_opacity_across_forced_frames() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine
        .animate("card")
        .opacity(0.5)
        .duration(400.ms())
        .ease(Ease::Linear)
        .play();
    assert!(engine.has_active_motion(), "a started animation must want frames");

    engine.advance(200.ms());
    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();

    assert_eq!(stats.flags, DirtyFlags::PAINT, "animation invalidates paint only");
    assert!(!stats.laid_out, "animation never reflows");
    assert!(diag.layout.recomputed_nodes.is_empty());
    let halfway = engine.motion_state("card").expect("animation wrote the view").opacity;
    assert!((halfway - 0.75).abs() < 1e-3, "expected 0.75 at half time, got {halfway}");

    engine.advance(200.ms());
    engine.frame();
    let finished = engine.motion_state("card").expect("animation wrote the view").opacity;
    assert!((finished - 0.5).abs() < 1e-3, "expected 0.5 at completion, got {finished}");
    assert!(
        !engine.has_active_motion(),
        "a settled animation must stop requesting frames"
    );
}

#[test]
fn spring_settles_scale_without_a_duration() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine.animate("card").scale(1.2).spring(Spring::snappy()).play();
    run_frames(&mut engine, 300);

    assert!(!engine.has_active_motion(), "a spring must settle on its own");
    let state = engine.motion_state("card").expect("settled motion persists");
    let motion = MotionVector::from_state(&state);
    assert!(
        (motion.scale_x - 1.2).abs() < 0.01,
        "expected scale_x 1.2, got {}",
        motion.scale_x
    );
    assert!(
        (motion.scale_y - 1.2).abs() < 0.01,
        "expected scale_y 1.2, got {}",
        motion.scale_y
    );
}

#[test]
fn retargeting_a_running_spring_carries_velocity() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine.animate("card").x(400.0).spring(Spring::snappy()).play();
    for _ in 0..8 {
        engine.advance(1.0 / 60.0);
        engine.frame();
    }

    let in_flight = MotionVector::from_state(
        &engine.motion_state("card").expect("controller writes during flight"),
    );
    let velocity = engine
        .controller("card")
        .expect("controller must still be running")
        .velocity();
    assert!(velocity.x.abs() > 1.0, "must be mid-flight, got {velocity:?}");

    engine.animate("card").x(100.0).spring(Spring::snappy()).play();

    let replaced = engine.controller("card").expect("retarget must replace");
    assert_eq!(replaced.velocity().x, velocity.x, "retarget must carry velocity");
    let position = MotionVector::from_state(
        &engine.motion_state("card").expect("motion persists"),
    );
    assert!(
        (position.x - in_flight.x).abs() < 1e-6,
        "retarget must not pop position, {} vs {}",
        position.x,
        in_flight.x
    );
}

#[test]
fn animation_stops_when_its_view_unmounts() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine.animate("card").x(300.0).duration(1.0).play();
    assert!(engine.has_active_motion());

    engine.mount(group().width(400.0).height(400.0));
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert!(!engine.has_active_motion(), "an orphaned animation must stop");
    assert!(engine.controller("card").is_none(), "an orphaned controller must be dropped");
}

#[test]
fn paused_controller_holds_its_value() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine
        .animate("card")
        .opacity(0.2)
        .duration(400.ms())
        .ease(Ease::Linear)
        .play();
    engine.advance(200.ms());
    engine.frame();
    let held = engine.motion_state("card").expect("animation wrote the view").opacity;

    engine.controller("card").expect("running").pause();
    assert!(!engine.has_active_motion(), "a paused animation needs no frames");

    engine.advance(200.ms());
    engine.frame();
    let after = engine.motion_state("card").expect("animation wrote the view").opacity;
    assert_eq!(after, held, "a paused animation must hold its value");
}

#[test]
fn rotation_interpolates_through_the_shortest_arc() {
    let start = MotionVector {
        rotation: 350.0,
        ..MotionVector::NEUTRAL
    };
    let end = MotionVector {
        rotation: 10.0,
        ..MotionVector::NEUTRAL
    };

    let midpoint = MotionVector::lerp(start, end, 0.5);

    assert!(
        (midpoint.rotation - 360.0).abs() < 1e-3,
        "must wrap the short way, got {}",
        midpoint.rotation
    );
}

#[test]
fn resumed_animation_continues_from_its_paused_progress() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine.animate("card").x(300.0).duration(1.0).ease(Ease::Linear).play();
    engine.advance(300.ms());
    engine.frame();

    engine.controller("card").expect("running").pause();
    engine.advance(300.ms());
    engine.frame();
    let paused = MotionVector::from_state(&engine.motion_state("card").expect("motion persists")).x;
    assert!((paused - 90.0).abs() < 1.0, "expected 90 at 30% of a linear tween, got {paused}");

    engine.controller("card").expect("paused").resume();
    assert!(engine.has_active_motion(), "resuming must request frames again");
    engine.advance(300.ms());
    engine.frame();

    let resumed = MotionVector::from_state(&engine.motion_state("card").expect("motion persists")).x;
    assert!((resumed - 180.0).abs() < 1.0, "expected 180 after resuming, got {resumed}");
}

#[test]
fn cancelled_animation_holds_its_position() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine.animate("card").x(300.0).duration(1.0).ease(Ease::Linear).play();
    engine.advance(400.ms());
    engine.frame();
    let held = MotionVector::from_state(&engine.motion_state("card").expect("motion persists")).x;
    assert!((held - 120.0).abs() < 1.0, "expected 120 before cancelling, got {held}");

    engine.controller("card").expect("running").cancel();
    assert!(!engine.has_active_motion(), "a cancelled animation needs no frames");

    engine.advance(400.ms());
    engine.frame();
    let after = MotionVector::from_state(&engine.motion_state("card").expect("motion persists")).x;
    assert_eq!(after, held, "a cancelled animation must hold its position");
    assert!(engine.controller("card").is_none(), "a cancelled controller must be dropped");
}

#[test]
fn animated_view_renders_its_interpolated_position() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine
        .animate("card")
        .x(200.0)
        .duration(400.ms())
        .ease(Ease::Linear)
        .play();
    engine.advance(200.ms());
    engine.frame();

    let canvas = engine.canvas();
    assert_eq!(
        canvas.pixel(110, 10).unwrap().green(),
        0,
        "the mid-flight position must paint red"
    );
    assert_eq!(
        canvas.pixel(10, 10).unwrap().green(),
        255,
        "the vacated origin must be cleared"
    );
}
