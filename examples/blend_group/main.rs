// Single responsibility: group blend-mode comparison — release driver.
//
// Four identical panels over one identical split backdrop; the row's
// `blend=` mode is the only variable. Human judges the composite; no
// test asserts.

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
