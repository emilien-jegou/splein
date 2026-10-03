// Single responsibility: Showcase imperative multi-property tweens and spring-driven targets.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::time::Instant;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 820;
pub const HEIGHT: u32 = 460;
const STEP_SECS: f32 = 1.7;
const TRAVEL: f32 = 180.0;

/// Retargets two views between two poses on a fixed cadence.
struct Choreographer {
    started: Instant,
    outward: bool,
}

impl AppExtension for Choreographer {
    fn on_update(&mut self, engine: &mut Engine) -> bool {
        if self.started.elapsed().as_secs_f32() < STEP_SECS {
            return true;
        }
        self.started = Instant::now();
        self.outward = !self.outward;

        let (x, opacity, scale) = if self.outward {
            (0.0, 1.0, 1.0)
        } else {
            (TRAVEL, 0.4, 0.9)
        };
        engine
            .animate("panel")
            .x(x)
            .opacity(opacity)
            .scale(scale)
            .duration(650.ms())
            .ease(Ease::OutQuint)
            .play();

        engine
            .animate("badge")
            .scale(if self.outward { 1.25 } else { 1.0 })
            .spring(Spring::bouncy())
            .play();
        true
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 03 · Imperative targets")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .extension(Choreographer {
            started: Instant::now(),
            outward: false,
        })
        .run(build_ui);
}

/// Builds the page shown for this concept.
pub fn build_ui() -> impl IntoElement {
    page(
        "MOTION · 03 — IMPERATIVE",
        "One controller, several properties.",
        "A single tween drives x, opacity and scale together under one duration and curve. The badge settles on a spring instead, so it carries no fixed duration at all.",
        content(
            panel(
                strip(
                    stat("TARGET", "x · opacity · scale"),
                    stat("EASE", "650ms · OutQuint"),
                ),
                body(),
            ),
        ),
    )
}

fn body() -> GroupDef {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .children((panel_card(), badge()))
}

fn panel_card() -> impl IntoElement {
    group()
        .key("panel")
        .direction(Direction::Vertical)
        .width(280.0)
        .height(Size::fill())
        .radius(14.0)
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
            text("MULTI-PROPERTY").size(11.0).weight(700).color(Color::white_alpha(0.70)),
            text("panel").size(17.0).weight(700).color(Color::WHITE),
            text("x · opacity · scale").size(12.0).color(Color::white_alpha(0.72)),
        ))
}

fn badge() -> impl IntoElement {
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
            text("SPRING").size(10.0).weight(700).color(Color::white_alpha(0.75)),
            text("bouncy").size(13.0).weight(700).color(Color::WHITE),
        ))
}
