// Single responsibility: Native OS window presenting the live Paper 1 showcase design.

use aurora::prelude::*;
use common::ExampleCli;

mod common;

pub const WIDTH: u32 = 500;
pub const HEIGHT: u32 = 482;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    cli.app_config("Aurora — Demo 3 (Optype Font Manager)")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::WHITE)
        .run(build_ui);
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
                    fill={0x91DF69}
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
                        >"Hello"</Text>
                    </Group>
                    <Group width={116.0} height={45.0} fill={0x4900FF}>
                        <Text
                            selectable={true}
                            size={12.0}
                            line_height={16.0}
                            color={Color::WHITE}
                            anchor={Anchor::BottomRight}
                        >"bottom-right"</Text>
                    </Group>
                    <Group
                        direction="vertical"
                        width={140.0}
                        height={164.0}
                        radius="max"
                        fill={0x4900FF}
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
