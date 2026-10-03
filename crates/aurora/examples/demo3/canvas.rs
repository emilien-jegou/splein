// Single responsibility: Center canvas hosting search bar, responsive font grid, and dock.

use super::hud::render_floating_hud;
use super::icons::AppIcons;
use aurora::prelude::*;

pub fn render_canvas(icons: &AppIcons) -> impl IntoElement {
    group().width(Size::fill()).height(Size::fill()).fill(Color::WHITE).children((
        group().direction(Direction::Vertical).width(Size::fill()).height(Size::fill()).children((
            render_toolbar(icons),
            group().width(Size::fill()).height(1.0).fill(Color::hex_alpha(0x08090C, 0.06)),
            group().direction(Direction::Vertical).width(Size::fill()).height(Size::fill()).margin(Margin::all(16.0)).gap(16.0).children((
                card_row(icons, "Inter", "Aa"),
                card_row(icons, "Inter", "Aa"),
                card_row(icons, "Arial (6)", "Aa"),
                group().direction(Direction::Horizontal).width(Size::fill()).height(80.0).alignment(Alignment::Center).distribution(Distribution::Center).children([
                    text("All 2 font families loaded").size(12.0).color(Color::hex(0x999999)),
                ]),
            )),
        )),
        render_floating_hud(),
    ))
}

fn render_toolbar(icons: &AppIcons) -> impl IntoElement {
    group().direction(Direction::Horizontal).width(Size::fill()).height(52.0).alignment(Alignment::Center).gap(12.0).margin(Margin::x(16.0)).children((
        group().direction(Direction::Horizontal).width(344.0).height(32.0).radius(Radius::max()).fill(Color::hex_alpha(0x999999, 0.12)).alignment(Alignment::Center).gap(10.0).children((
            svg(&icons.search).size(16.0, 16.0).margin(Margin::left(10.0)),
            text("Search font, family, tag...").size(14.0).color(Color::hex(0x7B7B7B)),
        )),
        group().direction(Direction::Horizontal).height(32.0).radius(Radius::max()).stroke(Stroke::inside(1.0, Color::hex_alpha(0x000000, 0.1))).alignment(Alignment::Center).distribution(Distribution::Center).gap(6.0).children((
            svg(&icons.filter).size(16.0, 16.0).margin(Margin::left(14.0)),
            text("Filters").size(14.0).weight(500).color(Color::hex(0x0A0A0A)).margin(Margin::right(14.0)),
        )),
        group().direction(Direction::Horizontal).width(Size::fill()).height(32.0).alignment(Alignment::Center).gap(12.0).children((
            group().width(Size::fill()).height(6.0).radius(3.0).fill(Color::hex_alpha(0x000000, 0.07)).children([
                group().width(Size::percent(0.40)).height(Size::fill()).radius(3.0).fill(Color::hex(0x1019EC)),
            ]),
            text("24px").size(14.0).color(Color::hex(0x2B2B2B)),
        )),
        group().direction(Direction::Horizontal).height(32.0).radius(Radius::max()).fill(Color::hex_alpha(0x08090C, 0.04)).alignment(Alignment::Center).children((
            group().height(28.0).radius(Radius::max()).fill(Color::WHITE).shadows([Shadow::outer(0.0, 1.0, 3.0, Color::hex_alpha(0x000000, 0.1))]).alignment(Alignment::Center).distribution(Distribution::Center).children([
                text("Grid").size(14.0).weight(500).color(Color::hex(0x0A0A0A)).margin(Margin::x(12.0)),
            ]),
            group().height(28.0).radius(Radius::max()).alignment(Alignment::Center).distribution(Distribution::Center).children([
                text("List").size(12.0).color(Color::hex(0x525252)).margin(Margin::x(12.0)),
            ]),
        )),
    ))
}

fn card_row(icons: &AppIcons, name: &'static str, sample: &'static str) -> impl IntoElement {
    group().direction(Direction::Horizontal).width(Size::fill()).gap(12.0).children((
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
        font_card(icons, name, sample),
    ))
}

fn font_card(icons: &AppIcons, name: &'static str, sample: &'static str) -> impl IntoElement {
    group().direction(Direction::Vertical)
        .width(Size::fill())
        .height(220.0)
        .radius(16.0)
        .fill(Color::WHITE)
        .stroke(Stroke::inside(1.0, Color::hex_alpha(0x08090C, 0.06)))
        .children((
            group().direction(Direction::Horizontal).width(Size::fill()).alignment(Alignment::Center).margin(Margin::all(8.0)).children((
                group().width(6.0).height(6.0).radius(Radius::max()).fill(Color::hex(0x999999)),
                text("V").size(13.0).weight(700).color(Color::hex(0x1019EC)).margin(Margin::left(4.0)),
                text(name).size(13.0).weight(500).color(Color::hex(0x0A0A0A)).margin(Margin::left(4.0)),
                group().width(Size::fill()),
                svg(&icons.star).size(15.0, 15.0),
            )),
            group().width(Size::fill()).height(Size::fill()).alignment(Alignment::Center).distribution(Distribution::Center).children([
                text(sample).size(90.0).color(Color::hex(0x0A0A0A)),
            ]),
        ))
}
