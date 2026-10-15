// Single responsibility: view! macro demo — the same card in builder vs RSX form.
//
// Two rows render the same thing: row 1 hand-built with builders, row 2 built
// with `view!` through a typed-props #[component]. Human judges identity:
// same layout, same drift, same pixels. No test asserts.

#[path = "../common/mod.rs"]
mod common;

#[path = "../common/motion_ui.rs"]
mod motion_ui;

use common::ExampleCli;
use motion_ui::*;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(view::HOT_WIDTH, view::HOT_HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi(view::HOT_TITLE)
        .font(
            "Inter",
            include_bytes!("../../crates/core/riode/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(view::PageEntry);
}
