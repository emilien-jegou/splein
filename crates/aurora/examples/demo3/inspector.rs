// Single responsibility: Right details inspector displaying metrics and variable font sliders.

use super::icons::AppIcons;
use aurora::prelude::*;

pub fn render_inspector(icons: &AppIcons) -> impl IntoElement {
    group()
        .direction("horizontal")
        .width(361.0)
        .height("fill")
        .children((
            group()
                .width(1.0)
                .height("fill")
                .fill(Color::hex_alpha(0x08090C, 0.06)),
            group()
                .direction("vertical")
                .width(360.0)
                .height("fill")
                .fill(0xFCFCFC)
                .children((
                    group()
                        .direction("horizontal")
                        .width("fill")
                        .alignment("center")
                        .margin(Margin::x(16.0))
                        .margin(Margin::top(16.0))
                        .children((
                            text("Universal Sans")
                                .size(16.0)
                                .weight(500)
                                .color(0x0A0A0A),
                            group().width("fill"),
                            group()
                                .direction("horizontal")
                                .height(24.0)
                                .radius(6.0)
                                .fill(0xE9FFEB)
                                .alignment("center")
                                .gap(6.0)
                                .children((
                                    group()
                                        .width(6.0)
                                        .height(6.0)
                                        .radius("max")
                                        .fill(0x008E2D)
                                        .margin(Margin::left(8.0)),
                                    text("Active")
                                        .size(12.0)
                                        .weight(500)
                                        .color(0x025719)
                                        .margin(Margin::right(8.0)),
                                )),
                        )),
                    group()
                        .direction("horizontal")
                        .width("fill")
                        .height(32.0)
                        .margin(Margin::top(8.0))
                        .children((
                            tab("General", true),
                            tab("Variants", false),
                            tab("Instances", false),
                            tab("Alternatives", false),
                        )),
                    group()
                        .width("fill")
                        .height(1.0)
                        .fill(Color::hex_alpha(0x08090C, 0.06)),
                    group()
                        .direction("vertical")
                        .width("fill")
                        .margin(Margin::x(16.0))
                        .margin(Margin::top(16.0))
                        .gap(14.0)
                        .children((
                            text("Details").size(13.0).weight(500).color(Color::BLACK),
                            key_value("Font name", "UniversalSans-Regular", None),
                            key_value("Designer", "Jhon Batista", Some(&icons.link)),
                            key_value("Publisher", "BatistaCorp", Some(&icons.link)),
                        )),
                    group()
                        .width("fill")
                        .height(1.0)
                        .fill(Color::hex_alpha(0x08090C, 0.06))
                        .margin(Margin::y(16.0)),
                    group()
                        .direction("vertical")
                        .width("fill")
                        .margin(Margin::x(16.0))
                        .gap(10.0)
                        .children((
                            text("Metrics").size(13.0).weight(500).color(Color::BLACK),
                            group()
                                .direction("horizontal")
                                .width("fill")
                                .gap(8.0)
                                .children((
                                    metric_tile("Ascender", "910"),
                                    metric_tile("Cap Height", "690"),
                                )),
                            group()
                                .direction("horizontal")
                                .width("fill")
                                .gap(8.0)
                                .children((
                                    metric_tile("x-Height", "540"),
                                    metric_tile("Descender", "-210"),
                                )),
                        )),
                    group()
                        .width("fill")
                        .height(1.0)
                        .fill(Color::hex_alpha(0x08090C, 0.06))
                        .margin(Margin::y(16.0)),
                    group()
                        .direction("vertical")
                        .width("fill")
                        .margin(Margin::x(16.0))
                        .gap(12.0)
                        .children((
                            group()
                                .direction("horizontal")
                                .width("fill")
                                .children((
                                    text("Axes").size(13.0).weight(500).color(Color::BLACK),
                                    group().width("fill"),
                                    text("Reset").size(10.0).color(0x525252),
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
        .width("fill")
        .height("fill")
        .alignment("center")
        .distribution("center")
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
    group()
        .direction("horizontal")
        .width("fill")
        .alignment("center")
        .children((
            group()
                .width(74.0)
                .children([text(key).size(13.0).color(Color::BLACK)]),
            text(val).size(13.0).color(0x888888),
            group().width("fill"),
            if let Some(ic) = icon {
                svg(ic).size(10.0, 10.0).into_element()
            } else {
                group().into_element()
            },
        ))
}

fn metric_tile(label: &'static str, val: &'static str) -> impl IntoElement {
    group()
        .direction("horizontal")
        .width("fill")
        .height(32.0)
        .radius(8.0)
        .fill(Color::hex_alpha(0x08090C, 0.03))
        .alignment("center")
        .margin(Margin::x(8.0))
        .children((
            text(label).size(12.0).color(0x525252),
            group().width("fill"),
            text(val).size(12.0).weight(500).color(0x0A0A0A),
        ))
}

fn axis_slider(label: &'static str, val: &'static str) -> impl IntoElement {
    group()
        .direction("vertical")
        .width("fill")
        .gap(6.0)
        .children((
            group()
                .direction("horizontal")
                .width("fill")
                .children((
                    text(label).size(11.0).color(0x525252),
                    group().width("fill"),
                    text(val).size(11.0).weight(700).color(0x0A0A0A),
                )),
            group()
                .width("fill")
                .height(6.0)
                .radius(3.0)
                .fill(Color::hex_alpha(0x08090C, 0.04))
                .children([group()
                    .width(Size::percent(0.50))
                    .height("fill")
                    .radius(3.0)
                    .fill(0x1019EC)]),
        ))
}
