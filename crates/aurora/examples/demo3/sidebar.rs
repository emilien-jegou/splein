// Single responsibility: Left navigation sidebar with category pills and local storage progress.

use super::icons::AppIcons;
use aurora::prelude::*;

pub fn render_sidebar(icons: &AppIcons) -> impl IntoElement {
    row().width(261.0).fill_height().children((
        column()
            .width(260.0)
            .fill_height()
            .margin(Margin::all(12.0))
            .children((
                column().fill_width().gap(24.0).children((
                    row()
                        .fill_width()
                        .align_center()
                        .gap(10.0)
                        .margin_x(4.0)
                        .children((
                            svg(&icons.logo).size(24.0, 24.0),
                            text("Optype")
                                .size(18.0)
                                .weight(700)
                                .color(Color::hex(0x0A0A0A)),
                        )),
                    column().fill_width().gap(2.0).children((
                        nav_item(icons, "All Fonts", "8", true),
                        nav_item_with_status("Active Fonts", "0", Color::hex(0x009530)),
                        nav_item_with_star(icons, "Favorites", "18"),
                    )),
                    column().fill_width().gap(2.0).children((
                        text("Folders")
                            .size(12.0)
                            .color(Color::hex(0x999999))
                            .margin_left(4.0),
                        folder_item(icons, "MySans", "18"),
                        folder_item(icons, "MySerif", "8"),
                    )),
                )),
                group().height(Size::fill()),
                storage_card(icons),
            )),
        group()
            .width(1.0)
            .fill_height()
            .fill(Color::hex_alpha(0x08090C, 0.06)),
    ))
}

fn icon_slot(child: impl IntoElement) -> GroupDef {
    group()
        .size(18.0, 18.0)
        .center()
        .margin_left(6.0)
        .children([child])
}

fn nav_item(
    icons: &AppIcons,
    label: &'static str,
    count: &'static str,
    active: bool,
) -> impl IntoElement {
    row()
        .fill_width()
        .height(32.0)
        .radius(10.0)
        .fill(if active {
            Color::hex_alpha(0x08090C, 0.06)
        } else {
            Color::TRANSPARENT
        })
        .align_center()
        .margin_x(4.0)
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
                .margin_right(10.0),
        ))
}

fn nav_item_with_status(label: &'static str, count: &'static str, dot: Color) -> impl IntoElement {
    row()
        .fill_width()
        .height(32.0)
        .radius(10.0)
        .align_center()
        .gap(10.0)
        .margin_x(4.0)
        .children((
            icon_slot(
                group()
                    .size(7.0, 7.0)
                    .radius(Radius::max())
                    .fill(dot)
                    .shadow(Shadow::outer(0.0, 0.0, 4.0, Color::hex(0xD5FFD7))),
            ),
            text(label).size(14.0).color(Color::hex(0x0A0A0A)),
            group().width(Size::fill()),
            text(count)
                .size(12.0)
                .color(Color::hex(0x999999))
                .margin_right(10.0),
        ))
}

fn nav_item_with_star(
    icons: &AppIcons,
    label: &'static str,
    count: &'static str,
) -> impl IntoElement {
    row()
        .fill_width()
        .height(32.0)
        .radius(10.0)
        .align_center()
        .gap(10.0)
        .margin_x(4.0)
        .children((
            icon_slot(svg(&icons.star).size(15.0, 15.0)),
            text(label).size(14.0).color(Color::hex(0x0A0A0A)),
            group().width(Size::fill()),
            text(count)
                .size(12.0)
                .color(Color::hex(0x999999))
                .margin_right(10.0),
        ))
}

fn folder_item(icons: &AppIcons, name: &'static str, count: &'static str) -> impl IntoElement {
    row()
        .fill_width()
        .height(32.0)
        .radius(10.0)
        .align_center()
        .gap(10.0)
        .margin_x(4.0)
        .children((
            icon_slot(svg(&icons.folder).size(18.0, 18.0)),
            text(name).size(14.0).color(Color::hex(0x0A0A0A)),
            group().width(Size::fill()),
            text(count)
                .size(12.0)
                .color(Color::hex(0x999999))
                .margin_right(10.0),
        ))
}

fn storage_card(icons: &AppIcons) -> impl IntoElement {
    column()
        .fill_width()
        .gap(8.0)
        .margin(Margin::all(12.0))
        .children((
            row().align_center().gap(6.0).children((
                svg(&icons.storage).size(15.0, 15.0),
                text("Local Storage")
                    .size(13.0)
                    .weight(500)
                    .color(Color::BLACK),
            )),
            group()
                .fill_width()
                .height(6.0)
                .radius(Radius::max())
                .fill(Color::hex_alpha(0x08090C, 0.06))
                .children([group()
                    .width(Size::percent(0.70))
                    .fill_height()
                    .radius(Radius::max())
                    .fill(Color::hex(0x4F4F4F))]),
            row().fill_width().align_center().children((
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
