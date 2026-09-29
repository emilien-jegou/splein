// Single responsibility: Floating bottom interactive HUD dock for font inspection and styling.

use aurora::prelude::*;

pub fn render_floating_hud() -> impl IntoElement {
    row()
        .fit_width()
        .height(48.0)
        .radius(Radius::max())
        .fill(Color::hex(0x0A0A0A))
        .stroke(Stroke::inside(1.0, Color::white_alpha(0.15)))
        .shadow(Shadow::outer(
            0.0,
            25.0,
            50.0,
            Color::rgba(0.0, 0.0, 0.0, 0.25),
        ))
        .anchor(Anchor::Bottom)
        .margin_bottom(12.0)
        .overlay()
        .align_center()
        .gap(10.0)
        .margin_x(6.0)
        .children((
            row()
                .width(280.0)
                .height(36.0)
                .radius(Radius::max())
                .stroke(Stroke::inside(1.0, Color::white_alpha(0.15)))
                .align_center()
                .children((
                    text("The King asks the Queen to Risk a foxy Gift.")
                        .size(13.0)
                        .color(Color::WHITE)
                        .margin_left(10.0),
                    group().width(Size::fill()),
                    text("Reset")
                        .size(11.0)
                        .color(Color::hex(0x9B9B9B))
                        .margin_right(10.0),
                )),
            row()
                .width(180.0)
                .height(36.0)
                .radius(Radius::max())
                .fill(Color::white_alpha(0.10))
                .align_center()
                .gap(8.0)
                .children((
                    text("W")
                        .size(10.0)
                        .weight(700)
                        .color(Color::hex(0x999999))
                        .margin_left(8.0),
                    group()
                        .width(Size::fill())
                        .height(6.0)
                        .radius(3.0)
                        .fill(Color::white_alpha(0.20)),
                    text("400").size(11.0).color(Color::WHITE).margin_right(8.0),
                )),
            hud_stepper("A/A", "1.2"),
            hud_stepper("VA", "0px"),
            group().size(1.0, 16.0).fill(Color::white_alpha(0.15)),
            row().align_center().gap(4.0).children((
                hud_button("I"),
                hud_button("AA"),
                hud_button("U"),
            )),
        ))
}

fn hud_stepper(label: &'static str, val: &'static str) -> impl IntoElement {
    row()
        .height(36.0)
        .radius(Radius::max())
        .fill(Color::white_alpha(0.10))
        .align_center()
        .gap(4.0)
        .margin_x(8.0)
        .children((
            text(label)
                .size(10.0)
                .weight(700)
                .color(Color::hex(0x999999)),
            text("-").size(12.0).color(Color::WHITE),
            text(val).size(11.0).color(Color::WHITE),
            text("+").size(12.0).color(Color::WHITE),
        ))
}

fn hud_button(label: &'static str) -> impl IntoElement {
    group()
        .size(28.0, 28.0)
        .radius(Radius::max())
        .center()
        .children([text(label).size(12.0).color(Color::white_alpha(0.70))])
}
