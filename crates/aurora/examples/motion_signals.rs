// Single responsibility: Showcase one spring signal driving several properties as pure functions.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use aurora::motion::SpringState;
use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 520;
const SWING_SECS: f32 = 1.5;

/// Publishes one spring's position so properties can be derived from it.
struct SpringDriver {
    state: SpringState,
    progress: Signal<f32>,
    started: Instant,
    last: Instant,
}

impl AppExtension for SpringDriver {
    fn on_update(&mut self, _engine: &mut Engine) -> bool {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        if now.duration_since(self.started).as_secs_f32() >= SWING_SECS {
            self.started = now;
            let target = if self.state.target >= 1.0 { 0.0 } else { 1.0 };
            self.state.retarget(target);
        }
        self.state.advance(dt);
        self.progress.set_if_changed(self.state.position);
        true
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let progress = Signal::new(Rc::clone(&runtime), 0.0f32);

    cli.app_config("Aurora — Motion 05 · Signal-driven motion")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .runtime(runtime)
        .extension(SpringDriver {
            state: SpringState::new(Spring::snappy(), 0.0, 1.0),
            progress: progress.clone(),
            started: Instant::now(),
            last: Instant::now(),
        })
        .run(move || build_ui(progress.clone()));
}

/// Builds the page shown for this concept.
pub fn build_ui(progress: Signal<f32>) -> impl IntoElement {
    page(
        "MOTION · 05 — REACTIVE",
        "One spring, published as one signal.",
        "A single spring simulation is published to a signal on every frame. Travel, path, scale and colour below are pure functions of it, so they stay in lockstep with no imperative call.",
        content(surface().direction(Direction::Vertical).width(Size::fill()).height(Size::fill())
            .children([group().direction(Direction::Vertical)
                .width(Size::fill()).height(Size::fill())
                .gap(16.0).margin(Margin::all(24.0))
                .children((
                    row("TRAVEL", travel(progress.clone())),
                    row("PATH", path(progress.clone())),
                    row("SCALE", scale(progress.clone())),
                    row("COLOUR", swatch(progress.clone())),
                ))])),
    )
}

/// Label and animated element sharing one fixed-height row.
fn row(label: &'static str, element: impl IntoElement) -> GroupDef {
    group().direction(Direction::Horizontal).width(Size::fill()).height(40.0)
        .alignment(Alignment::Center).gap(16.0)
        .children((
            group().width(96.0).height(40.0).alignment(Alignment::Center).children([eyebrow(label)]),
            group().width(Size::fill()).height(Size::fill())
                .alignment(Alignment::Center).children([element]),
        ))
}

/// Accent puck sliding along a bounded track.
fn travel(progress: Signal<f32>) -> impl IntoElement {
    group().width(Size::fill()).height(Size::fill()).radius(Radius::max())
        .clip(true).fill(Color::hex_alpha(HAIRLINE, 0.07))
        .children([group().width(120.0).height(Size::fill()).radius(Radius::max())
            .fill(Color::hex(ACCENT))
            .transform(progress.map(|p| Transform::from_translation(p * 380.0, 0.0)))])
}

/// Marker travelling a sine path across the row.
fn path(progress: Signal<f32>) -> impl IntoElement {
    group().width(Size::fill()).height(Size::fill())
        .children([group().width(20.0).height(20.0).radius(Radius::max())
            .fill(Color::hex(0xEC4899))
            .transform(progress.map(|p| {
                Transform::from_translation(p * 460.0, (p * 6.28).sin() * 9.0)
            }))])
}

/// Pill scaling with the same driver.
fn scale(progress: Signal<f32>) -> impl IntoElement {
    group().width(140.0).height(Size::fill()).radius(Radius::max()).fill(Color::hex(0x10B981))
        .alignment(Alignment::Center).distribution(Distribution::Center)
        .children([text("scale").size(12.0).weight(700).color(Color::WHITE)])
        .transform(progress.map(|p| {
            let s = 1.0 + p * 0.2;
            Transform::from_scale(s, s)
        }))
}

/// Swatch whose fill is sampled from the same position.
fn swatch(progress: Signal<f32>) -> impl IntoElement {
    group().width(Size::fill()).height(Size::fill()).radius(Radius::max())
        .fill(progress.map(|p| Color::hex_alpha(0xF59E0B, 0.25 + p * 0.75)))
}
