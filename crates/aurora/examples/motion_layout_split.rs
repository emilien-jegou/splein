// Single responsibility: Showcase static layout running once while a motion transform runs per frame.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::time::Instant;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 440;

/// Slides a keyed card on the compositor while its layout box stays frozen.
struct Pendulum {
    start: Instant,
}

impl AppExtension for Pendulum {
    fn on_update(&mut self, engine: &mut Engine) -> bool {
        let phase = self.start.elapsed().as_secs_f32() * 1.4;
        engine.update_motion("card", |motion| {
            motion.transform = Transform::from_translation(phase.sin() * 120.0, 0.0);
        });
        true
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 01 · Layout vs compositor transform")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .extension(Pendulum {
            start: Instant::now(),
        })
        .run(build_ui);
}

/// Builds the page shown for this concept.
pub fn build_ui() -> impl IntoElement {
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
                        .children([arena(card())]),
                )),
        ),
    )
}

fn card() -> impl IntoElement {
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
            text("STABLE VIEW").size(11.0).weight(700).color(Color::white_alpha(0.70)),
            text("inspector-panel").size(17.0).weight(700).color(Color::WHITE),
            text("layout fixed · transform animated")
                .size(12.0)
                .color(Color::white_alpha(0.72)),
        ))
}
