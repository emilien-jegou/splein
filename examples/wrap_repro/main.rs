// Release driver for the wrap-rewrite repro.

#[path = "../common/mod.rs"]
mod common;

use common::ExampleCli;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 900;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi("Aurora — Wrap repro")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(0x10151D)
        .run(view::Page);
}
