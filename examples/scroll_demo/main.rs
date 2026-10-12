// Single responsibility: scroll drift repro (release driver).
// Mirrors examples/text_smooth/main.rs; WIDTH/HEIGHT echoed in example.sh.

#[path = "../common/mod.rs"]
mod common;

#[path = "../common/motion_ui.rs"]
mod motion_ui;

use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 860;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi("Aurora — Scroll drift repro")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .run(view::Page);
}
