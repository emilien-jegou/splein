// Single responsibility: Self-contained interactive runner for the Paper HUD Dock design.

mod view;

pub use view::*;

const WIDTH: u32 = 480;
const HEIGHT: u32 = 130;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let icons = view::Icons::load();

    aurora::prelude::App::config()
        .title("Aurora — Demo 2 (Paper HUD Dock)")
        .size(WIDTH, HEIGHT)
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(0x141414)
        .run(move || view::build_ui(&icons));
}
