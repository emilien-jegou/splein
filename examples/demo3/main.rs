// Single responsibility: Entrypoint for the live 1720x1020 Optype font manager application.

mod canvas;
mod hud;
mod icons;
mod inspector;
mod sidebar;
mod view;

#[path = "../common/mod.rs"]
mod common;

use riode::prelude::*;
use common::ExampleCli;

fn main() {
    // 1. Parse CLI arguments (--gpu, --cpu, --width, --height, --no-hud)
    let cli = ExampleCli::parse(view::HOT_WIDTH, view::HOT_HEIGHT);
    cli.init_telemetry();

    // 2. Launch with configured backend (TinySkia or Vello)
    cli.app_config(view::HOT_TITLE)
        .font(
            "Inter",
            include_bytes!("../../crates/core/riode/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(view::build_ui);
}
