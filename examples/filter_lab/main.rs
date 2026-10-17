// Single responsibility: Release driver for the element-filter demo.
//
// Side-by-side cards each carrying one `filter` (blur, brightness,
// saturate, contrast, hue-rotate, and a chained blur+saturate) so the
// human can compare effects row-to-row.

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
