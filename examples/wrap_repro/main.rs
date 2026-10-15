// Release driver for the wrap-rewrite repro.

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
