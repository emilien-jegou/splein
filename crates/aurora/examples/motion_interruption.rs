// Single responsibility: Showcase seamless retargeting of an in-flight spring.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::time::Instant;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 780;
pub const HEIGHT: u32 = 440;
const RETARGET_SECS: f32 = 0.45;
const LEFT: f32 = -180.0;
const RIGHT: f32 = 180.0;

/// Retargets a running spring faster than it can settle, so it is always interrupted.
struct Retargetor {
    started: Instant,
    step: u32,
}

impl AppExtension for Retargetor {
    fn on_update(&mut self, engine: &mut Engine) -> bool {
        if self.started.elapsed().as_secs_f32() < RETARGET_SECS {
            return true;
        }
        self.started = Instant::now();
        self.step = self.step.wrapping_add(1);
        let x = if self.step % 2 == 0 { RIGHT } else { LEFT };

        // Retargeting samples the instantaneous position and velocity, so nothing pops.
        engine.animate("card").x(x).spring(Spring::snappy()).play();
        true
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 03c · Seamless interruption")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .extension(Retargetor {
            started: Instant::now(),
            step: 0,
        })
        .run(build_ui);
}

/// Builds the page shown for this concept.
pub fn build_ui() -> impl IntoElement {
    page(
        "MOTION · 03C — INTERRUPTION",
        "Retarget mid-flight without a single pop.",
        "A snappy spring needs roughly 600ms to settle, but it is retargeted every 450ms. Each retarget adopts the live position and velocity, so the card never jumps.",
        content(panel(strip(
            stat("CADENCE", "retarget every 450ms"),
            stat("CARRY", "position + velocity"),
        ), card())),
    )
}

fn card() -> impl IntoElement {
    group()
        .key("card")
        .direction(Direction::Vertical)
        .width(180.0)
        .height(Size::fill())
        .radius(16.0)
        .fill(Color::hex(ACCENT))
        .shadows([Shadow::outer(
            0.0,
            10.0,
            30.0,
            Color::hex_alpha(ACCENT, 0.30),
        )])
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(6.0)
        .children((
            text("SPRING").size(11.0).weight(700).color(Color::white_alpha(0.70)),
            text("snappy").size(18.0).weight(700).color(Color::WHITE),
            text("retargeted 450ms").size(12.0).color(Color::white_alpha(0.72)),
        ))
}
