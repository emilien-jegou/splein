// Single responsibility: Headless guards that squeezed or resized viewports never erase shaped text.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::render::TinySkiaRenderer;
use aurora::runtime::Engine;
use aurora::scene::{Scene, SceneChunk, SceneCommand};
use aurora::text::{TextConfig, TextContext};

/// Counts the shaped text runs present in the compiled display list.
fn text_runs(engine: &Engine) -> usize {
    engine
        .scene()
        .chunks
        .iter()
        .flat_map(|chunk| chunk.commands.iter())
        .filter(|cmd| matches!(cmd, SceneCommand::DrawText { .. }))
        .count()
}

/// Line count of the first emitted text run, or None when the display list holds no run.
fn first_run_lines(engine: &Engine) -> Option<usize> {
    engine
        .scene()
        .chunks
        .iter()
        .flat_map(|chunk| chunk.commands.iter())
        .find_map(|cmd| match cmd {
            SceneCommand::DrawText { layout, .. } => Some(layout.lines.len()),
            _ => None,
        })
}

#[test]
fn collapsed_text_box_still_paints_its_run() {
    let mut engine = Engine::headless(400, 200);
    // The fill sibling claims every pixel of width, so the label's box collapses to zero area.
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(400.0)
            .height(200.0)
            .children((
                group().width(Size::Fill).height(200.0),
                text("Resilient label").size(16.0),
            )),
    );
    engine.frame();
    assert_eq!(text_runs(&engine), 1, "baseline: label paints at full size");

    for width in [30u32, 12, 4, 1] {
        engine.resize(width, 20);
        engine.frame();
        assert!(
            text_runs(&engine) >= 1,
            "a collapsed box must not drop its subtree from the display list (width {width})"
        );
        assert_eq!(
            first_run_lines(&engine),
            Some(1),
            "a squeezed box must not shard the run into a one-glyph-per-line rail (width {width})"
        );
    }
}

#[test]
fn text_survives_shrink_and_grow_cycles() {
    let mut engine = Engine::headless(300, 200);
    engine.mount(
        group()
            .width(Size::Fill)
            .height(Size::Fill)
            .children([text("Resize me please").size(18.0)]),
    );
    engine.frame();
    assert_eq!(text_runs(&engine), 1, "baseline before any resize");

    for width in [1u32, 2, 240, 12, 300, 1] {
        engine.resize(width, 200);
        engine.frame();
        assert_eq!(
            text_runs(&engine),
            1,
            "text must persist through every viewport in the cycle, failed at width {width}"
        );
    }
}

/// Widest painted decoration box in the display list, i.e. the largest resolved content box.
fn widest_painted_box(engine: &Engine) -> f32 {
    engine
        .scene()
        .chunks
        .iter()
        .flat_map(|chunk| chunk.commands.iter())
        .filter_map(|cmd| match cmd {
            SceneCommand::DrawRect { rect, .. } => Some(rect.width),
            _ => None,
        })
        .fold(0.0_f32, f32::max)
}

#[test]
fn squeezed_box_floors_at_min_content_instead_of_zero() {
    let mut engine = Engine::headless(20, 200);
    // Two competing content-bearing siblings in a 20px row: the shrink path, not the overflow path.
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(20.0)
            .height(200.0)
            .children((
                group()
                    .width(Size::Fit)
                    .height(20.0)
                    .fill(Color::WHITE)
                    .children([text("Resilient label").size(16.0)]),
                group()
                    .width(Size::Fit)
                    .height(20.0)
                    .fill(Color::WHITE)
                    .children([text("Another caption").size(16.0)]),
            )),
    );
    engine.frame();

    let floor = widest_painted_box(&engine);
    assert!(
        floor > 30.0,
        "a starved sibling must not crush a text box below its widest word, got {floor}"
    );
}

#[test]
fn min_content_floor_never_inflates_a_box_past_its_desire() {
    let mut engine = Engine::headless(6, 200);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(6.0)
            .height(200.0)
            .children((
                group()
                    .width(Size::Fixed(8.0))
                    .height(20.0)
                    .fill(Color::WHITE)
                    .children([text("far wider than eight pixels").size(16.0)]),
                group()
                    .width(Size::Fixed(8.0))
                    .height(20.0)
                    .fill(Color::WHITE)
                    .children([text("also far wider").size(16.0)]),
            )),
    );
    engine.frame();

    let widest = widest_painted_box(&engine);
    assert!(
        widest > 0.0 && widest <= 8.01,
        "an explicit fixed size must stay authoritative over the content floor, got {widest}"
    );
}

/// Absolute x of the first painted text run, accumulated from the display list offset stack.
fn first_run_x(engine: &Engine) -> Option<f32> {
    for chunk in engine.scene().chunks.iter() {
        let mut x = 0.0_f32;
        let mut stack: Vec<f32> = Vec::new();
        for cmd in chunk.commands.iter() {
            match cmd {
                SceneCommand::PushOffset(p) => {
                    stack.push(p.x);
                    x += p.x;
                }
                SceneCommand::PopOffset => {
                    if let Some(popped) = stack.pop() {
                        x -= popped;
                    }
                }
                SceneCommand::DrawText { .. } => return Some(x),
                _ => {}
            }
        }
    }
    None
}

#[test]
fn children_of_a_stacking_context_are_not_offset_twice() {
    let mut engine = Engine::headless(400, 200);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(400.0)
            .height(200.0)
            .children((
                group().width(Size::Fixed(200.0)).height(200.0),
                group()
                    .opacity(0.95)
                    .width(Size::Fit)
                    .height(20.0)
                    .children([text("hello")]),
            )),
    );
    engine.frame();

    assert_eq!(
        first_run_x(&engine),
        Some(200.0),
        "a run inside a stacking context sits at the context origin, not origin plus its own offset"
    );
}

#[test]
fn a_clipped_item_may_still_be_crushed_below_its_content() {
    let mut engine = Engine::headless(20, 200);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(20.0)
            .height(200.0)
            .children((
                group()
                    .clip(true)
                    .width(Size::Fit)
                    .height(20.0)
                    .fill(Color::WHITE)
                    .children([text("first clipped word").size(16.0)]),
                group()
                    .clip(true)
                    .width(Size::Fit)
                    .height(20.0)
                    .fill(Color::WHITE)
                    .children([text("second clipped word").size(16.0)]),
            )),
    );
    engine.frame();

    assert!(
        widest_painted_box(&engine) <= 20.0,
        "clip(true) is the author's opt-out of the automatic minimum, so siblings may be crushed",
    );
}

#[test]
fn offscreen_text_is_still_culled() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(
        group().width(200.0).height(200.0).children([group()
            .absolute()
            .anchor(Anchor::TopLeft)
            .width(60.0)
            .height(60.0)
            .margin(Margin::sides(5_000.0, 0.0, 0.0, 0.0))
            .children([text("below the fold")])]),
    );
    engine.frame();
    assert_eq!(
        text_runs(&engine),
        0,
        "visibility resilience must not disable viewport culling"
    );
}

#[test]
fn degenerate_chunk_still_rasterizes_its_content() {
    let mut renderer = TinySkiaRenderer::new();
    renderer.set_text_context(TextContext::new());
    renderer.resize(120, 40);

    let mut scene = Scene::new();
    let mut chunk = SceneChunk::new(ResolvedRect::ZERO, Point::ZERO);
    chunk.push(SceneCommand::DrawRect {
        rect: ResolvedRect::new(0.0, 0.0, 100.0, 30.0),
        appearance: Appearance::EMPTY.with_fill(Fill::Solid(Color::WHITE)),
    });
    scene.push_chunk(chunk);

    let mut damage = DamageRegion::new();
    damage.push(ResolvedRect::new(0.0, 0.0, 120.0, 40.0));
    renderer
        .render_damage(&scene, &damage)
        .expect("rasterization of a collapsed chunk must succeed");

    assert!(
        renderer.canvas().data().iter().any(|byte| *byte != 0),
        "a chunk whose bounds collapsed to zero area must still rasterize the content it overflows"
    );
}

#[test]
fn empty_text_content_keeps_a_measurable_line_box() {
    let mut config = TextConfig::default();
    config.content = String::new();
    config.size = 20.0;
    let layout = TextContext::new().shape_config(&config, Constraints::loose(80.0, f32::INFINITY));

    assert!(
        layout.total_size.height > 0.0,
        "empty content must still reserve a line box so text reappears when typed into"
    );
    assert!(
        layout.lines.len() <= 1,
        "empty content must not inflate into a multi-line run"
    );
}
