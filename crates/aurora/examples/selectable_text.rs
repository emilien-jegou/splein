// Single responsibility: Selectable readonly text showcase (press, drag, multi-click).

use aurora::prelude::*;
use common::ExampleCli;

mod common;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

const LOREM: &str = "Drag across this paragraph to select it. \
    Double-click selects a word, triple-click the whole paragraph. \
    A plain click focuses a caret; arrows move it, Shift extends, \
    Ctrl+arrows jump words, Ctrl+A selects all, Ctrl+C copies, Escape collapses.";

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    cli.app_config_multi("Aurora — Selectable Text")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::WHITE)
        .run(build_ui);
}

fn build_ui() -> impl IntoElement {
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
