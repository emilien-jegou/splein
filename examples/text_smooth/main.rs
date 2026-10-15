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


mod view;

pub use view::*;

fn main() {
    common::example_cli_run(
        view::HOT_TITLE,
        view::HOT_WIDTH,
        view::HOT_HEIGHT,
        view::HOT_BG,
        true,
        view::Page,
    );
}
