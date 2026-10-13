// Single responsibility: demo3 view tree (chrome) shared by the entrypoint.

use aurora::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Demo 3 (Optype Font Manager)";
pub const HOT_WIDTH: u32 = 1720;
pub const HOT_HEIGHT: u32 = 1020;
pub const HOT_BG: u32 = 0xFFFFFF;

pub fn build_ui() -> impl IntoElement {
    let icons = super::icons::AppIcons::load();
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            fill={Color::WHITE}
        >
            {super::sidebar::render_sidebar(&icons)}
            {super::canvas::render_canvas(&icons)}
            {super::inspector::render_inspector(&icons)}
        </Group>
    }
}
