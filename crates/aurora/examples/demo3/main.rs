// Single responsibility: Entrypoint for the live 1720x1020 Optype font manager application.

mod canvas;
mod hud;
mod icons;
mod inspector;
mod sidebar;

#[path = "../common/mod.rs"]
mod common;

use aurora::prelude::*;
use canvas::render_canvas;
use common::ExampleCli;
use icons::AppIcons;
use inspector::render_inspector;
use sidebar::render_sidebar;

const DEFAULT_WIDTH: u32 = 1720;
const DEFAULT_HEIGHT: u32 = 1020;

fn main() {
    // 1. Parse CLI arguments (--gpu, --cpu, --width, --height, --no-hud)
    let cli = ExampleCli::parse(DEFAULT_WIDTH, DEFAULT_HEIGHT);
    cli.init_telemetry();

    let icons = AppIcons::load();

    // 2. Launch with configured backend (TinySkia or Vello)
    cli.app_config("Aurora — Demo 3 (Optype Font Manager)")
        .font("Inter", include_bytes!("../../assets/inter-var.ttf"))
        .background(Color::WHITE)
        .run(move || {
            group().direction(Direction::Horizontal).width(Size::fill()).height(Size::fill()).fill(Color::WHITE).children((
                render_sidebar(&icons),
                render_canvas(&icons),
                render_inspector(&icons),
            ))
        });
}
