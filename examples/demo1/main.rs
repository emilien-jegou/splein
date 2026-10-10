// Single responsibility: Native OS window presenting the live Paper 1 showcase design.

use aurora::prelude::*;
use common::ExampleCli;

#[path = "../common/mod.rs"]
mod common;

mod view;

pub use view::*;

pub const WIDTH: u32 = 500;
pub const HEIGHT: u32 = 482;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    cli.app_config_multi("Aurora — Demo 1")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(Color::WHITE)
        .run(view::build_ui);
}
