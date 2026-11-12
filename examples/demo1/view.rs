use riode::dsl::{GroupElement, IntoElement};
use riode::prelude::*;
use riode_shader::{ Shape, Fill };

pub const HOT_TITLE: &str = "Aurora — Demo 1";
pub const HOT_WIDTH: u32 = 500;
pub const HOT_HEIGHT: u32 = 482;
pub const HOT_BG: u32 = 0xFFFFFF;

#[component(name = "demo1::Page")]
pub fn Page() -> impl IntoElement {
    build_ui()
}

pub fn build_ui() -> GroupElement {
    view! {
        <g
            direction="vertical"
            width="fill"
            height="fill"
            material={Fill::solid(Color::WHITE)}
            alignment="center"
            distribution="center"
        >
            <Group
                direction="vertical"
                width={500.0}
                height="fill"
                margin={Margin::y(41.0)}
                material={Fill::solid(0xCA6D60)}
                alignment="center"
                distribution="center"
            >
                <Group
                    direction="horizontal"
                    width={460.0}
                    height={204.0}
                    material={Fill::solid(0x336133)}
                    gap={16.0}
                    alignment="center"
                    distribution="center"
                >
                    <Group
                        direction="vertical"
                        width={127.0}
                        height={96.0}
                        shape={Shape::squircle(999.0)}
                        material=[Fill::new(0x0000FF)]
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
                    <Group width={116.0} height={45.0} material={Fill::solid(0xFFFFFF)}>
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
                        material={Fill::solid(0xFF00FF)}
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
        </g>
    }
}
