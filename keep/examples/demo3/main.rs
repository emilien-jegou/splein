// Single responsibility: Entrypoint for the live 1720x1020 Optype font manager application.

mod canvas;
mod hud;
mod icons;
mod inspector;
mod sidebar;
mod view;

#[path = "../common/mod.rs"]
mod common;


fn main() {
    common::example_cli_run(
        view::HOT_TITLE,
        view::HOT_WIDTH,
        view::HOT_HEIGHT,
        view::HOT_BG,
        false,
        view::Page,
    );
}
