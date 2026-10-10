// Single responsibility: Showcase an action button that grows and pushes the field aside.

#[path = "../common/mod.rs"]
mod common;

#[path = "../common/motion_ui.rs"]
mod motion_ui;

use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 780;
pub const HEIGHT: u32 = 460;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi("Aurora — Web 03 · Growing action")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .run(view::Page);
}
