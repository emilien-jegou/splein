// Single responsibility: Showcase transitions as reactive signal adapters over one boolean.

#[path = "../common/mod.rs"]
mod common;

#[path = "../common/motion_ui.rs"]
mod motion_ui;

use common::ExampleCli;
use motion_ui::*;

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
        .run(view::Page);
}
