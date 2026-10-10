// Single responsibility: Selectable readonly text showcase (press, drag, multi-click).

#[path = "../common/mod.rs"]
mod common;

mod view;

use aurora::prelude::*;
use common::ExampleCli;
pub use view::*;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    cli.app_config_multi("Aurora — Selectable Text")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(Color::WHITE)
        .run(view::build_ui);
}
