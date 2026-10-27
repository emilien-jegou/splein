// Single responsibility: compositor-warp demo — windowed release preview.
//
// Layout/material preview only: custom effect stages execute in the
// riode-glass compositor plugin, and the parent-surface window path
// uploads just the largest island's stack — the order pair must be
// judged as the overlay (see apps/desktop-glass + view.rs header).

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
