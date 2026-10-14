// Single responsibility: Self-contained interactive runner for the Paper HUD Dock design.

mod view;

pub use view::*;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let icons = view::Icons::load();

    riode::prelude::App::config()
        .title(view::HOT_TITLE)
        .size(view::HOT_WIDTH, view::HOT_HEIGHT)
        .font(
            "Inter",
            include_bytes!("../../crates/riode/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(move || view::build_ui(&icons));
}
