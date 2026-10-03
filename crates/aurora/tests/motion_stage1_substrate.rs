// Single responsibility: TDD verification for the motion substrate: identity, composition, damage.

use aurora::dsl::*;
use aurora::foundation::*;
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

#[test]
fn motion_transform_is_paint_only_and_reflows_nothing() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    assert!(engine.update_motion("card", |m| {
        m.transform = Transform::from_translation(40.0, 0.0)
    }));
    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();

    assert_eq!(
        stats.flags,
        DirtyFlags::PAINT,
        "Motion invalidates paint only"
    );
    assert!(!stats.laid_out, "Motion never reflows");
    assert!(
        diag.layout.recomputed_nodes.is_empty(),
        "Motion recomputes no layout nodes"
    );
}

#[test]
fn motion_move_damages_both_old_and_new_positions() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine.update_motion("card", |m| m.transform = Transform::from_translation(100.0, 0.0));
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    let rects = &diag.spatial.damaged_rects;
    assert!(
        rects.iter().any(|r| r.contains(Point::new(50.0, 50.0))),
        "The vacated position must be damaged: {rects:?}"
    );
    assert!(
        rects.iter().any(|r| r.contains(Point::new(150.0, 50.0))),
        "The occupied position must be damaged: {rects:?}"
    );
}

#[test]
fn motion_transform_composes_after_declarative_transform() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([
                group()
                    .key("card")
                    .width(100.0)
                    .height(100.0)
                    .fill(Color::RED)
                    .transform(Transform::from_translation(50.0, 0.0)),
            ]),
    );
    engine.frame();

    engine.update_motion("card", |m| m.transform = Transform::from_translation(25.0, 0.0));
    engine.frame();

    // Declarative 50 + motion 25 renders the card across x 75..175.
    let canvas = engine.canvas();
    assert_eq!(canvas.pixel(80, 10).unwrap().green(), 0, "Composed origin must paint");
    assert_eq!(canvas.pixel(160, 10).unwrap().green(), 0, "Composed extent must paint");
    assert_eq!(
        canvas.pixel(55, 10).unwrap().green(),
        255,
        "The declarative-only origin must be cleared"
    );
}

#[test]
fn motion_opacity_reaches_the_compositor_without_reflow() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    assert!(engine.update_motion("card", |m| m.opacity = 0.5));
    let FrameReport { stats, .. } = engine.frame();

    assert!(!stats.laid_out, "Motion opacity never reflows");

    // Red at half alpha over the opaque white root blends to pink, not to partial alpha.
    let blended = engine.canvas().pixel(50, 10).unwrap();
    assert_eq!(blended.red(), 255, "Red channel must stay saturated");
    assert!(
        (110..=145).contains(&blended.green()),
        "Half opacity must blend to near 128, got {}",
        blended.green()
    );
}

#[test]
fn keyed_motion_lookup_resolves_only_live_nodes() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    assert!(
        engine.update_motion("card", |m| m.opacity = 0.9),
        "A mounted key must resolve"
    );
    assert!(
        !engine.update_motion("ghost", |m| m.opacity = 0.9),
        "An unknown key must resolve nothing"
    );

    engine.mount(group().width(400.0).height(400.0));
    engine.frame();
    assert!(
        !engine.update_motion("card", |m| m.opacity = 0.8),
        "A key unmounted with its node must resolve nothing"
    );
}

#[test]
fn settled_motion_keeps_the_idle_frame_invariant() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(keyed_card());
    engine.frame();

    engine.update_motion("card", |m| m.transform = Transform::from_translation(20.0, 0.0));
    engine.frame();

    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();
    assert!(!stats.laid_out, "Settled motion must not reflow");
    assert!(
        diag.spatial.damaged_rects.is_empty(),
        "Settled motion must not damage"
    );
    assert_eq!(
        diag.compile.commands_emitted, 0,
        "Settled motion must not recompile"
    );
}
