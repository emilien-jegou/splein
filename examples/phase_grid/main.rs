// Single responsibility: Static Y-eighth phase grid for subpixel text triage.
//
// Six sizes (10/12/14/16/24/36px), each at all eight Y-eighth device
// offsets in stacked rows: identical glyphs, identical X, only the
// fractional Y remainder varies. Human judges phase-dependent dirt with
// frozen captures; no test asserts.

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
