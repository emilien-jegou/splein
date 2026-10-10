// Single responsibility: Showcase static layout running once while a motion transform runs per frame.

#[path = "../common/mod.rs"]
mod common;

#[path = "../common/motion_ui.rs"]
mod motion_ui;

use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 440;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi("Aurora — Motion 01 · Layout vs compositor transform")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .run(view::Page);
}
