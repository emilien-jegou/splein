// Single responsibility: Reactive child lists: reordering by key without re-running the component.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::{group, text, IntoElement};
use aurora::foundation::Color;
use aurora::prelude::*;
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::Engine;
use aurora_tiny_skia::HeadlessRaster;

/// Row height plus gap, so a slot index maps to a known y offset.
const STRIDE: f32 = 60.0;

/// A list whose children are rebuilt from a reactive order.
#[component]
fn Reorderable(order: Signal<Vec<&'static str>>) -> impl IntoElement {
    group()
        .key("list")
        .direction(Direction::Vertical)
        .width(300.0)
        .height(400.0)
        .gap(4.0)
        // Rows are keyed by label, so a swap reads as two rows trading places rather than
        // as every row being replaced. Keying by index would defeat the whole mechanism.
        .children_labels(order, |_, label| {
            group()
                .key(format!("row-{label}"))
                .direction(Direction::Vertical)
                .width(300.0)
                .height(56.0)
                .fill(Color::hex(0x2266AA))
                .children([text(label.to_string()).size(14.0).color(Color::WHITE)])
                .into_element()
        })
}

fn engine_with(order: Signal<Vec<&'static str>>) -> Engine {
    let runtime = order.runtime();
    let cx = Cx::new(Rc::clone(&runtime));
    let mut engine =
        Engine::with_runtime(runtime, cx.enter(|| Reorderable(order.clone())), 400, 400);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.frame();
    engine
}

/// Absolute y of a keyed row, in screen pixels.
fn row_y(engine: &Engine, label: &str) -> f32 {
    let id_key = format!("row-{label}");
    engine
        .rect(&id_key)
        .map(|r| r.y)
        .unwrap_or_else(|| panic!("no row {id_key}"))
}

#[test]
#[ntest::timeout(30_000)]
fn a_static_list_lays_out_in_order() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let order = Signal::new(Rc::clone(&runtime), vec!["a", "b", "c"]);
    let engine = engine_with(order);

    let mut ys: Vec<f32> = ["a", "b", "c"].iter().map(|l| row_y(&engine, l)).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(ys.len(), 3, "every row must be laid out");
    assert!(
        (ys[1] - ys[0] - STRIDE).abs() < 1.0,
        "rows must be one stride apart, got {ys:?}"
    );
    assert!((ys[2] - ys[1] - STRIDE).abs() < 1.0, "got {ys:?}");
}

#[test]
#[ntest::timeout(30_000)]
fn reordering_moves_the_rows_and_leaves_the_tree_intact() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let order = Signal::new(Rc::clone(&runtime), vec!["a", "b", "c"]);
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_with(order.clone());

    let before = row_y(&engine, "c");
    assert!(
        (before - 2.0 * STRIDE).abs() < 1.0,
        "c starts last, at {before}"
    );

    // Move c to the front.
    let mut next = order.get();
    let moved = next.remove(2);
    next.insert(0, moved);
    order.set(next);
    engine.frame();
    raster.draw_engine(&mut engine);

    let after = row_y(&engine, "c");
    assert!(
        after.abs() < 1.0,
        "c must now occupy the first slot, at {after}"
    );
    assert!(
        (row_y(&engine, "a") - STRIDE).abs() < 1.0,
        "a must have moved down one slot, at {}",
        row_y(&engine, "a")
    );
    assert!(
        (row_y(&engine, "b") - 2.0 * STRIDE).abs() < 1.0,
        "b must have moved down one slot too, at {}",
        row_y(&engine, "b")
    );
    // Every row still exists: a reorder must move nodes, never recreate the subtree.
    for label in ["a", "b", "c"] {
        assert!(
            engine.rect(format!("row-{label}")).is_some(),
            "{label} must survive the reorder"
        );
    }
}

#[test]
#[ntest::timeout(30_000)]
fn an_unchanged_order_costs_nothing() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let order = Signal::new(Rc::clone(&runtime), vec!["a", "b", "c"]);
    let mut engine = engine_with(order.clone());

    // Writing the identical value must not dirty the tree at all.
    order.set(vec!["a", "b", "c"]);
    let report = engine.frame();
    assert_eq!(
        report.diagnostics.layout.recomputed_nodes.len(),
        0,
        "an unchanged order must not re-lay anything out"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_reorder_leaves_the_render_boxes_settling() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let order = Signal::new(Rc::clone(&runtime), vec!["a", "b", "c"]);
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_with(order.clone());

    let mut next = order.get();
    let moved = next.remove(0);
    next.push(moved);
    order.set(next);
    engine.frame();
    raster.draw_engine(&mut engine);

    // No spring assertion here any more. The engine no longer animates a box, so "did anything
    // move" is not a question it can answer; the animation lives in whichever signal the client
    // animates. What this file still guards is that the swap is keyed correctly, asserted below
    // by the settled row positions.
    for _ in 0..180 {
        engine.advance(1.0 / 60.0);
        engine.frame();
        raster.draw_engine(&mut engine);
    }
    assert!(
        (row_y(&engine, "a") - 2.0 * STRIDE).abs() < 1.0,
        "a must have settled in its last slot, at {}",
        row_y(&engine, "a")
    );
}

#[test]
#[ntest::timeout(30_000)]
fn growing_and_shrinking_a_list_reconciles_the_difference() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let order = Signal::new(Rc::clone(&runtime), vec!["a", "b", "c"]);
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_with(order.clone());

    order.set(vec!["a", "c"]);
    engine.frame();
    raster.draw_engine(&mut engine);
    assert!(engine.rect("row-b").is_none(), "b must be unmounted");
    assert!(engine.rect("row-c").is_some(), "c must survive");

    order.set(vec!["a", "b", "c", "d"]);
    engine.frame();
    raster.draw_engine(&mut engine);
    for label in ["a", "b", "c", "d"] {
        assert!(
            engine.rect(format!("row-{label}")).is_some(),
            "{label} must be mounted"
        );
    }
    assert!(
        (row_y(&engine, "d") - 3.0 * STRIDE).abs() < 1.0,
        "d must land in the fourth slot, at {}",
        row_y(&engine, "d")
    );
}
