// Single responsibility: Right details inspector displaying metrics and variable font sliders.

use super::icons::AppIcons;
use aurora::prelude::*;

pub fn render_inspector(icons: &AppIcons) -> impl IntoElement {
    row().width(361.0).fill_height().children((
        group()
            .width(1.0)
            .fill_height()
            .fill(Color::hex_alpha(0x08090C, 0.06)),
        column()
            .width(360.0)
            .fill_height()
            .fill(Color::hex(0xFCFCFC))
            .children((
                row()
                    .fill_width()
                    .align_center()
                    .margin_x(16.0)
                    .margin_top(16.0)
                    .children((
                        text("Universal Sans")
                            .size(16.0)
                            .weight(500)
                            .color(Color::hex(0x0A0A0A)),
                        group().width(Size::fill()),
                        row()
                            .height(24.0)
                            .radius(6.0)
                            .fill(Color::hex(0xE9FFEB))
                            .align_center()
                            .gap(6.0)
                            .children((
                                group()
                                    .size(6.0, 6.0)
                                    .radius(Radius::max())
                                    .fill(Color::hex(0x008E2D))
                                    .margin_left(8.0),
                                text("Active")
                                    .size(12.0)
                                    .weight(500)
                                    .color(Color::hex(0x025719))
                                    .margin_right(8.0),
                            )),
                    )),
                row().fill_width().height(32.0).margin_top(8.0).children((
                    tab("General", true),
                    tab("Variants", false),
                    tab("Instances", false),
                    tab("Alternatives", false),
                )),
                group()
                    .fill_width()
                    .height(1.0)
                    .fill(Color::hex_alpha(0x08090C, 0.06)),
                column()
                    .fill_width()
                    .margin_x(16.0)
                    .margin_top(16.0)
                    .gap(14.0)
                    .children((
                        text("Details").size(13.0).weight(500).color(Color::BLACK),
                        key_value("Font name", "UniversalSans-Regular", None),
                        key_value("Designer", "Jhon Batista", Some(&icons.link)),
                        key_value("Publisher", "BatistaCorp", Some(&icons.link)),
                    )),
                group()
                    .fill_width()
                    .height(1.0)
                    .fill(Color::hex_alpha(0x08090C, 0.06))
                    .margin_y(16.0),
                column().fill_width().margin_x(16.0).gap(10.0).children((
                    text("Metrics").size(13.0).weight(500).color(Color::BLACK),
                    row().fill_width().gap(8.0).children((
                        metric_tile("Ascender", "910"),
                        metric_tile("Cap Height", "690"),
                    )),
                    row().fill_width().gap(8.0).children((
                        metric_tile("x-Height", "540"),
                        metric_tile("Descender", "-210"),
                    )),
                )),
                group()
                    .fill_width()
                    .height(1.0)
                    .fill(Color::hex_alpha(0x08090C, 0.06))
                    .margin_y(16.0),
                column().fill_width().margin_x(16.0).gap(12.0).children((
                    row().fill_width().children((
                        text("Axes").size(13.0).weight(500).color(Color::BLACK),
                        group().width(Size::fill()),
                        text("Reset").size(10.0).color(Color::hex(0x525252)),
                    )),
                    axis_slider("Weight (wght)", "400"),
                    axis_slider("Width (wdth)", "100%"),
                    axis_slider("Optical Size (opsz)", "50pt"),
                )),
            )),
    ))
}

fn tab(label: &'static str, active: bool) -> impl IntoElement {
    group()
        .width(Size::fill())
        .fill_height()
        .center()
        .children([text(label)
            .size(13.0)
            .weight(if active { 500 } else { 400 })
            .color(if active {
                Color::BLACK
            } else {
                Color::hex(0x888888)
            })])
}

fn key_value(
    key: &'static str,
    val: &'static str,
    icon: Option<&std::sync::Arc<VectorGraphic>>,
) -> impl IntoElement {
    row().fill_width().align_center().children((
        group()
            .width(74.0)
            .children([text(key).size(13.0).color(Color::BLACK)]),
        text(val).size(13.0).color(Color::hex(0x888888)),
        group().width(Size::fill()),
        if let Some(ic) = icon {
            svg(ic).size(10.0, 10.0).into_element()
        } else {
            group().into_element()
        },
    ))
}

fn metric_tile(label: &'static str, val: &'static str) -> impl IntoElement {
    row()
        .width(Size::fill())
        .height(32.0)
        .radius(8.0)
        .fill(Color::hex_alpha(0x08090C, 0.03))
        .align_center()
        .margin_x(8.0)
        .children((
            text(label).size(12.0).color(Color::hex(0x525252)),
            group().width(Size::fill()),
            text(val).size(12.0).weight(500).color(Color::hex(0x0A0A0A)),
        ))
}

fn axis_slider(label: &'static str, val: &'static str) -> impl IntoElement {
    column().fill_width().gap(6.0).children((
        row().fill_width().children((
            text(label).size(11.0).color(Color::hex(0x525252)),
            group().width(Size::fill()),
            text(val).size(11.0).weight(700).color(Color::hex(0x0A0A0A)),
        )),
        group()
            .fill_width()
            .height(6.0)
            .radius(3.0)
            .fill(Color::hex_alpha(0x08090C, 0.04))
            .children([group()
                .width(Size::percent(0.50))
                .fill_height()
                .radius(3.0)
                .fill(Color::hex(0x1019EC))]),
    ))
}
