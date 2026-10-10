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

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 620;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi("Aurora — view! demo")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .run(view::PageEntry);
}
