// Single responsibility: Showcase one spring signal driving several properties as pure functions.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 520;
/// Seconds between reversals of the driver signal.
const SWING_SECS: f32 = 1.5;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 05 · Signal-driven motion")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(PAGE)
        .run(Page);
}

/// One spring, published as one signal, with everything else derived from it.
///
/// The simulation is not driven here: the transition owns it, the ticker advances it, and
/// this component only states the four things that read it. Each row is a pure function of
/// the same animated value, which is why they cannot drift apart.
#[component]
pub fn Page() -> impl IntoElement {
    let on = use_interval_toggle(SWING_SECS);
    let target = on.map(|is_on| if is_on { 1.0 } else { 0.0 });

    // The one simulation. Everything below is `p.map(..)`.
    let p = Transition::from_str("spring").on(target);

    page(
        "MOTION · 05 — REACTIVE",
        "One spring, published as one signal.",
        "A single spring simulation is published to a signal on every frame. Travel, path, scale and colour below are pure functions of it, so they stay in lockstep with no imperative call.",
        content(
            surface()
                .direction("vertical")
                .width("fill")
                .height("fill")
                .children([group()
                    .direction("vertical")
                    .width("fill")
                    .height("fill")
                    .gap(16.0)
                    .margin(24.0)
                    .children((
                        row("TRAVEL", travel(p.clone())),
                        row("PATH", path(p.clone())),
                        row("SCALE", scale(p.clone())),
                        row("COLOUR", swatch(p)),
                    ))]),
        ),
    )
}

/// Label and animated element sharing one fixed-height row.
fn row(label: &'static str, element: impl IntoElement) -> GroupDef {
    group()
        .direction("horizontal")
        .width("fill")
        .height(40.0)
        .alignment("center")
        .gap(16.0)
        .children((
            group()
                .width(96.0)
                .height(40.0)
                .alignment("center")
                .children([eyebrow(label)]),
            group()
                .width("fill")
                .height("fill")
                .alignment("center")
                .children([element]),
        ))
}

/// Accent puck sliding along a bounded track.
fn travel(p: Signal<f32>) -> impl IntoElement {
    group()
        .width("fill")
        .height("fill")
        .radius("max")
        .clip(true)
        .fill(Color::hex_alpha(HAIRLINE, 0.07))
        .children([group()
            .width(120.0)
            .height("fill")
            .radius("max")
            .fill(ACCENT)
            .x(p.map(|v| v * 380.0))])
}

/// Marker travelling a sine path across the row.
fn path(p: Signal<f32>) -> impl IntoElement {
    group()
        .width("fill")
        .height("fill")
        .children([group()
            .width(20.0)
            .height(20.0)
            .radius("max")
            .fill(0xEC4899)
            .transform(p.map(|v| Transform::from_translation(v * 460.0, (v * 6.28).sin() * 9.0)))])
}

/// Pill scaling with the same driver.
fn scale(p: Signal<f32>) -> impl IntoElement {
    group()
        .width(140.0)
        .height("fill")
        .radius("max")
        .fill(0x10B981)
        .alignment("center")
        .distribution("center")
        .children([text("scale").size(12.0).weight(700).color(Color::WHITE)])
        .scale(p.map(|v| 1.0 + v * 0.2))
}

/// Swatch whose fill is sampled from the same position.
fn swatch(p: Signal<f32>) -> impl IntoElement {
    group()
        .width("fill")
        .height("fill")
        .radius("max")
        .fill(p.map(|v| Color::hex_alpha(0xF59E0B, 0.25 + v * 0.75)))
}
