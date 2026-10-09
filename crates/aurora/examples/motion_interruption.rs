// Single responsibility: Showcase seamless retargeting of an in-flight spring.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 780;
pub const HEIGHT: u32 = 440;
const RETARGET_SECS: f32 = 0.45;
const LEFT: f32 = -180.0;
const RIGHT: f32 = 180.0;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 03c · Seamless interruption")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// A spring retargeted faster than it can settle, so it is always mid-flight.
///
/// Nothing here coordinates the retargets. The interval flips a plain signal; the transition
/// adopts the live position and velocity on its own each time the target changes, which is
/// the whole claim this demo makes.
#[component]
pub fn Page() -> impl IntoElement {
    // A step counter, rather than a boolean, so the two endpoints stay distinguishable.
    let step = use_signal(0u32);
    let target = step.map(|n| if n % 2 == 0 { RIGHT } else { LEFT });
    use_interval(RETARGET_SECS, {
        let step = step.clone();
        move || step.update(|n| *n += 1)
    });

    let x = Transition::spring(Spring::snappy()).on(target);

    page(
        "MOTION · 03C — INTERRUPTION",
        "Retarget mid-flight without a single pop.",
        "A snappy spring needs roughly 600ms to settle, but it is retargeted every 450ms. Each retarget adopts the live position and velocity, so the card never jumps.",
        content(panel(
            strip(
                stat("CADENCE", "retarget every 450ms"),
                stat("CARRY", "position + velocity"),
            ),
            card().x(x),
        )),
    )
}

fn card() -> GroupDef {
    group()
        .key("card")
        .direction(Direction::Vertical)
        .width(180.0)
        .height(Size::fill())
        .radius(16.0)
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
            text("SPRING")
                .size(11.0)
                .weight(700)
                .color(Color::white_alpha(0.70)),
            text("snappy").size(18.0).weight(700).color(Color::WHITE),
            text("retargeted 450ms")
                .size(12.0)
                .color(Color::white_alpha(0.72)),
        ))
}
