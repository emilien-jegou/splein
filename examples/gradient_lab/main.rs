// Single responsibility: Linear-gradient + dithering lab (wgpu Bundle 2).
// Release driver; the view lives in `view.rs`, window chrome in `view::HOT_*`.

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
