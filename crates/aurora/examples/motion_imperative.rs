// Single responsibility: Showcase one target pose driving several independent property streams.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 820;
pub const HEIGHT: u32 = 460;
const STEP_SECS: f32 = 1.7;
const TRAVEL: f32 = 180.0;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 03 · Reactive targets")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// Two views, one boolean, five independent streams.
///
/// The pose is declared as data rather than issued as a command, so each property gets to
/// choose its own curve. The panel's three properties share one tween; the badge deliberately
/// does not, and settles on a spring instead.
#[component]
pub fn Page() -> impl IntoElement {
    let outward = use_interval_toggle(STEP_SECS);

    // Panel: one tween, applied to each property independently.
    let panel_ease = Transition::ease(650.ms(), Ease::OutQuint);
    let x = panel_ease.on(outward.map(|out| if out { 0.0 } else { TRAVEL }));
    let opacity = panel_ease.on(outward.map(|out| if out { 1.0 } else { 0.4 }));
    let scale = panel_ease.on(outward.map(|out| if out { 1.0 } else { 0.9 }));

    // Badge: a spring, so it carries no duration at all and arrives on its own terms.
    let badge_scale =
        Transition::spring(Spring::bouncy()).on(outward.map(|out| if out { 1.25 } else { 1.0 }));

    page(
        "MOTION · 03 — REACTIVE TARGETS",
        "One target. Several streams.",
        "A single tween drives x, opacity and scale together under one duration and curve. The badge settles on a spring instead, so it carries no fixed duration at all.",
        content(panel(
            strip(
                stat("TARGET", "x · opacity · scale"),
                stat("EASE", "650ms · OutQuint"),
            ),
            body().children((
                panel_card().x(x).opacity(opacity).scale(scale),
                badge().scale(badge_scale),
            )),
        )),
    )
}

fn body() -> GroupDef {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
}

fn panel_card() -> GroupDef {
    group()
        .key("panel")
        .direction(Direction::Vertical)
        .width(280.0)
        .height(Size::fill())
        .radius(14.0)
        .fill(Color::hex(ACCENT))
        .shadow(Shadow::web_like(
            0.0,
            10.0,
            30.0,
            0.0,
            Color::hex_alpha(ACCENT, 0.30),
        ))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(6.0)
        .children((
            text("MULTI-PROPERTY")
                .size(11.0)
                .weight(700)
                .color(Color::white_alpha(0.70)),
            text("panel").size(17.0).weight(700).color(Color::WHITE),
            text("x · opacity · scale")
                .size(12.0)
                .color(Color::white_alpha(0.72)),
        ))
}

fn badge() -> GroupDef {
    group()
        .key("badge")
        .direction(Direction::Vertical)
        .width(96.0)
        .height(96.0)
        .radius(Radius::max())
        .fill(Color::hex(0x10B981))
        .anchor(Anchor::TopRight)
        .margin(Margin::all(24.0))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(4.0)
        .children((
            text("SPRING")
                .size(10.0)
                .weight(700)
                .color(Color::white_alpha(0.75)),
            text("bouncy").size(13.0).weight(700).color(Color::WHITE),
        ))
}
