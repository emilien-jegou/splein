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

#[path = "../common/mod.rs"]
mod common;

mod view;

use common::ExampleCli;
pub use view::*;

fn main() {
    let cli = ExampleCli::parse(view::HOT_WIDTH, view::HOT_HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi(view::HOT_TITLE)
        .font(
            "Inter",
            include_bytes!("../../crates/riode/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(view::build_ui);
}
