// Single responsibility: Headless smoke tests for the text shaping pipeline.

use aurora::dsl::{group, text};
use aurora::foundation::Constraints;
use aurora::foundation::{Direction, Margin, Point, Size};
use aurora::runtime::Engine;
use aurora::scene::SceneCommand;
use aurora::text::decoration::decoration_rules;
use aurora::text::{LineHeight, TextAlign, TextConfig, TextContext, TextDecoration, TextOverflow};
use std::sync::Arc;

const LOREM: &str = "Aurora wraps this paragraph to the box width and keeps the rhythm consistent as the type scales across the interface.";

fn context() -> TextContext {
    let ctx = TextContext::new();
    ctx.load_font(include_bytes!("../assets/inter-var.ttf"));
    ctx
}

fn config(content: &str) -> TextConfig {
    TextConfig {
        content: content.to_string(),
        size: 16.0,
        ..TextConfig::default()
    }
}

/// Absolute origin, max line width, and total height of every emitted text run.
fn draw_runs(engine: &Engine) -> Vec<(Point, f32, f32)> {
    let mut runs = Vec::new();
    for chunk in &engine.scene().chunks {
        let mut offset = chunk.abs_origin;
        let mut stack: Vec<Point> = Vec::new();
        for cmd in &chunk.commands {
            match cmd {
                SceneCommand::PushOffset(p) => {
                    stack.push(*p);
                    offset = Point::new(offset.x + p.x, offset.y + p.y);
                }
                SceneCommand::PopOffset => {
                    if let Some(p) = stack.pop() {
                        offset = Point::new(offset.x - p.x, offset.y - p.y);
                    }
                }
                SceneCommand::DrawText { origin, layout, .. } => runs.push((
                    Point::new(offset.x + origin.x, offset.y + origin.y),
                    layout.total_size.width,
                    layout.total_size.height,
                )),
                _ => {}
            }
        }
    }
    runs
}

#[test]
fn missing_weight_face_does_not_panic() {
    let ctx = context();
    let mut cfg = config("Bold heading");
    cfg.family = Some("Inter".into());
    cfg.weight = 700;
    let layout = ctx.shape_config(&cfg, Constraints::loose(400.0, f32::INFINITY));
    assert!(!layout.lines.is_empty());
}

#[test]
fn italic_without_italic_face_does_not_panic() {
    let ctx = context();
    let mut cfg = config("Emphasis");
    cfg.family = Some("Inter".into());
    cfg.style = aurora::text::FontStyle::Italic;
    let layout = ctx.shape_config(&cfg, Constraints::loose(400.0, f32::INFINITY));
    assert!(!layout.lines.is_empty());
}

#[test]
fn wraps_at_constrained_width() {
    let ctx = context();
    let layout = ctx.shape_config(&config(LOREM), Constraints::loose(160.0, f32::INFINITY));
    assert!(layout.lines.len() > 1, "expected wrapping, got one line");
}

#[test]
fn center_alignment_offsets_line_origin() {
    let ctx = context();
    let mut cfg = config("Hi");
    cfg.align = TextAlign::Center;
    let layout = ctx.shape_config(&cfg, Constraints::loose(400.0, f32::INFINITY));
    let offset = layout.lines[0].align_offset;
    assert!(offset > 0.0, "centered line should carry a positive offset");
}

#[test]
fn ellipsis_clamps_to_line_budget() {
    let ctx = context();
    let mut cfg = config(LOREM);
    cfg.line_height = LineHeight::Multiple(1.4);
    cfg.max_lines = Some(2);
    cfg.overflow = TextOverflow::Ellipsis;
    let layout = ctx.shape_config(&cfg, Constraints::loose(160.0, f32::INFINITY));
    assert_eq!(layout.lines.len(), 2, "ellipsis should clamp to max_lines");
}

#[test]
fn decoration_produces_underline_rule() {
    let ctx = context();
    let layout = ctx.shape_config(
        &config("Underlined"),
        Constraints::loose(400.0, f32::INFINITY),
    );
    let rules = decoration_rules(&layout.lines[0], 16.0, TextDecoration::UNDERLINE);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].width > 0.0);
}

#[test]
fn fixed_width_text_wraps_at_that_width_in_a_vertical_group() {
    let mut engine = Engine::headless(760, 980);
    engine.mount(
        group()
            .direction(Direction::Vertical)
            .width(Size::fill())
            .height(Size::fill())
            .children([text(LOREM).size(15.0).width(560.0)]),
    );
    engine.frame();

    let mut checked = false;
    for chunk in &engine.scene().chunks {
        for cmd in &chunk.commands {
            if let SceneCommand::DrawText { layout, .. } = cmd {
                for line in &layout.lines {
                    assert!(
                        line.width <= 561.0,
                        "wrapped line {} exceeds 560",
                        line.width
                    );
                }
                checked = true;
            }
        }
    }
    assert!(checked, "no text was emitted");
}

#[test]
fn fill_width_text_wraps_within_parent_minus_margins() {
    let mut engine = Engine::headless(760, 980);
    engine.mount(
        group()
            .direction(Direction::Vertical)
            .width(Size::fill())
            .height(Size::fill())
            .children([text(LOREM)
                .size(15.0)
                .width(Size::fill())
                .margin(Margin::x(48.0))]),
    );
    engine.frame();

    let mut checked = false;
    for chunk in &engine.scene().chunks {
        for cmd in &chunk.commands {
            if let SceneCommand::DrawText { layout, .. } = cmd {
                for line in &layout.lines {
                    assert!(
                        line.width <= 665.0,
                        "line {} exceeds 760 - 2*48",
                        line.width
                    );
                }
                checked = true;
            }
        }
    }
    assert!(checked, "no text was emitted");
}

#[test]
fn stacked_fill_text_siblings_do_not_overlap() {
    // Regression: Fill width forced a re-wrap after the parent reserved height at the
    // parent width, so the second paragraph started before the first one ended.
    let mut engine = Engine::headless(760, 980);
    engine.mount(
        group()
            .direction(Direction::Vertical)
            .width(Size::fill())
            .height(Size::fill())
            .children([
                text(LOREM)
                    .size(15.0)
                    .width(Size::fill())
                    .margin(Margin::x(48.0)),
                text(LOREM)
                    .size(15.0)
                    .width(Size::fill())
                    .margin(Margin::x(48.0)),
            ]),
    );
    engine.frame();

    let runs = draw_runs(&engine);
    assert!(
        runs.len() >= 2,
        "expected two text runs, got {}",
        runs.len()
    );
    let (first, _, first_h) = runs[0];
    let (second, ..) = runs[1];
    assert!(
        second.y + 0.5 >= first.y + first_h,
        "siblings overlap: second top {} < first bottom {}",
        second.y,
        first.y + first_h
    );
}

#[test]
fn row_text_wraps_at_assigned_width_not_parent_width() {
    let mut engine = Engine::headless(760, 980);
    engine.mount(
        group()
            .direction(Direction::Horizontal)
            .width(Size::fill())
            .height(Size::fill())
            .children([
                text(LOREM).size(15.0).width(Size::fixed(300.0)),
                text(LOREM).size(15.0).width(Size::fixed(300.0)),
            ]),
    );
    engine.frame();

    let mut checked = 0;
    for chunk in &engine.scene().chunks {
        for cmd in &chunk.commands {
            if let SceneCommand::DrawText { layout, .. } = cmd {
                assert!(layout.lines.len() > 1, "row text should wrap at 300px");
                for line in &layout.lines {
                    assert!(line.width <= 301.0, "row line {} exceeds 300", line.width);
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 2, "expected two row text runs");
}

#[test]
fn tracking_does_not_overflow_wrap_width() {
    let ctx = context();
    let mut cfg = config(LOREM);
    cfg.letter_spacing = 2.0;
    let layout = ctx.shape_config(&cfg, Constraints::loose(160.0, f32::INFINITY));
    assert!(layout.lines.len() > 1, "expected wrapping with tracking");
    for line in &layout.lines {
        assert!(
            line.width <= 160.5,
            "tracked line {} exceeds 160",
            line.width
        );
    }
}

#[test]
fn ellipsis_is_word_aware_and_within_width() {
    let ctx = context();
    let mut cfg = config(LOREM);
    cfg.line_height = LineHeight::Multiple(1.4);
    cfg.max_lines = Some(2);
    cfg.overflow = TextOverflow::Ellipsis;
    let layout = ctx.shape_config(&cfg, Constraints::loose(160.0, f32::INFINITY));
    assert_eq!(layout.lines.len(), 2, "ellipsis should clamp to max_lines");
    for line in &layout.lines {
        assert!(line.width <= 160.5, "line {} exceeds 160", line.width);
    }
}

#[test]
fn ellipsis_handles_unbreakable_word() {
    let ctx = context();
    let mut cfg = config("supercalifragilisticexpialidociousandthensomemoretext");
    cfg.max_lines = Some(1);
    cfg.overflow = TextOverflow::Ellipsis;
    let layout = ctx.shape_config(&cfg, Constraints::loose(60.0, f32::INFINITY));
    assert_eq!(layout.lines.len(), 1, "long word should clamp to one line");
    assert!(layout.lines[0].width <= 60.5, "line exceeds 60");
}

#[test]
fn cache_shares_layouts_across_widths() {
    let ctx = context();
    let cfg = config(LOREM);
    let wide = ctx.shape_config(&cfg, Constraints::loose(400.0, f32::INFINITY));
    let narrow = ctx.shape_config(&cfg, Constraints::loose(160.0, f32::INFINITY));
    assert!(
        narrow.lines.len() > wide.lines.len(),
        "narrower width should wrap into more lines"
    );
    let wide_again = ctx.shape_config(&cfg, Constraints::loose(400.0, f32::INFINITY));
    assert!(
        Arc::ptr_eq(&wide, &wide_again),
        "repeat lookup should return the cached Arc"
    );
    assert_eq!(wide_again.lines.len(), wide.lines.len());
}
