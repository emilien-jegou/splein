// Single responsibility: Minimal slow-drift text repro for jitter triage.
//
// Three identical runs drift very slowly (~0.25px/frame): X only, Y only,
// X+Y diagonal. Human judges shimmer at the ease-out tail; no test asserts.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 620;
/// Travel in px per axis.
const TRAVEL: f32 = 120.0;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Text smooth drift repro")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// Three slow runs: horizontal, vertical, diagonal.
#[component]
pub fn Page() -> impl IntoElement {
    // Frame-clock ping-pong: moves from the first frame, no 12s dead wait.
    // 16s period (8s out, 8s back) at 120px => ~15px/s, subpixel per frame.
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 16.0) / 16.0;
        let tri = if phase < 0.5 {
            phase * 2.0
        } else {
            2.0 - phase * 2.0
        };
        tri as f32
    });

    let x = t.clone().map(|v| v * TRAVEL);
    let y = t.clone().map(|v| v * TRAVEL);
    let xy = t.clone().map(|v| Transform::from_translation(v * TRAVEL, v * TRAVEL));
    let y_big = t.map(|v| v * TRAVEL);

    page(
        "TEXT · SMOOTH DRIFT",
        "Slow text repro.",
        "15px rows (hinted) drift on X, Y, diagonal; 22px row (unhinted) drifts on Y. If only hinted Y morphs, the hinter is the culprit.",
        content(
            group()
                .direction(Direction::Vertical)
                .width(Size::fill())
                .height(Size::fill())
                .gap(16.0)
                .children((
                    drift_row("X · 15PX", group().children([drift_text()]).x(x)),
                    drift_row("Y · 15PX", group().children([drift_text()]).y(y)),
                    drift_row(
                        "X+Y · 15PX",
                        group().children([drift_text()]).transform(xy),
                    ),
                    drift_row(
                        "Y · 22PX",
                        group().children([drift_text_big()]).y(y_big),
                    ),
                )),
        ),
    )
}

/// Fixed-height track holding one drifting run.
fn drift_row(label: &'static str, element: impl IntoElement) -> GroupDef {
    group()
        .direction(Direction::Horizontal)
        .width(Size::fill())
        .height(64.0)
        .alignment(Alignment::Center)
        .gap(16.0)
        .children((
            group()
                .width(96.0)
                .height(Size::fill())
                .alignment(Alignment::Center)
                .children([eyebrow(label)]),
            group()
                .width(Size::fill())
                .height(Size::fill())
                .radius(12.0)
                .fill(Color::hex(SURFACE))
                .stroke(Stroke::inside(1.0, hairline()))
                .alignment(Alignment::Center)
                .children([element]),
        ))
}

/// Shared sample copy so all three axes render identical glyphs.
fn drift_text() -> TextDef {
    text("Pack my box with five dozen liquor jugs 0123456789")
        .size(15.0)
        .weight(500)
        .color(Color::hex(INK))
}

/// Same copy above the hint cutoff (16px): unhinted linear outlines.
fn drift_text_big() -> TextDef {
    text("Pack my box with five dozen liquor jugs 0123456789")
        .size(22.0)
        .weight(500)
        .color(Color::hex(INK))
}
