// Single responsibility: TDD verification for Stage 4 overlapping repaints, stacking contexts, and overlays.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::{Engine, FrameReport};

#[test]
fn test_4_1_underlying_mutation_forces_repaint_of_overlapping_clean_sibling() {
    let mut engine = Engine::headless(400, 400);
    let fill_a = engine.signal(Color::RED);

    engine.mount(group().direction(Direction::Horizontal).width(400.0).height(400.0).children([
        group().width(100.0).height(100.0).fill(fill_a.clone()),
        group().margin(Margin::left(-50.0)).width(100.0).height(100.0).fill(Color::BLUE),
    ]));
    engine.frame();

    fill_a.set(Color::GREEN);
    engine.frame();

    let canvas = engine.canvas();
    let overlap_pixel = canvas.pixel(75, 50).unwrap();
    assert_eq!(overlap_pixel.blue(), 255, "Scenario 4.1: Top sibling must paint over lower node");
}

#[test]
fn test_4_3_repetitive_damage_repaints_do_not_progressively_darken() {
    let mut engine = Engine::headless(400, 400);
    let fill_a = engine.signal(Color::WHITE);

    engine.mount(group().width(400.0).height(400.0).children([
        group().width(200.0).height(200.0).fill(fill_a.clone()),
        group()
            .width(100.0).height(100.0)
            .shadows([Shadow::outer(0.0, 10.0, 5.0, Color::rgba(0.0, 0.0, 0.0, 0.5))]),
    ]));
    engine.frame();

    let initial_alpha = engine.canvas().pixel(50, 105).unwrap().alpha();

    for i in 0..20 {
        let val = if i % 2 == 0 { Color::WHITE } else { Color::rgb(0.99, 0.99, 0.99) };
        fill_a.set(val);
        engine.frame();
    }

    let end_alpha = engine.canvas().pixel(50, 105).unwrap().alpha();
    assert_eq!(initial_alpha, end_alpha, "Scenario 4.3: Alpha must not accumulate over frames");
}

#[test]
fn test_4_5_distant_clean_node_is_excluded_from_repaint() {
    let mut engine = Engine::headless(800, 400);
    let color_a = engine.signal(Color::RED);

    engine.mount(group().direction(Direction::Horizontal).width(800.0).height(400.0).children([
        group().width(100.0).height(100.0).fill(color_a.clone()),
        group().margin(Margin::left(400.0)).width(100.0).height(100.0).fill(Color::BLUE),
    ]));
    engine.frame();

    color_a.set(Color::GREEN);
    let FrameReport { diagnostics: diag, .. } = engine.frame();

    let box_c_rect = ResolvedRect::new(500.0, 0.0, 100.0, 100.0);
    assert!(
        !diag.spatial.damaged_rects.iter().any(|r| r.intersects(&box_c_rect)),
        "Scenario 4.5: Distant clean node must not be included in damage"
    );
}

#[test]
fn test_4_6_equal_z_index_resolved_by_tree_order() {
    let mut engine = Engine::headless(200, 200);

    engine.mount(group().width(200.0).height(200.0).children([
        group().width(100.0).height(100.0).fill(Color::RED).z_index(10),
        group().width(100.0).height(100.0).fill(Color::BLUE).z_index(10),
    ]));
    engine.frame();

    let canvas = engine.canvas();
    let pixel = canvas.pixel(50, 50).expect("Pixel must exist");
    assert_eq!(pixel.blue(), 255, "Scenario 4.6: Equal z-index resolved by DOM order");
    assert_eq!(pixel.red(), 0);
}

#[test]
fn test_4_7_overlay_is_drawn_above_base_tree_regardless_of_dom_order() {
    let mut engine = Engine::headless(400, 400);

    engine.mount(group().width(400.0).height(400.0).children([
        group().width(100.0).height(100.0).fill(Color::BLUE).overlay(true).z_index(100),
        group().width(200.0).height(200.0).fill(Color::RED),
    ]));
    engine.frame();

    let canvas = engine.canvas();
    let top_pixel = canvas.pixel(50, 50).unwrap();
    assert_eq!(top_pixel.blue(), 255, "Scenario 4.7: Overlay must render above standard flow");
}

#[test]
fn test_4_8_absolute_overlay_over_nested_boxes() {
    let mut engine = Engine::headless(500, 500);

    engine.mount(group().width(500.0).height(500.0).children([
        group().width(300.0).height(300.0).fill(Color::RED).children([
            group().margin(Margin::sides(50.0, 50.0, 50.0, 50.0)).width(100.0).height(100.0).fill(Color::GREEN),
        ]),
        group().absolute().anchor(Anchor::TopLeft).width(200.0).height(200.0).fill(Color::BLUE).overlay(true).z_index(100),
    ]));
    engine.frame();

    let canvas = engine.canvas();
    let pixel = canvas.pixel(75, 75).unwrap();
    assert_eq!(pixel.blue(), 255, "Absolute overlay covers nested green box");
}
