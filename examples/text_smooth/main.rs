// Single responsibility: Slow-drift text repro for subpixel animation triage.
//
// Four runs drift at ~15px/s (subpixel per frame) to judge animation
// smoothness: X-only, Y-only and diagonal at 15px (hinted outlines), plus
// Y-only at 22px (unhinted, above the 16px hint cutoff). The rasterizer uses
// an axis-split phase policy — X renders at its nearest quarter-pixel bin,
// Y frozen at bin zero — with the fractional remainder blended explicitly
// and placed at integer pixels, so motion glides without morphing and rest
// frames stay crisp. Human judges shimmer; no test asserts.

#[path = "../common/mod.rs"]
mod common;

#[path = "../common/motion_ui.rs"]
mod motion_ui;

use common::ExampleCli;
use motion_ui::*;

mod view;

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
        .run(view::Page);
}
