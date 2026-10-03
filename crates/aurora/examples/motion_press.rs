// Single responsibility: Showcase press-and-release feedback on a clickable button.

#[path = "common/mod.rs"]
mod common;
#[path = "common/interactive.rs"]
mod interactive;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;
use interactive::{Interactive, Pointer};
use motion_ui::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 440;
/// How long the button stays squeezed before the release spring fires.
const SQUEEZE: Duration = Duration::from_millis(90);

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let presses = Signal::new(Rc::clone(&runtime), 0u32);

    cli.app_config("Aurora — Web 01 · Press feedback")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .runtime(runtime)
        .extension(Interactive::new(press_driver(presses.clone())))
        .run(move || build_ui(presses.clone()));
}

/// Squeezes the button on press, then releases it back on an overshooting spring.
fn press_driver(presses: Signal<u32>) -> impl FnMut(&mut Engine, &mut Pointer) -> bool {
    let mut releases_at: Option<Instant> = None;
    move |engine, pointer| {
        if pointer.over_key("cta") && pointer.clicked {
            presses.set(presses.get() + 1);
            releases_at = Some(Instant::now() + SQUEEZE);
            engine
                .animate("cta")
                .scale(0.9)
                .duration(90.ms())
                .ease(Ease::OutCubic)
                .play();
        }

        if let Some(at) = releases_at {
            if Instant::now() >= at {
                releases_at = None;
                engine
                    .animate("cta")
                    .scale(1.0)
                    .spring(Spring::bouncy())
                    .play();
            }
        }

        // Frames are only requested while the squeeze is in flight.
        releases_at.is_some()
    }
}

/// Builds the page shown for this concept.
pub fn build_ui(presses: Signal<u32>) -> impl IntoElement {
    page(
        "WEB · 01 — PRESS FEEDBACK",
        "Down in 90ms, back on a spring.",
        "The press is a short eased squeeze; the release is a spring, so it overshoots slightly on the way home. Nothing requests frames while the button is at rest.",
        content(panel(
            strip(
                stat("GESTURE", "pointer down"),
                stat("RELEASE", "Spring::bouncy"),
            ),
            body(presses),
        )),
    )
}

fn body(presses: Signal<u32>) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(18.0)
        .children((
            button(),
            text(presses.map(|n| format!("pressed {n} time{}", if n == 1 { "" } else { "s" })))
                .size(13.0)
                .color(Color::hex(BODY)),
            text("hover the button, then press")
                .size(12.0)
                .color(Color::hex(MUTED)),
        ))
}

fn button() -> impl IntoElement {
    group()
        .key("cta")
        .height(52.0)
        .radius(Radius::max())
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .children([text("Continue")
            .size(15.0)
            .weight(700)
            .color(Color::WHITE)
            .margin(Margin::x(28.0))])
}
