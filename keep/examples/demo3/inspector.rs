// Single responsibility: Right details inspector displaying metrics and variable font sliders.

use super::icons::AppIcons;
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this file composes repr-builders directly

pub fn render_inspector(icons: &AppIcons) -> impl IntoElement {
    view! {
        <Group direction="horizontal" width={361.0} height="fill">
            <Group
                width={1.0}
                height="fill"
                fill={Color::hex_alpha(0x08090C, 0.06)}
            />
            <Group
                direction="vertical"
                width={360.0}
                height="fill"
                fill={0xFCFCFC}
            >
                <Group
                    direction="horizontal"
                    width="fill"
                    alignment="center"
                    margin={Margin::x(16.0)}
                    margin={Margin::top(16.0)}
                >
                    <Text size={16.0} weight={500u16} color={0x0A0A0A}>"Universal Sans"</Text>
                    <Group width="fill" />
                    <Group
                        direction="horizontal"
                        height={24.0}
                        radius={6.0}
                        fill={0xE9FFEB}
                        alignment="center"
                        gap={6.0}
                    >
                        <Group
                            width={6.0}
                            height={6.0}
                            radius="max"
                            fill={0x008E2D}
                            margin={Margin::left(8.0)}
                        />
                        <Text
                            size={12.0}
                            weight={500u16}
                            color={0x025719}
                            margin={Margin::right(8.0)}
                        >"Active"</Text>
                    </Group>
                </Group>
                <Group
                    direction="horizontal"
                    width="fill"
                    height={32.0}
                    margin={Margin::top(8.0)}
                >
                    {tab("General", true)}
                    {tab("Variants", false)}
                    {tab("Instances", false)}
                    {tab("Alternatives", false)}
                </Group>
                <Group
                    width="fill"
                    height={1.0}
                    fill={Color::hex_alpha(0x08090C, 0.06)}
                />
                <Group
                    direction="vertical"
                    width="fill"
                    margin={Margin::x(16.0)}
                    margin={Margin::top(16.0)}
                    gap={14.0}
                >
                    <Text size={13.0} weight={500u16} color={Color::BLACK}>"Details"</Text>
                    {key_value("Font name", "UniversalSans-Regular", None)}
                    {key_value("Designer", "Jhon Batista", Some(&icons.link))}
                    {key_value("Publisher", "BatistaCorp", Some(&icons.link))}
                </Group>
                <Group
                    width="fill"
                    height={1.0}
                    fill={Color::hex_alpha(0x08090C, 0.06)}
                    margin={Margin::y(16.0)}
                />
                <Group
                    direction="vertical"
                    width="fill"
                    margin={Margin::x(16.0)}
                    gap={10.0}
                >
                    <Text size={13.0} weight={500u16} color={Color::BLACK}>"Metrics"</Text>
                    <Group direction="horizontal" width="fill" gap={8.0}>
                        {metric_tile("Ascender", "910")}
                        {metric_tile("Cap Height", "690")}
                    </Group>
                    <Group direction="horizontal" width="fill" gap={8.0}>
                        {metric_tile("x-Height", "540")}
                        {metric_tile("Descender", "-210")}
                    </Group>
                </Group>
                <Group
                    width="fill"
                    height={1.0}
                    fill={Color::hex_alpha(0x08090C, 0.06)}
                    margin={Margin::y(16.0)}
                />
                <Group
                    direction="vertical"
                    width="fill"
                    margin={Margin::x(16.0)}
                    gap={12.0}
                >
                    <Group direction="horizontal" width="fill">
                        <Text size={13.0} weight={500u16} color={Color::BLACK}>"Axes"</Text>
                        <Group width="fill" />
                        <Text size={10.0} color={0x525252}>"Reset"</Text>
                    </Group>
                    {axis_slider("Weight (wght)", "400")}
                    {axis_slider("Width (wdth)", "100%")}
                    {axis_slider("Optical Size (opsz)", "50pt")}
                </Group>
            </Group>
        </Group>
    }
}

fn tab(label: &'static str, active: bool) -> impl IntoElement {
    view! {
        <Group width="fill" height="fill" alignment="center" distribution="center">
            <Text
                size={13.0}
                weight={if active { 500u16 } else { 400u16 }}
                color={if active {
                    Color::BLACK
                } else {
                    Color::hex(0x888888)
                }}
            >{label}</Text>
        </Group>
    }
}

fn key_value(
    key: &'static str,
    val: &'static str,
    icon: Option<&std::sync::Arc<VectorGraphic>>,
) -> impl IntoElement {
    view! {
        <Group direction="horizontal" width="fill" alignment="center">
            <Group width={74.0}>
                <Text size={13.0} color={Color::BLACK}>{key}</Text>
            </Group>
            <Text size={13.0} color={0x888888}>{val}</Text>
            <Group width="fill" />
            {if let Some(ic) = icon {
                svg(ic).size(10.0, 10.0).into_element()
            } else {
                group().into_element()
            }}
        </Group>
    }
}

fn metric_tile(label: &'static str, val: &'static str) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={32.0}
            radius={8.0}
            fill={Color::hex_alpha(0x08090C, 0.03)}
            alignment="center"
            margin={Margin::x(8.0)}
        >
            <Text size={12.0} color={0x525252}>{label}</Text>
            <Group width="fill" />
            <Text size={12.0} weight={500u16} color={0x0A0A0A}>{val}</Text>
        </Group>
    }
}

fn axis_slider(label: &'static str, val: &'static str) -> impl IntoElement {
    view! {
        <Group direction="vertical" width="fill" gap={6.0}>
            <Group direction="horizontal" width="fill">
                <Text size={11.0} color={0x525252}>{label}</Text>
                <Group width="fill" />
                <Text size={11.0} weight={700u16} color={0x0A0A0A}>{val}</Text>
            </Group>
            <Group
                width="fill"
                height={6.0}
                radius={3.0}
                fill={Color::hex_alpha(0x08090C, 0.04)}
            >
                <Group
                    width={Size::percent(0.50)}
                    height="fill"
                    radius={3.0}
                    fill={0x1019EC}
                />
            </Group>
        </Group>
    }
}
