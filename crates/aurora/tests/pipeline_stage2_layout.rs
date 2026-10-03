// Single responsibility: TDD verification for Stage 2 layout caching, boundaries, and shaping.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::{Engine, FrameReport};

#[test]
fn test_2_1_fixed_child_hits_cache_while_fill_child_recomputes() {
    let mut engine = Engine::headless(100, 100);
    engine.mount(group().direction(Direction::Horizontal).width(100.0).height(100.0).children([
        group().width(50.0).height(100.0),
        group().width(Size::fill()).height(100.0),
    ]));
    engine.frame();

    engine.resize(150, 100);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert!(diag.layout.laid_out);
    assert_eq!(diag.layout.recomputed_nodes.len(), 1, "Scenario 2.1: Only B is recomputed");
    assert_eq!(diag.layout.cached_nodes.len(), 1, "Scenario 2.1: A must hit layout cache");
}

#[test]
fn test_2_2_moved_sibling_does_not_remeasure() {
    let mut engine = Engine::headless(200, 100);
    engine.mount(group().direction(Direction::Horizontal).width(200.0).height(100.0).children([
        group().width(50.0).height(100.0),
        group().width(Size::fill()).height(100.0),
        group().width(30.0).height(100.0),
    ]));
    engine.frame();

    engine.resize(300, 100);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    let cached_c = diag
        .layout
        .cached_nodes
        .iter()
        .find(|r| r.width == 30.0)
        .expect("Scenario 2.2: Sibling C must hit cache");
    assert_eq!(cached_c.x, 270.0, "Scenario 2.2: C moved to 270 without remeasuring");
}

#[test]
fn test_2_3_text_in_fixed_container_skips_reshaping_on_window_resize() {
    let mut engine = Engine::headless(200, 100);
    engine.mount(group().direction(Direction::Horizontal).width(200.0).height(100.0).children([
        group().width(50.0).height(50.0).children([text("Fixed")]),
        group().width(Size::fill()).height(50.0),
    ]));
    engine.frame();

    engine.resize(300, 100);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert!(diag.layout.cached_nodes.iter().any(|r| r.width == 50.0));
}

#[test]
fn test_2_4_height_only_resize_never_reshapes_single_line_text() {
    let mut engine = Engine::headless(200, 100);
    engine.mount(group().direction(Direction::Vertical).width(200.0).height(100.0).children([text("Label")]));
    engine.frame();

    engine.resize(200, 200);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert!(diag.layout.cached_nodes.len() >= 1);
}

#[test]
fn test_2_5_clipped_fixed_boundary_isolates_layout_to_boundary_only() {
    let mut engine = Engine::headless(800, 600);
    let count = engine.signal("0".to_string());

    engine.mount(group().width(800.0).height(600.0).children([
        group().width(100.0).height(100.0).clip(true).children([
            text(count.clone()),
        ]),
        group().width(200.0).height(200.0),
    ]));
    engine.frame();

    count.set("1".to_string());
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert!(diag.layout.laid_out);
    assert_eq!(diag.layout.recomputed_nodes.len(), 1, "Scenario 2.5: Only boundary node laid out");
}

#[test]
fn test_2_6_disjoint_boundaries_both_dirty_execute_independently() {
    let mut engine = Engine::headless(600, 600);
    let w1 = engine.signal(20.0f32);
    let w2 = engine.signal(20.0f32);

    engine.mount(group().width(600.0).height(600.0).children([
        group().width(100.0).height(100.0).children([group().width(w1.clone()).height(20.0)]),
        group().width(100.0).height(100.0).children([group().width(w2.clone()).height(20.0)]),
    ]));
    engine.frame();

    w1.set(40.0);
    w2.set(40.0);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert_eq!(diag.layout.recomputed_nodes.len(), 2, "Scenario 2.6: Both boundaries recomputed");
}

#[test]
fn test_2_7_nested_dirty_boundaries_deduplicate_to_outer_boundary() {
    let mut engine = Engine::headless(800, 800);
    let sig_outer = engine.signal(80.0f32);
    let sig_inner = engine.signal(40.0f32);

    engine.mount(group().width(800.0).height(800.0).children([
        group().width(300.0).height(300.0).children([
            group().width(sig_outer.clone()).height(100.0),
            group().width(150.0).height(150.0).children([
                group().width(sig_inner.clone()).height(50.0),
            ]),
        ]),
    ]));
    engine.frame();

    sig_outer.set(90.0);
    sig_inner.set(50.0);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert!(diag.layout.laid_out);
    assert_eq!(diag.layout.recomputed_nodes.len(), 1, "Scenario 2.7: Pruned to 1 outer boundary");
    assert_eq!(diag.layout.recomputed_nodes[0].width, 300.0);
}

#[test]
fn test_2_8_boundary_escalates_to_parent_when_fixed_changes_to_fit() {
    let mut engine = Engine::headless(500, 500);
    let size_intent = engine.signal(Size::Fixed(100.0));

    engine.mount(group().width(500.0).height(500.0).children([
        group().width(size_intent.clone()).height(100.0),
    ]));
    engine.frame();

    size_intent.set(Size::Fit);
    let FrameReport { stats, .. } = engine.frame();

    assert!(stats.laid_out, "Scenario 2.8: Loss of fixed constraint escalates layout");
}

#[test]
fn test_2_9_boundary_relayout_uses_exact_parent_constraints() {
    let mut engine = Engine::headless(600, 600);
    let inner_w = engine.signal(50.0f32);

    engine.mount(group().width(600.0).height(600.0).children([
        group().width(250.0).height(250.0).children([
            group().width(inner_w.clone()).height(50.0),
        ]),
    ]));
    engine.frame();

    inner_w.set(80.0);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert_eq!(diag.layout.recomputed_nodes.len(), 1);
    assert_eq!(diag.layout.recomputed_nodes[0], ResolvedRect::new(0.0, 0.0, 250.0, 250.0));
}
