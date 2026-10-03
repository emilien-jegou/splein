// Single responsibility: Left navigation sidebar with category pills and local storage progress.

use super::icons::AppIcons;
use aurora::prelude::*;

pub fn render_sidebar(icons: &AppIcons) -> impl IntoElement {
    group().direction(Direction::Horizontal).width(261.0).height(Size::fill()).children((
        group().direction(Direction::Vertical)
            .width(260.0)
            .height(Size::fill())
            .margin(Margin::all(12.0))
            .children((
                group().direction(Direction::Vertical).width(Size::fill()).gap(24.0).children((
                    group().direction(Direction::Horizontal)
                        .width(Size::fill())
                        .alignment(Alignment::Center)
                        .gap(10.0)
                        .margin(Margin::x(4.0))
                        .children((
                            svg(&icons.logo).size(24.0, 24.0),
                            text("Optype")
                                .size(18.0)
                                .weight(700)
                                .color(Color::hex(0x0A0A0A)),
                        )),
                    group().direction(Direction::Vertical).width(Size::fill()).gap(2.0).children((
                        nav_item(icons, "All Fonts", "8", true),
                        nav_item_with_status("Active Fonts", "0", Color::hex(0x009530)),
                        nav_item_with_star(icons, "Favorites", "18"),
                    )),
                    group().direction(Direction::Vertical).width(Size::fill()).gap(2.0).children((
                        text("Folders")
                            .size(12.0)
                            .color(Color::hex(0x999999))
                            .margin(Margin::left(4.0)),
                        folder_item(icons, "MySans", "18"),
                        folder_item(icons, "MySerif", "8"),
                    )),
                )),
                group().height(Size::fill()),
                storage_card(icons),
            )),
        group()
            .width(1.0)
            .height(Size::fill())
            .fill(Color::hex_alpha(0x08090C, 0.06)),
    ))
}

fn icon_slot(child: impl IntoElement) -> GroupDef {
    group()
        .width(18.0).height(18.0)
        .alignment(Alignment::Center).distribution(Distribution::Center)
        .margin(Margin::left(6.0))
        .children([child])
}

fn nav_item(
    icons: &AppIcons,
    label: &'static str,
    count: &'static str,
    active: bool,
) -> impl IntoElement {
    group().direction(Direction::Horizontal)
        .width(Size::fill())
        .height(32.0)
        .radius(10.0)
        .fill(if active {
            Color::hex_alpha(0x08090C, 0.06)
        } else {
            Color::TRANSPARENT
        })
        .alignment(Alignment::Center)
        .margin(Margin::x(4.0))
        .children((
            icon_slot(svg(&icons.storage).size(16.0, 16.0)),
            text(label)
                .size(14.0)
                .weight(if active { 500 } else { 400 })
                .color(Color::hex(0x0A0A0A)),
            group().width(Size::fill()),
            text(count)
                .size(12.0)
                .color(Color::hex(0x999999))
                .margin(Margin::right(10.0)),
        ))
}

fn nav_item_with_status(label: &'static str, count: &'static str, dot: Color) -> impl IntoElement {
    group().direction(Direction::Horizontal)
        .width(Size::fill())
        .height(32.0)
        .radius(10.0)
        .alignment(Alignment::Center)
        .gap(10.0)
        .margin(Margin::x(4.0))
        .children((
            icon_slot(
                group()
                    .width(7.0).height(7.0)
                    .radius(Radius::max())
                    .fill(dot)
                    .shadows([Shadow::outer(0.0, 0.0, 4.0, Color::hex(0xD5FFD7))]),
            ),
            text(label).size(14.0).color(Color::hex(0x0A0A0A)),
            group().width(Size::fill()),
            text(count)
                .size(12.0)
                .color(Color::hex(0x999999))
                .margin(Margin::right(10.0)),
        ))
}

fn nav_item_with_star(
    icons: &AppIcons,
    label: &'static str,
    count: &'static str,
) -> impl IntoElement {
    group().direction(Direction::Horizontal)
        .width(Size::fill())
        .height(32.0)
        .radius(10.0)
        .alignment(Alignment::Center)
        .gap(10.0)
        .margin(Margin::x(4.0))
        .children((
            icon_slot(svg(&icons.star).size(15.0, 15.0)),
            text(label).size(14.0).color(Color::hex(0x0A0A0A)),
            group().width(Size::fill()),
            text(count)
                .size(12.0)
                .color(Color::hex(0x999999))
                .margin(Margin::right(10.0)),
        ))
}

fn folder_item(icons: &AppIcons, name: &'static str, count: &'static str) -> impl IntoElement {
    group().direction(Direction::Horizontal)
        .width(Size::fill())
        .height(32.0)
        .radius(10.0)
        .alignment(Alignment::Center)
        .gap(10.0)
        .margin(Margin::x(4.0))
        .children((
            icon_slot(svg(&icons.folder).size(18.0, 18.0)),
            text(name).size(14.0).color(Color::hex(0x0A0A0A)),
            group().width(Size::fill()),
            text(count)
                .size(12.0)
                .color(Color::hex(0x999999))
                .margin(Margin::right(10.0)),
        ))
}

fn storage_card(icons: &AppIcons) -> impl IntoElement {
    group().direction(Direction::Vertical)
        .width(Size::fill())
        .gap(8.0)
        .margin(Margin::all(12.0))
        .children((
            group().direction(Direction::Horizontal).alignment(Alignment::Center).gap(6.0).children((
                svg(&icons.storage).size(15.0, 15.0),
                text("Local Storage")
                    .size(13.0)
                    .weight(500)
                    .color(Color::BLACK),
            )),
            group()
                .width(Size::fill())
                .height(6.0)
                .radius(Radius::max())
                .fill(Color::hex_alpha(0x08090C, 0.06))
                .children([group()
                    .width(Size::percent(0.70))
                    .height(Size::fill())
                    .radius(Radius::max())
                    .fill(Color::hex(0x4F4F4F))]),
            group().direction(Direction::Horizontal).width(Size::fill()).alignment(Alignment::Center).children((
                text("15MB used of 2048MB")
                    .size(10.0)
                    .color(Color::hex(0x737373)),
                group().width(Size::fill()),
                text("Clear")
                    .size(10.0)
                    .weight(500)
                    .color(Color::hex(0x1019EC)),
            )),
        ))
}
