// Single responsibility: Release driver for the env_reflect demo.
//
// Rows of two identical cards whose stacks differ in one ENV knob or
// item only, so the human can compare the glint row-to-row.

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
