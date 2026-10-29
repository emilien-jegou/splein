// Single responsibility: the demo1 view itself, shared by both modes.
//
// Bare module: no `extern crate` line here. Each host aliases its own runtime
// to the name `riode` before including this file, so the same code compiles
// against the real `riode` (release binary) and `riode-shared` (dev cdylib):
//   - release: `examples/demo1/main.rs` (riode-examples)
//   - dev:     generative `riode-hot-gen-ui` (dep: riode-shared, cdylib)
// Keep this file free of `define_hot_ui!` / entry points; those live in the
// dev dispatcher only. Pin component identity so hook state recalls across reloads.

use riode::dsl::IntoElement;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Demo 1";
pub const HOT_WIDTH: u32 = 500;
pub const HOT_HEIGHT: u32 = 482;
pub const HOT_BG: u32 = 0xFFFFFF;

/// Stable root scope for the demo1 view.
#[component(name = "demo1::Page")]
pub fn Page() -> impl IntoElement {
    build_ui()
}

pub fn build_ui() -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={Color::WHITE}
            alignment="center"
            distribution="center"
        >
            <Group
                direction="vertical"
                width={500.0}
                height="fill"
                margin={Margin::y(41.0)}
                fill={0xCA6D60}
                alignment="center"
                distribution="center"
            >
                <Group
                    direction="horizontal"
                    width={460.0}
                    height={204.0}
                    fill={0x336133}
                    gap={16.0}
                    alignment="center"
                    distribution="center"
                >
                    <Group
                        direction="vertical"
                        width={127.0}
                        height={96.0}
                        fill={0x4900FF}
                        alignment="center"
                        distribution="center"
                    >
                        <Text
                            selectable={true}
                            size={16.0}
                            line_height={20.0}
                            color={Color::WHITE}
                        >"Hello you"</Text>
                    </Group>
                    <Group width={116.0} height={45.0} fill={0xFFFFFF}>
                        <Text
                            selectable={true}
                            size={12.0}
                            line_height={16.0}
                            color={Color::BLACK}
                            anchor={Anchor::BottomRight}
                        >"bottom-right"</Text>
                    </Group>
                    <Group
                        direction="vertical"
                        width={140.0}
                        height={144.0}
                        radius="max"
                        fill={0xFF00FF}
                        stroke={Stroke::inside(3.0, Color::BLACK)}
                        alignment="center"
                        distribution="center"
                    >
                        <Text
                            size={12.0}
                            selectable={true}
                            line_height={16.0}
                            color={Color::WHITE}
                        >"with radius"</Text>
                    </Group>
                </Group>
            </Group>
        </Group>
    }
}
