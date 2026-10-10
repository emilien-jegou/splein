// Single responsibility: Entrypoint for the live 1720x1020 Optype font manager application.

mod canvas;
mod hud;
mod icons;
mod inspector;
mod sidebar;
mod view;

#[path = "../common/mod.rs"]
mod common;

use aurora::prelude::*;
use common::ExampleCli;

const DEFAULT_WIDTH: u32 = 1720;
const DEFAULT_HEIGHT: u32 = 1020;

fn main() {
    // 1. Parse CLI arguments (--gpu, --cpu, --width, --height, --no-hud)
    let cli = ExampleCli::parse(DEFAULT_WIDTH, DEFAULT_HEIGHT);
    cli.init_telemetry();

    // 2. Launch with configured backend (TinySkia or Vello)
    cli.app_config("Aurora — Demo 3 (Optype Font Manager)")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(Color::WHITE)
        .run(view::build_ui);
}
