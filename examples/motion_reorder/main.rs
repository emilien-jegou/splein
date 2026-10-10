// Single responsibility: Showcase dragging a list item into a new position.

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/motion_ui.rs"]
mod motion_ui;

mod view;

use common::ExampleCli;
use motion_ui::*;
pub use view::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 660;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi("Aurora — Web 04 · Drag to reorder")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .run(view::Page);
}
