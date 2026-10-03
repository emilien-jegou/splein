// Single responsibility: Showcases Aurora text layout: wrap, alignment, tracking, decoration, ellipsis.

use aurora::prelude::*;

mod common;
use common::ExampleCli;

const WIDTH: u32 = 760;
const HEIGHT: u32 = 980;

const WRAP: &str = "Aurora wraps this paragraph to the box width and measures the result once, so the box grows to fit every line. Line height is relative to the font size, keeping the typographic rhythm consistent as the type scales.";

const JUSTIFY: &str = "Justification stretches the gaps between words so every line but the last one reaches the edge of the box, producing an even, magazine-style paragraph block instead of a ragged right margin.";

const ELLIPSIS: &str = "The dock is a compact floating control that sits above the canvas and reports the active tool, hovered element, and pending gesture, truncating gracefully when the label would otherwise overflow its allotted width.";

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    cli.app_config("Aurora — Text Layout")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(0x0B1020))
        .run(build_ui);
}

/// Small uppercase section label.
fn heading(content: &'static str) -> TextDef {
    text(content)
        .font_family("Inter")
        .size(12.0)
        .weight(700)
        .letter_spacing(1.6)
        .line_height_multiple(1.2)
        .color(Color::hex(0x6C8CFF))
        .margin(Margin::x(48.0))
}

/// Body copy constrained to the available column width, so it wraps.
fn body(content: &'static str) -> TextDef {
    text(content)
        .font_family("Inter")
        .size(15.0)
        .line_height_multiple(1.6)
        .color(Color::hex(0xAEB9CC))
        .width(Size::fill())
        .margin(Margin::x(48.0))
}

/// Single-line sample constrained to the available column width.
fn sample(content: &'static str) -> TextDef {
    text(content)
        .font_family("Inter")
        .size(15.0)
        .color(Color::hex(0xE2E8F0))
        .width(Size::fill())
        .margin(Margin::x(48.0))
}

fn build_ui() -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .fill(Color::hex(0x0B1020))
        .gap(14.0)
        .children([
            text("Typography")
                .font_family("Inter")
                .size(34.0)
                .weight(700)
                .line_height_multiple(1.1)
                .color(Color::WHITE)
                .margin(Margin::sides(26.0, 0.0, 12.0, 48.0)),
            heading("01 · SOFT WRAP · RELATIVE LINE HEIGHT"),
            body(WRAP),
            heading("02 · ALIGNMENT"),
            sample("Aurora").text_align(TextAlign::Start),
            sample("Aurora").text_align(TextAlign::Center),
            sample("Aurora").text_align(TextAlign::End),
            heading("03 · JUSTIFY"),
            body(JUSTIFY).text_align(TextAlign::Justify),
            heading("04 · TRACKING · WEIGHTS · ITALIC"),
            sample("Wide tracking through the whole run.")
                .letter_spacing(3.0)
                .color(Color::hex(0x8FE3C0)),
            sample("Light weight 300").weight(300),
            sample("Bold weight 700").weight(700),
            sample("Italic face for emphasis.").italic(),
            heading("05 · DECORATION"),
            sample("Underlined value").underline(),
            sample("Struck-through value").strikethrough(),
            sample("Overlined value").overline(),
            heading("06 · ELLIPSIS · MAX LINES 2"),
            sample(ELLIPSIS).max_lines(2),
        ])
}
