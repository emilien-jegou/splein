// Single responsibility: Prism-only spectral repro — release driver.
//
// One regular glass card plus an independent spectral response, over fixed
// colour blobs. Human judges the rim; no test asserts.

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
