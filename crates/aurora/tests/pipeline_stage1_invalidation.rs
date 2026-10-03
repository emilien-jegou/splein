// Single responsibility: TDD verification for Stage 1 invalidation classification and bypasses.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::reactive::*;
use aurora::runtime::Engine;
use aurora::tree::DirtyFlags;

#[test]
fn test_1_1_fill_change_sets_paint_only_and_bypasses_layout() {
    let mut engine = Engine::headless(400, 400);
    let fill = engine.signal(Color::RED);
    engine.mount(group().width(200.0).height(200.0).fill(fill.clone()));
    engine.frame();

    fill.set(Color::BLUE);
    let (stats, _, _, diag) = engine.frame();

    assert_eq!(stats.flags, DirtyFlags::PAINT);
    assert!(
        !stats.laid_out,
        "Scenario 1.1: Must not trigger layout pass"
    );
    assert!(diag.layout.recomputed_nodes.is_empty());
}

#[test]
fn test_1_2_opacity_and_shadow_color_are_paint_only() {
    let mut engine = Engine::headless(400, 400);
    let opacity = engine.signal(1.0f32);
    let shadow_color = engine.signal(Color::rgba(0.0, 0.0, 0.0, 0.5));
    let sc = shadow_color.clone();
    let dyn_shadow = computed(move || Shadow::outer(0.0, 4.0, 6.0, sc.get()));

    engine.mount(
        group()
            .width(100.0).height(100.0)
            .opacity(opacity.clone())
            .dynamic_shadow(dyn_shadow),
    );
    engine.frame();

    // 1. Opacity mutation
    opacity.set(0.5);
    let (stats_opacity, _, _, _) = engine.frame();
    assert_eq!(stats_opacity.flags, DirtyFlags::PAINT);
    assert!(
        !stats_opacity.laid_out,
        "Scenario 1.2: Opacity must be PAINT only"
    );

    // 2. Shadow color mutation
    shadow_color.set(Color::rgba(1.0, 0.0, 0.0, 0.5));
    let (stats_shadow, _, _, _) = engine.frame();
    assert_eq!(stats_shadow.flags, DirtyFlags::PAINT);
    assert!(
        !stats_shadow.laid_out,
        "Scenario 1.2: Shadow color must be PAINT only"
    );
}

#[test]
fn test_1_3_transform_sets_layout_only_and_skips_measure() {
    let mut engine = Engine::headless(400, 400);
    let offset_x = engine.signal(0.0f32);
    let ox = offset_x.clone();
    let tx = computed(move || Transform::from_translation(ox.get(), 0.0));

    engine.mount(
        group()
            .width(200.0).height(200.0)
            .children([group().width(50.0).height(50.0).transform(tx)]),
    );
    engine.frame();

    offset_x.set(10.0);
    let (stats, _, _, _) = engine.frame();

    assert!(stats.flags.contains(DirtyFlags::LAYOUT));
    assert!(
        !stats.flags.contains(DirtyFlags::MEASURE),
        "Scenario 1.3: Transform skips measure"
    );
}

#[test]
fn test_1_4_text_content_change_triggers_measure_without_reconcile() {
    let mut engine = Engine::headless(400, 400);
    let content = engine.signal("5".to_string());
    engine.mount(group().width(200.0).height(200.0).children([text(content.clone())]));
    engine.frame();

    content.set("50".to_string());
    let (stats, _, _, _) = engine.frame();

    assert!(
        stats.flags.contains(DirtyFlags::MEASURE),
        "Scenario 1.4: Text change must MEASURE"
    );
    assert!(stats.laid_out);
}

#[test]
fn test_1_5_child_structural_mutation_forces_reconciliation() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group()
            .width(200.0).height(200.0)
            .children([group().width(50.0).height(50.0)]),
    );
    engine.frame();

    engine.mount(
        group()
            .width(200.0).height(200.0)
            .children([group().width(50.0).height(50.0), group().width(50.0).height(50.0)]),
    );
    let (stats, _, _, _) = engine.frame();

    assert!(
        stats.laid_out,
        "Scenario 1.5: Structural mutations must trigger layout"
    );
}

#[test]
fn test_1_6_identical_property_mutation_suppresses_invalidation() {
    let mut engine = Engine::headless(400, 400);
    let width = engine.signal(100.0f32);
    engine.mount(
        group()
            .width(400.0).height(400.0)
            .children([group().width(width.clone()).height(50.0)]),
    );
    engine.frame();

    width.set(100.0);
    let (stats, _, _, diag) = engine.frame();

    assert_eq!(
        stats.flags,
        DirtyFlags::NONE,
        "Scenario 1.6: No flags on same-value set"
    );
    assert_eq!(stats.nodes_dirtied, 0);
    assert!(!stats.laid_out);
    assert!(diag.spatial.damaged_rects.is_empty());
}
