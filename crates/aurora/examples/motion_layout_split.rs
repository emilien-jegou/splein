// Single responsibility: Showcase static layout running once while a motion transform runs per frame.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 440;
/// Radians per second of the sweep, matching what the page copy claims.
const SWEEP_HZ: f32 = 1.4;
/// Peak travel in logical pixels.
const TRAVEL: f32 = 120.0;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 01 · Layout vs compositor transform")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// The page, written entirely as declarative reactive state.
///
/// The frame clock is the only source of motion, and it feeds a `.x()` binding. Because a
/// transform is a compositor override, nothing here can dirty layout: the card's box is
/// computed once and never recomputed, which is the entire point this demo makes.
#[component]
pub fn Page() -> impl IntoElement {
    let time = use_frame_clock();

    // A pure function of time. No driver, no engine handle, no per-frame closure.
    let travel = time.map(|t| ((t as f32) * SWEEP_HZ).sin() * TRAVEL);

    page(
        "MOTION · 01 — IDENTITY",
        "Layout runs once. Transform runs every frame.",
        "The card holds a fixed 280-wide box at a fixed origin. Only its compositor transform moves, so the layout boundary never re-executes and damage stays inside its old and new bounds.",
        content(
            surface()
                .direction(Direction::Vertical)
                .width(Size::fill())
                .height(Size::fill())
                .children((
                    strip(
                        stat("LAYOUT", "fixed box, computed once"),
                        stat("COMPOSITOR", "translate(sin t) x 120px"),
                    ),
                    group().width(Size::fill()).height(1.0).fill(hairline()),
                    group()
                        .width(Size::fill())
                        .height(Size::fill())
                        .margin(Margin::all(20.0))
                        .children([arena(card().x(travel))]),
                )),
        ),
    )
}

/// The one view that moves. Its box is fixed; only the transform binding changes.
fn card() -> GroupDef {
    group()
        .key("card")
        .direction(Direction::Vertical)
        .width(280.0)
        .height(Size::fill())
        .radius(14.0)
        .fill(Color::hex(ACCENT))
        .shadows([Shadow::outer(
            0.0,
            10.0,
            30.0,
            Color::hex_alpha(ACCENT, 0.35),
        )])
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(6.0)
        .children((
            text("STABLE VIEW")
                .size(11.0)
                .weight(700)
                .color(Color::white_alpha(0.70)),
            text("inspector-panel")
                .size(17.0)
                .weight(700)
                .color(Color::WHITE),
            text("layout fixed · transform animated")
                .size(12.0)
                .color(Color::white_alpha(0.72)),
        ))
}
