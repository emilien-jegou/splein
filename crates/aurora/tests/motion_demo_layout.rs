// Single responsibility: Verify no motion demo page lays out or paints outside its viewport.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::IntoElement;
use aurora::foundation::ResolvedRect;
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::Engine;

#[allow(dead_code)]
#[path = "../examples/motion_accordion/main.rs"]
mod accordion;
#[allow(dead_code)]
#[path = "../examples/motion_imperative.rs"]
mod imperative;
#[allow(dead_code)]
#[path = "../examples/motion_interruption.rs"]
mod interruption;
#[allow(dead_code)]
#[path = "../examples/motion_layout_split.rs"]
mod layout_split;
#[allow(dead_code)]
#[path = "../examples/motion_playback/main.rs"]
mod playback;
#[allow(dead_code)]
#[path = "../examples/motion_press.rs"]
mod press;
#[allow(dead_code)]
#[path = "../examples/motion_reorder.rs"]
mod reorder;
#[allow(dead_code)]
#[path = "../examples/motion_signals.rs"]
mod signals;
#[allow(dead_code)]
#[path = "../examples/motion_transitions.rs"]
mod transitions;
#[allow(dead_code)]
#[path = "../examples/motion_validate.rs"]
mod validate;

/// Page background the shared shell leaves in its margins.
const PAGE: (i16, i16, i16) = (244, 245, 247);
/// Content inset declared by the shared page shell.
const MARGIN: u32 = 24;

/// Builds a fonted engine for a page that owns no external signals.
fn engine_for(root: impl IntoElement, w: u32, h: u32) -> Engine {
    let mut engine = Engine::new(root.into_element(), w, h);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine
}

/// Builds a fonted engine over a runtime the page's signals already belong to.
fn engine_over(
    runtime: Rc<RefCell<ReactiveRuntime>>,
    root: impl IntoElement,
    w: u32,
    h: u32,
) -> Engine {
    let mut engine = Engine::with_runtime(runtime, root.into_element(), w, h);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine
}

/// Runs the page's first frame and asserts no rect and no pixel leaves the viewport.
fn fits(name: &str, engine: &mut Engine, w: u32, h: u32) {
    let report = engine.frame();
    let rects: Vec<ResolvedRect> = report.diagnostics.layout.recomputed_nodes;

    assert!(
        rects.len() >= 12,
        "{name}: captured only {} rects, so the probe is blind",
        rects.len()
    );
    assert!(
        rects
            .iter()
            .any(|r| r.x == 0.0 && r.y == 0.0 && r.width == w as f32 && r.height == h as f32),
        "{name}: the page root rect was not captured"
    );
    for rect in &rects {
        assert!(
            rect.x >= -0.5 && rect.y >= -0.5,
            "{name}: rect starts outside the viewport: {rect:?}"
        );
        assert!(
            rect.right() <= w as f32 + 0.5 && rect.bottom() <= h as f32 + 0.5,
            "{name}: rect spills past {w}x{h}: {rect:?}"
        );
    }

    // Paint: the shell clips content to its box, so its own margins must stay pure background.
    let content_top = rects
        .iter()
        .find(|r| (r.height - 1.0).abs() < 0.01 && (r.width - w as f32).abs() < 0.01)
        .expect("page hairline was not captured")
        .bottom() as u32;

    let canvas = engine.canvas();
    let check = |x: u32, y: u32| {
        let p = canvas.pixel(x, y).expect("pixel must exist");
        let (r, g, b) = (p.red() as i16, p.green() as i16, (p.blue() as i16));
        let clean = (r - PAGE.0).abs() <= 1 && (g - PAGE.1).abs() <= 1 && (b - PAGE.2).abs() <= 1;
        assert!(
            clean,
            "{name}: painted outside the content box at ({x},{y}) -> r{r} g{g} b{b}"
        );
    };

    for y in content_top..h {
        for x in (w - MARGIN)..w {
            check(x, y);
        }
    }
    for y in (h - MARGIN)..h {
        for x in 0..w {
            check(x, y);
        }
    }
}

#[test]
fn layout_split_page_fits_its_viewport() {
    let mut engine = engine_for(
        layout_split::build_ui(),
        layout_split::WIDTH,
        layout_split::HEIGHT,
    );
    fits("layout_split", &mut engine, layout_split::WIDTH, layout_split::HEIGHT);
}

#[test]
fn transitions_page_fits_its_viewport() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let enabled = Signal::new(Rc::clone(&runtime), false);
    let mut engine = engine_over(
        runtime,
        transitions::build_ui(enabled),
        transitions::WIDTH,
        transitions::HEIGHT,
    );
    fits("transitions", &mut engine, transitions::WIDTH, transitions::HEIGHT);
}

#[test]
fn imperative_page_fits_its_viewport() {
    let mut engine = engine_for(
        imperative::build_ui(),
        imperative::WIDTH,
        imperative::HEIGHT,
    );
    fits("imperative", &mut engine, imperative::WIDTH, imperative::HEIGHT);
}

#[test]
fn interruption_page_fits_its_viewport() {
    let mut engine = engine_for(
        interruption::build_ui(),
        interruption::WIDTH,
        interruption::HEIGHT,
    );
    fits("interruption", &mut engine, interruption::WIDTH, interruption::HEIGHT);
}

#[test]
fn playback_page_fits_its_viewport() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let status = Signal::new(Rc::clone(&runtime), String::from("idle"));
    let mut engine = engine_over(
        runtime,
        playback::build_ui(status),
        playback::WIDTH,
        playback::HEIGHT,
    );
    fits("playback", &mut engine, playback::WIDTH, playback::HEIGHT);
}

#[test]
fn signals_page_fits_its_viewport() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let progress = Signal::new(Rc::clone(&runtime), 0.0f32);
    let mut engine = engine_over(
        runtime,
        signals::build_ui(progress),
        signals::WIDTH,
        signals::HEIGHT,
    );
    fits("signals", &mut engine, signals::WIDTH, signals::HEIGHT);
}

#[test]
fn press_page_fits_its_viewport() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let presses = Signal::new(Rc::clone(&runtime), 0u32);
    let mut engine = engine_over(
        runtime,
        press::build_ui(presses),
        press::WIDTH,
        press::HEIGHT,
    );
    fits("press", &mut engine, press::WIDTH, press::HEIGHT);
}

#[test]
fn accordion_page_fits_its_viewport() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(0usize));
    let mut engine = engine_over(
        runtime,
        accordion::build_ui(open),
        accordion::WIDTH,
        accordion::HEIGHT,
    );
    fits("accordion", &mut engine, accordion::WIDTH, accordion::HEIGHT);
}

#[test]
fn reorder_page_fits_its_viewport() {
    let mut engine = engine_for(
        reorder::build_ui(&reorder::ITEMS),
        reorder::WIDTH,
        reorder::HEIGHT,
    );
    fits("reorder", &mut engine, reorder::WIDTH, reorder::HEIGHT);
}

#[test]
fn validate_page_fits_its_viewport() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let width = Signal::new(Rc::clone(&runtime), 52.0f32);
    let mut engine = engine_over(
        runtime,
        validate::build_ui(width),
        validate::WIDTH,
        validate::HEIGHT,
    );
    fits("validate", &mut engine, validate::WIDTH, validate::HEIGHT);
}
