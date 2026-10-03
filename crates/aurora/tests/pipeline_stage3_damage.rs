// Single responsibility: TDD verification for Stage 3 geometric damage, expansion, and merge limits.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::reactive::*;
use aurora::runtime::{Engine, FrameReport};

#[test]
fn test_3_1_moved_node_damages_both_old_and_new_positions() {
    let mut engine = Engine::headless(600, 400);
    let offset_x = engine.signal(0.0f32);
    let ox = offset_x.clone();
    let tx = computed(move || Transform::from_translation(ox.get(), 0.0));

    engine.mount(group().width(600.0).height(400.0).children([
        group().transform(tx).width(50.0).height(50.0).fill(Color::RED),
    ]));
    engine.frame();

    offset_x.set(10.0);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    let old_rect = ResolvedRect::new(0.0, 0.0, 50.0, 50.0);
    let new_rect = ResolvedRect::new(10.0, 0.0, 50.0, 50.0);

    assert!(diag.spatial.damaged_rects.iter().any(|r| r.intersects(&old_rect)));
    assert!(diag.spatial.damaged_rects.iter().any(|r| r.intersects(&new_rect)));
}

#[test]
fn test_3_2_removed_node_populates_removed_rects_and_damages_old_bounds() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(group().width(400.0).height(400.0).children([
        group().width(80.0).height(80.0).fill(Color::BLUE),
    ]));
    engine.frame();

    // Frame 2: Unmount child
    engine.mount(group().width(400.0).height(400.0));
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert_eq!(diag.spatial.removed_rects.len(), 1, "Scenario 3.2: Removed rect captured");
    assert_eq!(diag.spatial.removed_rects[0], ResolvedRect::new(0.0, 0.0, 80.0, 80.0));

    // Frame 3: Verify cleared on next frame
    let FrameReport { diagnostics: diag_next, .. } = engine.frame();
    assert!(
        diag_next.spatial.removed_rects.is_empty(),
        "Scenario 3.2: removed_rects must be cleared on next frame"
    );
}

#[test]
fn test_3_3_inserted_node_damages_new_bounds() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(group().width(400.0).height(400.0));
    engine.frame();

    engine.mount(group().width(400.0).height(400.0).children([
        group().margin(Margin::sides(20.0, 20.0, 20.0, 20.0)).width(60.0).height(60.0).fill(Color::GREEN),
    ]));
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    let expected_rect = ResolvedRect::new(20.0, 20.0, 60.0, 60.0);
    assert!(
        diag.spatial.damaged_rects.iter().any(|r| r.intersects(&expected_rect)),
        "Scenario 3.3: Inserted node must damage its new bounds"
    );
}

#[test]
fn test_3_4_shadow_bounds_reach_farther_in_offset_direction() {
    let mut engine = Engine::headless(500, 500);
    let fill = engine.signal(Color::RED);

    engine.mount(group().width(500.0).height(500.0).children([
        group()
            .margin(Margin::sides(100.0, 100.0, 100.0, 100.0))
            .width(50.0).height(50.0)
            .fill(fill.clone())
            .shadows([Shadow::outer(0.0, 4.0, 12.0, Color::BLACK)]),
    ]));
    engine.frame();

    fill.set(Color::BLUE);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    let d = diag.spatial.damaged_rects[0];
    assert!(d.y < 100.0, "Shadow reaches upwards");
    assert!(d.bottom() > 154.0, "Shadow reaches further downward due to offset_y=4");
}

#[test]
fn test_3_5_two_layouts_before_one_paint_retains_true_last_painted_origin() {
    let mut engine = Engine::headless(600, 400);
    let offset_x = engine.signal(0.0f32);
    let ox = offset_x.clone();
    let tx = computed(move || Transform::from_translation(ox.get(), 0.0));

    engine.mount(group().width(600.0).height(400.0).children([
        group().transform(tx).width(50.0).height(50.0).fill(Color::RED),
    ]));
    engine.frame();

    offset_x.set(50.0);
    offset_x.set(100.0);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    let origin_rect = ResolvedRect::new(0.0, 0.0, 50.0, 50.0);
    assert!(diag.spatial.damaged_rects.iter().any(|r| r.intersects(&origin_rect)));
}

#[test]
fn test_3_8_five_disjoint_changes_cluster_into_at_most_four_rects() {
    let mut engine = Engine::headless(1000, 1000);
    let sigs: Vec<_> = (0..5).map(|_| engine.signal(Color::RED)).collect();

    engine.mount(group().direction(Direction::Horizontal).width(1000.0).height(200.0).children([
        group().width(50.0).height(50.0).fill(sigs[0].clone()),
        group().margin(Margin::left(50.0)).width(50.0).height(50.0).fill(sigs[1].clone()),
        group().margin(Margin::left(50.0)).width(50.0).height(50.0).fill(sigs[2].clone()),
        group().margin(Margin::left(50.0)).width(50.0).height(50.0).fill(sigs[3].clone()),
        group().margin(Margin::left(50.0)).width(50.0).height(50.0).fill(sigs[4].clone()),
    ]));
    engine.frame();

    for s in &sigs {
        s.set(Color::BLUE);
    }
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    assert!(
        diag.spatial.damaged_rects.len() <= 4,
        "Scenario 3.8: Must enforce <= 4 rects via least-added-area merge"
    );
}
