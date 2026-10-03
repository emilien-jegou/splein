// Single responsibility: End-to-end verification asserting multi-stage sanity table invariants.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::{Engine, FrameReport};

#[test]
fn test_e2e_idle_frame_invariant() {
    let mut engine = Engine::headless(500, 500);
    engine.mount(group().width(500.0).height(500.0).fill(Color::WHITE));
    engine.frame();

    // Always true: two frames in a row with no change -> laid_out == false, no damage, commands == 0
    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();

    assert!(!stats.laid_out, "E2E: laid_out must be false on idle");
    assert!(diag.spatial.damaged_rects.is_empty(), "E2E: No damage on idle");
    assert_eq!(diag.compile.commands_emitted, 0, "E2E: 0 commands emitted on idle");
}

#[test]
fn test_e2e_hover_box_with_shadow_recomputes_zero_layout() {
    let mut engine = Engine::headless(400, 400);
    let fill = engine.signal(Color::RED);

    engine.mount(group().width(400.0).height(400.0).children([
        group()
            .width(100.0).height(100.0)
            .fill(fill.clone())
            .shadows([Shadow::outer(0.0, 4.0, 6.0, Color::BLACK)]),
    ]));
    engine.frame();

    fill.set(Color::BLUE);
    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();

    assert!(!stats.laid_out, "E2E Hover: Layout must not recompute");
    assert_eq!(diag.layout.recomputed_nodes.len(), 0);
    assert_eq!(diag.spatial.damaged_rects.len(), 1);

    let d = diag.spatial.damaged_rects[0];
    assert!(d.width > 100.0);
    assert!(d.height > 100.0);
}

#[test]
fn test_e2e_counter_tick_inside_boundary_isolates_layout_and_damage() {
    let mut engine = Engine::headless(600, 600);
    let count = engine.signal("1".to_string());

    engine.mount(group().width(600.0).height(600.0).children([
        group().width(100.0).height(100.0).clip(true).children([
            text(count.clone()),
        ]),
        group().width(200.0).height(200.0).fill(Color::BLACK),
    ]));
    engine.frame();

    count.set("2".to_string());
    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();

    assert!(stats.laid_out);
    assert_eq!(diag.layout.recomputed_nodes.len(), 1, "E2E Counter: Boundary only");
    assert_eq!(diag.spatial.damaged_rects.len(), 1);

    let d = diag.spatial.damaged_rects[0];
    assert!(d.x >= 0.0 && d.y >= 0.0 && d.right() <= 100.0 && d.bottom() <= 100.0);
}

#[test]
fn test_e2e_box_in_boxed_re_render_with_absolute_badge() {
    let mut engine = Engine::headless(600, 600);
    let badge_count = engine.signal("3".to_string());

    // Outer card -> inner container -> absolute badge on the corner
    engine.mount(group().width(600.0).height(600.0).children([
        group().width(300.0).height(300.0).children([
            group().width(200.0).height(200.0).children([
                group().absolute().anchor(Anchor::TopRight).width(30.0).height(30.0).children([
                    text(badge_count.clone()),
                ]),
            ]),
        ]),
    ]));
    engine.frame();

    badge_count.set("4".to_string());
    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();

    assert!(stats.laid_out);
    assert!(!diag.spatial.damaged_rects.is_empty());
}
