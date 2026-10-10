// Single responsibility: Showcases Aurora text layout: wrap, alignment, tracking, decoration, ellipsis.

#[path = "../common/mod.rs"]
mod common;

mod view;

use common::ExampleCli;
pub use view::*;

const WIDTH: u32 = 760;
const HEIGHT: u32 = 980;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    cli.app_config_multi("Aurora — Text Layout")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(0x0B1020)
        .run(view::build_ui);
}
