// Single responsibility: Native OS window presenting the live Paper 1 showcase design.

use riode::prelude::*;
use common::ExampleCli;

#[path = "../common/mod.rs"]
mod common;

mod view;

pub use view::*;

fn main() {
    let cli = ExampleCli::parse(view::HOT_WIDTH, view::HOT_HEIGHT);
    cli.init_telemetry();
    cli.app_config_multi(view::HOT_TITLE)
        .font(
            "Inter",
            include_bytes!("../../crates/core/riode/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(view::build_ui);
}
