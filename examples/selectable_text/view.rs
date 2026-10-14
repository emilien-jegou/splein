// Single responsibility: Selectable readonly text showcase (press, drag, multi-click).

use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Selectable Text";
pub const HOT_WIDTH: u32 = 640;
pub const HOT_HEIGHT: u32 = 360;
pub const HOT_BG: u32 = 0xFFFFFF;

const LOREM: &str = "Drag across this paragraph to select it. \
    Double-click selects a word, triple-click the whole paragraph. \
    A plain click focuses a caret; arrows move it, Shift extends, \
    Ctrl+arrows jump words, Ctrl+A selects all, Ctrl+C copies, Escape collapses.";

pub(crate) fn build_ui() -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={Color::WHITE}
            alignment="center"
            distribution="center"
        >
            <Text size={13.0} color={0x666666}>"Selectable (readonly, mouse-only)"</Text>
            <Text
                key="selectable-body"
                selectable={true}
                size={17.0}
                line_height={26.0}
                color={Color::BLACK}
                width={Size::fixed(520.0)}
            >{LOREM}</Text>
            <Text size={13.0} color={0x666666} width={Size::fixed(520.0)}>
                "This label stays click-transparent: dragging over it selects nothing."
            </Text>
        </Group>
    }
}
