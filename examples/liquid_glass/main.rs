// Single responsibility: Liquid-glass material demo — release driver.
//
// Three P1 presets (regular / clear / lens) plus a pill over a moving
// gradient backdrop. Human judges the lens; no test asserts.

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
