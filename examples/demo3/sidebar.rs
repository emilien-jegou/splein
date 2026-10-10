// Single responsibility: Left navigation sidebar with category pills and local storage progress.

use super::icons::AppIcons;
use aurora::prelude::*;

pub fn render_sidebar(icons: &AppIcons) -> impl IntoElement {
    view! {
        <Group direction="horizontal" width={261.0} height="fill">
            <Group direction="vertical" width={260.0} height="fill" margin={12.0}>
                <Group direction="vertical" width="fill" gap={24.0}>
                    <Group
                        direction="horizontal"
                        width="fill"
                        alignment="center"
                        gap={10.0}
                        margin={Margin::x(4.0)}
                    >
                        {svg(&icons.logo).size(24.0, 24.0)}
                        <Text size={18.0} weight={700u16} color={0x0A0A0A}>"Optype"</Text>
                    </Group>
                    <Group direction="vertical" width="fill" gap={2.0}>
                        {nav_item(icons, "All Fonts", "8", true)}
                        {nav_item_with_status("Active Fonts", "0", 0x009530)}
                        {nav_item_with_star(icons, "Favorites", "18")}
                    </Group>
                    <Group direction="vertical" width="fill" gap={2.0}>
                        <Text size={12.0} color={0x999999} margin={Margin::left(4.0)}>
                            "Folders"
                        </Text>
                        {folder_item(icons, "MySans", "18")}
                        {folder_item(icons, "MySerif", "8")}
                    </Group>
                </Group>
                <Group height="fill" />
                {storage_card(icons)}
            </Group>
            <Group
                width={1.0}
                height="fill"
                fill={Color::hex_alpha(0x08090C, 0.06)}
            />
        </Group>
    }
}

fn icon_slot(child: impl IntoElement) -> GroupDef {
    group()
        .width(18.0)
        .height(18.0)
        .alignment("center")
        .distribution("center")
        .margin(Margin::left(6.0))
        .children([child])
}

fn nav_item(
    icons: &AppIcons,
    label: &'static str,
    count: &'static str,
    active: bool,
) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={32.0}
            radius={10.0}
            fill={if active {
                Color::hex_alpha(0x08090C, 0.06)
            } else {
                Color::TRANSPARENT
            }}
            alignment="center"
            margin={Margin::x(4.0)}
        >
            {icon_slot(svg(&icons.storage).size(16.0, 16.0))}
            <Text size={14.0} weight={if active { 500u16 } else { 400u16 }} color={0x0A0A0A}>
                {label}
            </Text>
            <Group width="fill" />
            <Text size={12.0} color={0x999999} margin={Margin::right(10.0)}>{count}</Text>
        </Group>
    }
}

fn nav_item_with_status(
    label: &'static str,
    count: &'static str,
    dot: impl Into<Color>,
) -> impl IntoElement {
    let dot = dot.into();
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={32.0}
            radius={10.0}
            alignment="center"
            gap={10.0}
            margin={Margin::x(4.0)}
        >
            {icon_slot(group().width(7.0).height(7.0).radius("max").fill(dot).shadow(
                Shadow::from("0 0 4 0 #D5FFD7"),
            ))}
            <Text size={14.0} color={0x0A0A0A}>{label}</Text>
            <Group width="fill" />
            <Text size={12.0} color={0x999999} margin={Margin::right(10.0)}>{count}</Text>
        </Group>
    }
}

fn nav_item_with_star(
    icons: &AppIcons,
    label: &'static str,
    count: &'static str,
) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={32.0}
            radius={10.0}
            alignment="center"
            gap={10.0}
            margin={Margin::x(4.0)}
        >
            {icon_slot(svg(&icons.star).size(15.0, 15.0))}
            <Text size={14.0} color={0x0A0A0A}>{label}</Text>
            <Group width="fill" />
            <Text size={12.0} color={0x999999} margin={Margin::right(10.0)}>{count}</Text>
        </Group>
    }
}

fn folder_item(icons: &AppIcons, name: &'static str, count: &'static str) -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height={32.0}
            radius={10.0}
            alignment="center"
            gap={10.0}
            margin={Margin::x(4.0)}
        >
            {icon_slot(svg(&icons.folder).size(18.0, 18.0))}
            <Text size={14.0} color={0x0A0A0A}>{name}</Text>
            <Group width="fill" />
            <Text size={12.0} color={0x999999} margin={Margin::right(10.0)}>{count}</Text>
        </Group>
    }
}

fn storage_card(icons: &AppIcons) -> impl IntoElement {
    view! {
        <Group direction="vertical" width="fill" gap={8.0} margin={12.0}>
            <Group direction="horizontal" alignment="center" gap={6.0}>
                {svg(&icons.storage).size(15.0, 15.0)}
                <Text size={13.0} weight={500u16} color={Color::BLACK}>"Local Storage"</Text>
            </Group>
            <Group
                width="fill"
                height={6.0}
                radius="max"
                fill={Color::hex_alpha(0x08090C, 0.06)}
            >
                <Group
                    width={Size::percent(0.70)}
                    height="fill"
                    radius="max"
                    fill={0x4F4F4F}
                />
            </Group>
            <Group direction="horizontal" width="fill" alignment="center">
                <Text size={10.0} color={0x737373}>"15MB used of 2048MB"</Text>
                <Group width="fill" />
                <Text size={10.0} weight={500u16} color={0x1019EC}>"Clear"</Text>
            </Group>
        </Group>
    }
}
