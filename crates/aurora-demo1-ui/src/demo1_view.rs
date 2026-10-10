// Single responsibility: demo1's declarative interface, shared by two hosts.
//
// This file is the single source of truth for the demo1 view. It is included both by
// the standalone `demo1` example (`examples/demo1.rs`, normal static binary) and by the
// hot-reloadable UI crate (`crates/aurora-demo1-ui`, rebuilt live). It must stay
// backend-free and hook-free: only `aurora::` paths (which resolve to the real crate in
// the example and to the shared image in the UI crate) and pure builder calls.
//
// Edit the colors below while the hot host runs to see them land live.

use aurora::prelude::*;

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
                        fill={0x4400FF}
                        stroke={Stroke::inside(3.0, Color::BLACK)}
                        alignment="center"
                        distribution="center"
                    >
                        <Text
                            size={12.0}
                            selectable={true}
                            line_height={16.0}
                            color={Color::BLACK}
                        >"with radius"</Text>
                    </Group>
                </Group>
            </Group>
        </Group>
    }
}
