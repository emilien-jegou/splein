// Single responsibility: TDD verification for timeline markers, labels, and delays.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::motion::{Ease, Millis, MotionVector};
use aurora::runtime::Engine;

/// Reads the horizontal motion currently applied to a keyed view.
fn x_of(engine: &Engine, key: &str) -> f32 {
    engine
        .motion_state(key)
        .map(|state| MotionVector::from_state(&state).x)
        .unwrap_or(0.0)
}

/// Steps the clock forward in `count` equal slices of `slice_ms`.
fn advance_by(engine: &mut Engine, count: u32, slice_ms: u32) {
    for _ in 0..count {
        engine.advance((slice_ms as f32).ms());
        engine.frame();
    }
}

/// Builds a white root hosting three keyed cards.
fn stage() -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .children((
            group().key("a").width(60.0).height(60.0).fill(Color::hex(0xEF4444)),
            group().key("b").width(60.0).height(60.0).fill(Color::hex(0x3B82F6)),
            group().key("c").width(60.0).height(60.0).fill(Color::hex(0x22C55E)),
        ))
}

#[test]
fn relative_marker_delays_an_entry_until_its_instant() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(stage());
    engine.frame();

    engine
        .timeline()
        .to("a").x(100.0).duration(400.ms()).ease(Ease::Linear)
        .to("b").x(100.0).duration(400.ms()).ease(Ease::Linear).at("+200ms")
        .play();

    assert!(engine.has_active_motion(), "a timeline must keep frames flowing");

    advance_by(&mut engine, 2, 50);
    let a = x_of(&engine, "a");
    let b = x_of(&engine, "b");
    assert!((a - 25.0).abs() < 1.0, "a must be 25% through at 100ms, got {a}");
    assert_eq!(b, 0.0, "b must still be waiting for its 200ms marker");

    advance_by(&mut engine, 4, 50);
    let b = x_of(&engine, "b");
    assert!(b > 5.0, "b must have started once its marker passed, got {b}");
}

#[test]
fn with_previous_marker_shares_the_previous_start() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(stage());
    engine.frame();

    engine
        .timeline()
        .to("a").x(100.0).duration(400.ms()).ease(Ease::Linear)
        .to("b").x(100.0).duration(400.ms()).ease(Ease::Linear).at("<")
        .play();

    advance_by(&mut engine, 2, 50);

    let a = x_of(&engine, "a");
    let b = x_of(&engine, "b");
    assert!((a - 25.0).abs() < 1.0, "a must be 25% through, got {a}");
    assert!((b - 25.0).abs() < 1.0, "`<` must start b with a, got {b}");
}

#[test]
fn label_marker_returns_to_its_recorded_time() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(stage());
    engine.frame();

    engine
        .timeline()
        .label("origin")
        .to("a").x(100.0).duration(400.ms()).ease(Ease::Linear).at("+300ms")
        .to("b").x(100.0).duration(400.ms()).ease(Ease::Linear).at("origin")
        .play();

    advance_by(&mut engine, 1, 50);
    assert_eq!(x_of(&engine, "a"), 0.0, "a must wait for its 300ms marker");
    assert!(
        x_of(&engine, "b") > 0.0,
        "b must start at the label, not at the timeline cursor"
    );

    advance_by(&mut engine, 6, 50);
    assert!(x_of(&engine, "a") > 5.0, "a must start once its marker passes");
}

#[test]
fn absolute_marker_pins_an_entry_independently() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(stage());
    engine.frame();

    engine
        .timeline()
        .to("a").x(100.0).duration(400.ms()).ease(Ease::Linear)
        .to("c").x(100.0).duration(400.ms()).ease(Ease::Linear).at("300ms")
        .play();

    advance_by(&mut engine, 4, 50);
    assert_eq!(x_of(&engine, "c"), 0.0, "an absolute 300ms entry waits past 200ms");

    advance_by(&mut engine, 4, 50);
    assert!(x_of(&engine, "c") > 5.0, "the pinned entry must start at 300ms");
}

#[test]
fn timeline_stops_once_every_entry_settles() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(stage());
    engine.frame();

    engine
        .timeline()
        .to("a").x(100.0).duration(200.ms()).ease(Ease::Linear)
        .to("b").x(100.0).duration(200.ms()).ease(Ease::Linear).at("+400ms")
        .play();

    advance_by(&mut engine, 10, 100);
    assert!(
        !engine.has_active_motion(),
        "a delayed timeline must settle once its last entry finishes"
    );
}
