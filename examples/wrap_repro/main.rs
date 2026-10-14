// Release driver for the wrap-rewrite repro.

#[path = "../common/mod.rs"]
mod common;

use common::ExampleCli;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(view::HOT_WIDTH, view::HOT_HEIGHT);
    cli.init_telemetry();

    cli.app_config_multi(view::HOT_TITLE)
        .font(
            "Inter",
            include_bytes!("../../crates/riode/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(view::Page);
}
