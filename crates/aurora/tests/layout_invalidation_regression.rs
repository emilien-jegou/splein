// Single responsibility: Regression tests for layout invalidation and sizing below a fill-sized root.

#[allow(dead_code)]
#[path = "../examples/motion_accordion/main.rs"]
mod accordion;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::{Engine, FrameReport};

const RED: u32 = 0xEF4444;
const BLUE: u32 = 0x3B82F6;
/// Height of an accordion item while its panel is revealed.
const OPEN: f32 = 176.0;
/// Height of an accordion item while its panel is hidden.
const CLOSED: f32 = 56.0;

/// Runs one frame and rasterizes its damage into the retained canvas.
fn step(engine: &mut Engine) -> FrameReport {
    let report = engine.frame();
    engine.canvas();
    report
}

/// Applies `change` plus one frame, returning how many canvas pixels the repaint moved.
fn repaint_delta<F: FnOnce()>(engine: &mut Engine, change: F) -> (u32, FrameReport) {
    let (w, h) = engine.logical_size();
    let before = engine.canvas().clone();
    change();
    let report = engine.frame();
    let after = engine.canvas();
    let mut changed = 0u32;
    for y in 0..h {
        for x in 0..w {
            if before.pixel(x, y) != after.pixel(x, y) {
                changed += 1;
            }
        }
    }
    (changed, report)
}

/// A page root and inner column that both fill the window: no fixed-size ancestor exists.
fn page(height: Signal<f32>) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .children([group()
            .direction(Direction::Vertical)
            .width(Size::fill())
            .height(Size::fill())
            .children((
                group().key("box").width(Size::fill()).height(height).fill(Color::hex(RED)),
                group().key("tail").width(Size::fill()).height(50.0).fill(Color::hex(BLUE)),
            ))])
}

/// Builds the accordion demo mounted at its own window size, with one panel open.
fn accordion_engine(open_index: usize) -> (Engine, Signal<Option<usize>>) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(open_index));
    let mut engine = Engine::with_runtime(
        runtime,
        accordion::build_ui(open.clone()).into_element(),
        accordion::WIDTH,
        accordion::HEIGHT,
    );
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    (engine, open)
}

#[test]
fn a_height_change_below_a_fill_root_repaints_without_a_resize() {
    let mut engine = Engine::headless(400, 400);
    let height = engine.signal(40.0f32);
    engine.mount(page(height.clone()));
    step(&mut engine);

    let (changed, report) = repaint_delta(&mut engine, || height.set(120.0));

    assert!(
        !report.diagnostics.layout.recomputed_nodes.is_empty(),
        "the root must lay out again even though no ancestor is a layout boundary"
    );
    assert_eq!(engine.rect("box").expect("keyed box").height, 120.0, "the bound height must apply");
    assert_eq!(engine.rect("tail").expect("keyed tail").y, 120.0, "the sibling must follow it");
    assert!(
        changed > 0,
        "a height change must repaint on its own, got {changed} changed pixels"
    );
}

#[test]
fn accordion_items_keep_their_declared_heights() {
    for open_index in 0..4usize {
        let (mut engine, _open) = accordion_engine(open_index);
        step(&mut engine);

        for index in 0..4usize {
            let expected = if index == open_index { OPEN } else { CLOSED };
            let rect = engine.rect(format!("panel-{index}")).expect("keyed panel");
            assert_eq!(
                rect.height, expected,
                "panel {index} must keep its declared height with {open_index} open"
            );
        }
    }
}
