// Single responsibility: demo3 view tree (chrome) shared by the entrypoint.

use aurora::prelude::*;

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
