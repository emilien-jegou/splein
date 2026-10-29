// Single responsibility: desktop-glass demo — windowed release preview.
//
// The same view the overlay host (`apps/desktop-glass`) mounts through the
// hot dylib, run here as a plain opaque window for layout/material work
// without a compositor. For the transparent overlay with compositor blur,
// run the overlay host instead (see apps/desktop-glass/README.md).

#[path = "../common/mod.rs"]
mod common;

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
