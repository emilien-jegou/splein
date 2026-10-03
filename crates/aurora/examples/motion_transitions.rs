// Single responsibility: Showcase declarative transitions interpolated onto bound properties.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 820;
pub const HEIGHT: u32 = 460;
const TOGGLE_SECS: f32 = 1.4;

/// Flips a bound signal so the declared transitions keep replaying.
struct Toggler {
    enabled: Signal<bool>,
    started: Instant,
}

impl AppExtension for Toggler {
    fn on_update(&mut self, _engine: &mut Engine) -> bool {
        if self.started.elapsed().as_secs_f32() >= TOGGLE_SECS {
            self.started = Instant::now();
            let next = !self.enabled.get();
            self.enabled.set(next);
        }
        true
    }
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let enabled = Signal::new(Rc::clone(&runtime), false);

    cli.app_config("Aurora — Motion 02 · Declarative transitions")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .runtime(runtime)
        .extension(Toggler {
            enabled: enabled.clone(),
            started: Instant::now(),
        })
        .run(move || build_ui(enabled.clone()));
}

/// Builds the page shown for this concept.
pub fn build_ui(enabled: Signal<bool>) -> impl IntoElement {
    page(
        "MOTION · 02 — DECLARATIVE",
        "The value jumps. The transition interpolates it.",
        "Both tiles read one bound signal. No imperative call and no key: the declared tween or spring sits on the property itself and owns the interpolation.",
        content(
            group()
                .direction(Direction::Horizontal)
                .width(Size::fill())
                .height(Size::fill())
                .gap(20.0)
                .children((tween_panel(enabled.clone()), spring_panel(enabled))),
        ),
    )
}

fn tween_panel(enabled: Signal<bool>) -> GroupDef {
    panel(
        strip(
            eyebrow("TWEEN"),
            text(".animate(200ms, OutCubic)").size(12.0).color(Color::hex(BODY)),
        ),
        tile("fade + slide", ACCENT)
            .opacity(enabled.map(|on| if on { 1.0 } else { 0.35 }))
            .transform(enabled.map(|on| {
                Transform::from_translation(if on { 0.0 } else { 48.0 }, 0.0)
            }))
            .animate(Tween::new(200.ms(), Ease::OutCubic)),
    )
}

fn spring_panel(enabled: Signal<bool>) -> GroupDef {
    panel(
        strip(
            eyebrow("SPRING"),
            text(".spring(Spring::snappy())").size(12.0).color(Color::hex(BODY)),
        ),
        tile("settle", 0x10B981)
            .transform(enabled.map(|on| {
                let s = if on { 1.18 } else { 1.0 };
                Transform::from_scale(s, s)
            }))
            .spring(Spring::snappy()),
    )
}

/// Accent tile used by both panels, sized so scaling never reaches the arena edge.
fn tile(label: &'static str, accent: u32) -> GroupDef {
    group()
        .direction(Direction::Vertical)
        .width(Size::percent(0.62))
        .height(Size::fill())
        .radius(14.0)
        .fill(Color::hex(accent))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(6.0)
        .children((
            text(label).size(16.0).weight(700).color(Color::WHITE),
            text("bound property").size(11.0).weight(700).color(Color::white_alpha(0.72)),
        ))
}
