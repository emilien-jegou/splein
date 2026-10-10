// Single responsibility: Floating bottom interactive HUD dock for font inspection and styling.

use aurora::prelude::*;

pub fn render_floating_hud() -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fit"
            height={48.0}
            radius="max"
            fill={0x0A0A0A}
            stroke={Stroke::inside(1.0, Color::white_alpha(0.15))}
            shadow={Shadow::web_like(0.0, 25.0, 50.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.25))}
            anchor={Anchor::Bottom}
            margin={Margin::bottom(12.0)}
            overlay={true}
            z_index={100}
            alignment="center"
            gap={10.0}
        >
            <Group
                direction="horizontal"
                width={280.0}
                height={36.0}
                radius="max"
                stroke={Stroke::inside(1.0, Color::white_alpha(0.15))}
                alignment="center"
                margin={Margin::x(6.0)}
            >
                <Text size={13.0} color={Color::WHITE} margin={Margin::left(10.0)}>
                    "The King asks the Queen to Risk a foxy Gift."
                </Text>
                <Group width="fill" />
                <Text size={11.0} color={0x9B9B9B} margin={Margin::right(10.0)}>"Reset"</Text>
            </Group>
            <Group
                direction="horizontal"
                width={180.0}
                height={36.0}
                radius="max"
                fill={Color::white_alpha(0.10)}
                alignment="center"
                gap={8.0}
            >
                <Text size={10.0} weight={700u16} color={0x999999} margin={Margin::left(8.0)}>
                    "W"
                </Text>
                <Group
                    width="fill"
                    height={6.0}
                    radius={3.0}
                    fill={Color::white_alpha(0.20)}
                />
                <Text size={11.0} color={Color::WHITE} margin={Margin::right(8.0)}>"400"</Text>
            </Group>
            {hud_stepper("A/A", "1.2")}
            {hud_stepper("VA", "0px")}
            <Group width={1.0} height={16.0} fill={Color::white_alpha(0.15)} />
            <Group direction="horizontal" alignment="center" gap={4.0}>
                {hud_button("I")}
                {hud_button("AA")}
                {hud_button("U")}
            </Group>
        </Group>
    }
}

fn hud_stepper(label: &'static str, val: &'static str) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            height={36.0}
            radius="max"
            fill={Color::white_alpha(0.10)}
            alignment="center"
            gap={4.0}
            margin={Margin::x(8.0)}
        >
            <Text size={10.0} weight={700u16} color={0x999999}>{label}</Text>
            <Text size={12.0} color={Color::WHITE}>"-"</Text>
            <Text size={11.0} color={Color::WHITE}>{val}</Text>
            <Text size={12.0} color={Color::WHITE}>"+"</Text>
        </Group>
    }
}

fn hud_button(label: &'static str) -> impl IntoElement {
    view! {
        <Group
            width={28.0}
            height={28.0}
            radius="max"
            alignment="center"
            distribution="center"
        >
            <Text size={12.0} color={Color::white_alpha(0.70)}>{label}</Text>
        </Group>
    }
}
