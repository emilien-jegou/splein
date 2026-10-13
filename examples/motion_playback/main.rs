// Single responsibility: Entrypoint for the playback demo; the page itself is declarative.

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/motion_ui.rs"]
mod motion_ui;

mod view;

use common::ExampleCli;
use motion_ui::*;

pub use view::Page;

fn main() {
    let cli = ExampleCli::parse(view::HOT_WIDTH, view::HOT_HEIGHT);
    cli.init_telemetry();

    cli.app_config(view::HOT_TITLE)
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(Page);
}
