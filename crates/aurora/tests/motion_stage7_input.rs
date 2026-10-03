// Single responsibility: TDD verification for reverse-order pointer hit-testing.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;

/// Two keyed cards side by side on a white root.
fn two_cards() -> impl IntoElement {
    group()
        .direction(Direction::Horizontal)
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .children((
            group().key("left").width(100.0).height(100.0).fill(Color::hex(0xEF4444)),
            group().key("right").width(100.0).height(100.0).fill(Color::hex(0x3B82F6)),
        ))
}

/// Returns the hit key's payload, so assertions read as plain strings.
fn key_at(engine: &Engine, x: f32, y: f32) -> Option<String> {
    engine.hit_test(x, y).map(|key| key.0)
}

#[test]
fn hit_test_resolves_the_card_under_a_point() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(two_cards());
    engine.frame();

    assert_eq!(key_at(&engine, 50.0, 50.0).as_deref(), Some("left"));
    assert_eq!(key_at(&engine, 150.0, 50.0).as_deref(), Some("right"));
    assert_eq!(key_at(&engine, 300.0, 300.0), None, "empty space resolves to nothing");
}

#[test]
fn hit_test_follows_a_motion_transform() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(two_cards());
    engine.frame();

    assert_eq!(
        key_at(&engine, 50.0, 50.0).as_deref(),
        Some("left"),
        "before moving, the card is at its layout origin"
    );

    engine.update_motion("left", |motion| {
        motion.transform = Transform::from_translation(200.0, 0.0);
    });
    engine.frame();

    assert_eq!(key_at(&engine, 50.0, 50.0), None, "the vacated origin no longer hits");
    assert_eq!(
        key_at(&engine, 250.0, 50.0).as_deref(),
        Some("left"),
        "the hit must follow the compositor transform"
    );
}

#[test]
fn hit_test_falls_through_to_the_keyed_ancestor() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([group().key("card").width(200.0).height(100.0).fill(Color::hex(0xEF4444)).children([
                group().width(80.0).height(40.0).fill(Color::hex(0x0A0A0A)),
            ])]),
    );
    engine.frame();

    assert_eq!(
        key_at(&engine, 40.0, 20.0).as_deref(),
        Some("card"),
        "an unkeyed child must fall through to its keyed parent"
    );
    assert_eq!(key_at(&engine, 150.0, 20.0).as_deref(), Some("card"), "the parent still hits");
}

#[test]
fn hit_test_prefers_the_later_sibling_on_overlap() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE).children((
            group().key("base").width(200.0).height(200.0).fill(Color::hex(0xEF4444)),
            group()
                .key("top")
                .width(100.0)
                .height(100.0)
                .fill(Color::hex(0x3B82F6))
                .anchor(Anchor::TopLeft),
        )),
    );
    engine.frame();

    assert_eq!(
        key_at(&engine, 50.0, 50.0).as_deref(),
        Some("top"),
        "the later sibling paints on top and must win"
    );
    assert_eq!(key_at(&engine, 150.0, 150.0).as_deref(), Some("base"));
}
