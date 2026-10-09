// Single responsibility: Side-by-side web-parity demo for shadow styling.
//
// Mirrors a web reference 1:1 so the human can compare directly: a #F1CFFC
// background holding two 150x150 white cards (radius 8, no border) with
// centered labels above them, spaced as the reference (96v/48h breathing
// room, 48px between cards).
//   - "Typical Shadow": box-shadow rgba(0,0,0,0.35) 3px 6px 6px 0px,
//     via Shadow::web_like.
//   - "Optimized Shadow": five stacked tinted drop-shadows
//     rgba(122,61,143,0.15) at (0.5,1,1) (1,2,2) (2,4,4) (4,8,8) (8,16,16),
//     via Shadow::layered + Shadow::drop.
// Frozen inspection via F8 / `.` / `s` comes from the shared harness.

#[path = "common/mod.rs"]
mod common;

use aurora::prelude::*;
use common::ExampleCli;

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 640;

const PANEL_BG: u32 = 0xF1CFFC;
const LABEL_COLOR: u32 = 0x483050;

fn card(label: &'static str, shadow: Shadow) -> Element {
    group()
        .direction(Direction::Vertical)
        .gap(24.0)
        .alignment(Alignment::Center)
        .children((
            text(label)
                .size(16.0)
                .weight(500)
                .color(Color::hex(LABEL_COLOR)),
            group()
                .width(150.0)
                .height(150.0)
                .radius(Radius::scalar(8.0))
                .fill(Color::WHITE)
                .shadow(shadow),
        ))
        .into_element()
}

fn build_ui() -> impl IntoElement {
    // The pink panel is the full background: no outer card, just the
    // two labeled cards centered with the reference's 96v/48h breathing room.
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .fill(Color::hex(PANEL_BG))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .children([group()
            .direction(Direction::Horizontal)
            .width(Size::fill())
            .gap(48.0)
            .margin(Margin::symmetric(96.0, 48.0))
            .alignment(Alignment::Center)
            .distribution(Distribution::Center)
            .children([
                card(
                    "Typical Shadow",
                    Shadow::web_like(3.0, 6.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35)),
                ),
                // TEMPORARY diagnostics (remove after): zero offset isolates
                // whether the bottom band follows the offset; big offset
                // tests whether any hole scales with oy.
                card(
                    "probe zero-offset",
                    Shadow::web_like(0.0, 0.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35)),
                ),
                card(
                    "probe big-offset",
                    Shadow::web_like(0.0, 20.0, 6.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.35)),
                ),
                // TEMPORARY isolation (remove after): single layers tell
                // whether squareness lives in one layer (geometry) or only
                // in the stack (profile/stacking interaction).
                card(
                    "probe single big",
                    Shadow::layered(Color::rgba(
                        122.0 / 255.0,
                        61.0 / 255.0,
                        143.0 / 255.0,
                        0.15,
                    ))
                    .drop(8.0, 16.0, 16.0),
                ),
                card(
                    "probe single small",
                    Shadow::layered(Color::rgba(
                        122.0 / 255.0,
                        61.0 / 255.0,
                        143.0 / 255.0,
                        0.15,
                    ))
                    .drop(0.5, 1.0, 1.0),
                ),
                card(
                    "Optimized Shadow",
                    Shadow::layered(Color::rgba(
                        122.0 / 255.0,
                        61.0 / 255.0,
                        143.0 / 255.0,
                        0.15,
                    ))
                    .drop(8.0, 16.0, 16.0),
                ),
            ])])
}

fn main() {
    let cli = ExampleCli::parse(DEFAULT_WIDTH, DEFAULT_HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Shadow Lab")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PANEL_BG))
        .run(build_ui);
}
