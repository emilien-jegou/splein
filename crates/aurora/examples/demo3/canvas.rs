// Single responsibility: Center canvas hosting search bar, responsive font grid, and dock.

use super::hud::render_floating_hud;
use super::icons::AppIcons;
use aurora::prelude::*;

pub fn render_canvas(icons: &AppIcons) -> impl IntoElement {
    group().width(Size::fill()).fill_height().fill(Color::WHITE).children((
        column().fill_parent().children((
            render_toolbar(icons),
            group().fill_width().height(1.0).fill(Color::hex_alpha(0x08090C, 0.06)),
            column().fill_parent().margin_x(16.0).margin_top(16.0).gap(16.0).children((
                card_row(icons, "Inter", "Aa"),
                card_row(icons, "Inter", "Aa"),
                card_row(icons, "Arial (6)", "Aa"),
                row().fill_width().height(80.0).center().children([
                    text("All 2 font families loaded").size(12.0).color(Color::hex(0x999999)),
                ]),
            )),
        )),
        render_floating_hud(),
    ))
}

fn render_toolbar(icons: &AppIcons) -> impl IntoElement {
    row().fill_width().height(52.0).align_center().gap(12.0).margin_x(16.0).children((
        row().width(344.0).height(32.0).radius(Radius::max()).fill(Color::hex_alpha(0x999999, 0.12)).align_center().gap(10.0).children((
            svg(&icons.search).size(16.0, 16.0).margin_left(10.0),
            text("Search font, family, tag...").size(14.0).color(Color::hex(0x7B7B7B)),
        )),
        row().height(32.0).radius(Radius::max()).stroke(Stroke::inside(1.0, Color::hex_alpha(0x000000, 0.1))).center().gap(6.0).children((
            svg(&icons.filter).size(16.0, 16.0).margin_left(14.0),
            text("Filters").size(14.0).weight(500).color(Color::hex(0x0A0A0A)).margin_right(14.0),
        )),
        row().width(Size::fill()).height(32.0).align_center().gap(12.0).children((
            group().width(Size::fill()).height(6.0).radius(3.0).fill(Color::hex_alpha(0x000000, 0.07)).children([
                group().width(Size::percent(0.40)).fill_height().radius(3.0).fill(Color::hex(0x1019EC)),
            ]),
            text("24px").size(14.0).color(Color::hex(0x2B2B2B)),
        )),
        row().height(32.0).radius(Radius::max()).fill(Color::hex_alpha(0x08090C, 0.04)).align_center().children((
            group().height(28.0).radius(Radius::max()).fill(Color::WHITE).shadow(Shadow::outer(0.0, 1.0, 3.0, Color::hex_alpha(0x000000, 0.1))).center().children([
                text("Grid").size(14.0).weight(500).color(Color::hex(0x0A0A0A)).margin_x(12.0),
            ]),
            group().height(28.0).radius(Radius::max()).center().children([
                text("List").size(12.0).color(Color::hex(0x525252)).margin_x(12.0),
            ]),
        )),
    ))
}

fn card_row(icons: &AppIcons, name: &'static str, sample: &'static str) -> impl IntoElement {
    row().fill_width().gap(12.0).children((
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
    ))
}

fn font_card(icons: &AppIcons, name: &'static str, sample: &'static str) -> impl IntoElement {
    column()
        .width(Size::fill())
        .height(220.0)
        .radius(16.0)
        .fill(Color::WHITE)
        .stroke(Stroke::inside(1.0, Color::hex_alpha(0x08090C, 0.06)))
        .children((
            row().fill_width().align_center().margin_x(8.0).margin_top(8.0).children((
                group().size(6.0, 6.0).radius(Radius::max()).fill(Color::hex(0x999999)),
                text("V").size(13.0).weight(700).color(Color::hex(0x1019EC)).margin_left(4.0),
                text(name).size(13.0).weight(500).color(Color::hex(0x0A0A0A)).margin_left(4.0),
                group().width(Size::fill()),
                svg(&icons.star).size(15.0, 15.0),
            )),
            group().fill_parent().center().children([
                text(sample).size(90.0).color(Color::hex(0x0A0A0A)),
            ]),
        ))
}
