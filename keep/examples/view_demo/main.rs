// Single responsibility: view! macro demo — the same card in builder vs RSX form.
//
// Two rows render the same thing: row 1 hand-built with builders, row 2 built
// with `view!` through a typed-props #[component]. Human judges identity:
// same layout, same drift, same pixels. No test asserts.

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
        view::PageEntry,
    );
}
