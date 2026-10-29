// Single responsibility: Release driver for the effect-stack demo.
//
// Four identical cards whose `effects` stacks differ only in stage
// order (saturate/blur before vs after the glass lens) so the human
// can compare composition row-to-row.

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
