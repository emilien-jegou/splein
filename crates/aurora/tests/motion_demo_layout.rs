// Single responsibility: Verify no motion demo page lays out or paints outside its viewport.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::IntoElement;
use aurora::foundation::ResolvedRect;
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::Engine;
use aurora_tiny_skia::HeadlessRaster;

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

/// Builds a fonted engine for a page written as a `#[component]`.
///
/// Components run inside the ambient hook scope, so the harness stands one up the same way
/// `App::run` does before calling the page.
fn engine_for_page(page: fn() -> aurora::dsl::Element, w: u32, h: u32) -> Engine {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = aurora::reactive::Cx::new(Rc::clone(&runtime));
    engine_over(Rc::clone(&runtime), cx.enter(page), w, h)
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
fn fits(name: &str, engine: &mut Engine, raster: &mut HeadlessRaster, w: u32, h: u32) {
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

    let canvas = raster.draw_engine(&mut *engine);
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
#[ntest::timeout(5_000)]
fn layout_split_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(
        layout_split::Page,
        layout_split::WIDTH,
        layout_split::HEIGHT,
    );
    fits(
        "layout_split",
        &mut engine,
        &mut raster,
        layout_split::WIDTH,
        layout_split::HEIGHT,
    );
}

#[test]
#[ntest::timeout(5_000)]
fn transitions_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(transitions::Page, transitions::WIDTH, transitions::HEIGHT);
    fits(
        "transitions",
        &mut engine,
        &mut raster,
        transitions::WIDTH,
        transitions::HEIGHT,
    );
}

#[test]
#[ntest::timeout(5_000)]
fn imperative_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(imperative::Page, imperative::WIDTH, imperative::HEIGHT);
    fits(
        "imperative",
        &mut engine,
        &mut raster,
        imperative::WIDTH,
        imperative::HEIGHT,
    );
}

#[test]
#[ntest::timeout(5_000)]
fn interruption_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(
        interruption::Page,
        interruption::WIDTH,
        interruption::HEIGHT,
    );
    fits(
        "interruption",
        &mut engine,
        &mut raster,
        interruption::WIDTH,
        interruption::HEIGHT,
    );
}

#[test]
#[ntest::timeout(5_000)]
fn playback_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(playback::Page, playback::WIDTH, playback::HEIGHT);
    fits("playback", &mut engine, &mut raster, playback::WIDTH, playback::HEIGHT);
}

#[test]
#[ntest::timeout(5_000)]
fn signals_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(signals::Page, signals::WIDTH, signals::HEIGHT);
    fits("signals", &mut engine, &mut raster, signals::WIDTH, signals::HEIGHT);
}

#[test]
#[ntest::timeout(5_000)]
fn press_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(press::Page, press::WIDTH, press::HEIGHT);
    fits("press", &mut engine, &mut raster, press::WIDTH, press::HEIGHT);
}

#[test]
#[ntest::timeout(5_000)]
fn accordion_page_fits_its_viewport() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(0usize));
    let cx = aurora::reactive::Cx::new(Rc::clone(&runtime));
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_over(
        runtime,
        cx.enter(|| accordion::Page(open)),
        accordion::WIDTH,
        accordion::HEIGHT,
    );
    fits(
        "accordion",
        &mut engine,
        &mut raster,
        accordion::WIDTH,
        accordion::HEIGHT,
    );
}

#[test]
#[ntest::timeout(5_000)]
fn reorder_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(reorder::Page, reorder::WIDTH, reorder::HEIGHT);
    fits("reorder", &mut engine, &mut raster, reorder::WIDTH, reorder::HEIGHT);
}

#[test]
#[ntest::timeout(5_000)]
fn validate_page_fits_its_viewport() {
    let mut raster = HeadlessRaster::new();
    let mut engine = engine_for_page(validate::Page, validate::WIDTH, validate::HEIGHT);
    fits("validate", &mut engine, &mut raster, validate::WIDTH, validate::HEIGHT);
}
