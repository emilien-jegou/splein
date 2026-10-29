// Single responsibility: Center canvas hosting search bar, responsive font grid, and dock.

use super::hud::render_floating_hud;
use super::icons::AppIcons;
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this file composes repr-builders directly

pub fn render_canvas(icons: &AppIcons) -> impl IntoElement {
    view! {
        <Group width="fill" height="fill" fill={Color::WHITE}>
            <Group direction="vertical" width="fill" height="fill">
                {render_toolbar(icons)}
                <Group
                    width="fill"
                    height={1.0}
                    fill={Color::hex_alpha(0x08090C, 0.06)}
                />
                <Group
                    direction="vertical"
                    width="fill"
                    height="fill"
                    margin={16.0}
                    gap={16.0}
                >
                    {card_row(icons, "Inter", "Aa")}
                    {card_row(icons, "Inter", "Aa")}
                    {card_row(icons, "Arial (6)", "Aa")}
                    <Group
                        direction="horizontal"
                        width="fill"
                        height={80.0}
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={12.0} color={0x999999}>"All 2 font families loaded"</Text>
                    </Group>
                </Group>
            </Group>
            {render_floating_hud()}
        </Group>
    }
}

fn render_toolbar(icons: &AppIcons) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={52.0}
            alignment="center"
            gap={12.0}
            margin={Margin::x(16.0)}
        >
            <Group
                direction="horizontal"
                width={344.0}
                height={32.0}
                radius="max"
                fill={Color::hex_alpha(0x999999, 0.12)}
                alignment="center"
                gap={10.0}
            >
                {svg(&icons.search).size(16.0, 16.0).margin(Margin::left(10.0))}
                <Text size={14.0} color={0x7B7B7B}>"Search font, family, tag..."</Text>
            </Group>
            <Group
                direction="horizontal"
                height={32.0}
                radius="max"
                stroke={Stroke::inside(1.0, Color::hex_alpha(0x000000, 0.1))}
                alignment="center"
                distribution="center"
                gap={6.0}
            >
                {svg(&icons.filter).size(16.0, 16.0).margin(Margin::left(14.0))}
                <Text
                    size={14.0}
                    weight={500u16}
                    color={0x0A0A0A}
                    margin={Margin::right(14.0)}
                >"Filters"</Text>
            </Group>
            <Group
                direction="horizontal"
                width="fill"
                height={32.0}
                alignment="center"
                gap={12.0}
            >
                <Group
                    width="fill"
                    height={6.0}
                    radius={3.0}
                    fill={Color::hex_alpha(0x000000, 0.07)}
                >
                    <Group
                        width={Size::percent(0.40)}
                        height="fill"
                        radius={3.0}
                        fill={0x1019EC}
                    />
                </Group>
                <Text size={14.0} color={0x2B2B2B}>"24px"</Text>
            </Group>
            <Group
                direction="horizontal"
                height={32.0}
                radius="max"
                fill={Color::hex_alpha(0x08090C, 0.04)}
                alignment="center"
            >
                <Group
                    height={28.0}
                    radius="max"
                    fill={Color::WHITE}
                    shadow={Shadow::web_like(0.0, 1.0, 3.0, 0.0, Color::hex_alpha(0x000000, 0.1))}
                    alignment="center"
                    distribution="center"
                >
                    <Text
                        size={14.0}
                        weight={500u16}
                        color={0x0A0A0A}
                        margin={Margin::x(12.0)}
                    >"Grid"</Text>
                </Group>
                <Group
                    height={28.0}
                    radius="max"
                    alignment="center"
                    distribution="center"
                >
                    <Text
                        size={12.0}
                        color={0x525252}
                        margin={Margin::x(12.0)}
                    >"List"</Text>
                </Group>
            </Group>
        </Group>
    }
}

fn card_row(icons: &AppIcons, name: &'static str, sample: &'static str) -> impl IntoElement {
    view! {
        <Group direction="horizontal" width="fill" gap={12.0}>
            {font_card(icons, name, sample)}
            {font_card(icons, name, sample)}
            {font_card(icons, name, sample)}
            {font_card(icons, name, sample)}
            {font_card(icons, name, sample)}
            {font_card(icons, name, sample)}
        </Group>
    }
}

fn font_card(icons: &AppIcons, name: &'static str, sample: &'static str) -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height={220.0}
            radius={16.0}
            fill={Color::WHITE}
            stroke={Stroke::inside(1.0, Color::hex_alpha(0x08090C, 0.06))}
        >
            <Group
                direction="horizontal"
                width="fill"
                alignment="center"
                margin={8.0}
            >
                <Group width={6.0} height={6.0} radius="max" fill={0x999999} />
                <Text
                    size={13.0}
                    weight={700u16}
                    color={0x1019EC}
                    margin={Margin::left(4.0)}
                >"V"</Text>
                <Text
                    size={13.0}
                    weight={500u16}
                    color={0x0A0A0A}
                    margin={Margin::left(4.0)}
                >{name}</Text>
                <Group width="fill" />
                {svg(&icons.star).size(15.0, 15.0)}
            </Group>
            <Group
                width="fill"
                height="fill"
                alignment="center"
                distribution="center"
            >
                <Text size={90.0} color={0x0A0A0A}>{sample}</Text>
            </Group>
        </Group>
    }
}
