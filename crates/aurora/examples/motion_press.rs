// Single responsibility: Showcase press-and-release feedback on a clickable button.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 440;
/// Target scale while the button is held.
const HELD: f32 = 0.9;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Web 01 · Press feedback")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// A button that squeezes on press and springs back on release.
///
/// The asymmetry is the point. A press is a short, definite event, so it eases down over
/// 90ms; a release is something the user watches for, so it springs back and overshoots.
/// Two transitions over one boolean express that, and the scale picks whichever is in
/// charge, with the idle one parked at rest.
#[component]
pub fn Page() -> impl IntoElement {
    let pressed = use_signal(false);
    let presses = use_signal(0u32);

    let down = Transition::ease(90.ms(), Ease::OutCubic).on(pressed.clone().map(|p| {
        if p {
            HELD
        } else {
            1.0
        }
    }));
    let up =
        Transition::spring(Spring::bouncy()).on(pressed.clone().map(|p| if p { 1.0 } else { 1.0 }));

    // Whichever stream owns the current state drives the view; the other sits at 1.0.
    let active = {
        let down = down.clone();
        let up = up.clone();
        pressed.map(
            move |is_pressed| {
                if is_pressed {
                    down.get()
                } else {
                    up.get()
                }
            },
        )
    };

    page(
        "WEB · 01 — PRESS FEEDBACK",
        "Down in 90ms, back on a spring.",
        "The press is a short eased squeeze; the release is a spring, so it overshoots slightly on the way home. Nothing requests frames while the button is at rest.",
        content(panel(
            strip(
                stat("GESTURE", "pointer down / up"),
                stat("RELEASE", "Spring::bouncy"),
            ),
            group()
                .direction(Direction::Vertical)
                .width(Size::fill())
                .height(Size::fill())
                .alignment(Alignment::Center)
                .distribution(Distribution::Center)
                .gap(18.0)
                .children((
                    button(pressed.clone(), presses.clone(), active),
                    text(presses.map(|n| {
                        format!("pressed {n} time{}", if n == 1 { "" } else { "s" })
                    }))
                    .size(13.0)
                    .color(Color::hex(BODY))
                    .into_element(),
                    text("press and hold the button")
                        .size(12.0)
                        .color(Color::hex(MUTED))
                        .into_element(),
                )),
        )),
    )
}

/// The button declares its own interaction, so nothing outside the tree has to know it exists.
///
/// Leaving the surface counts as a release, otherwise a pointer-down that slides off would
/// leave the button stuck in its pressed state.
fn button(pressed: Signal<bool>, presses: Signal<u32>, scale: Derived<f32>) -> GroupDef {
    let down = pressed.clone();
    let up = pressed.clone();
    let drop = pressed;
    let counted = presses;

    group()
        .key("cta")
        .height(52.0)
        .radius(Radius::max())
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .scale(scale)
        .on_pointer_down(move |_| down.set(true))
        .on_pointer_up(move |_| up.set(false))
        .on_pointer_leave(move |_| drop.set(false))
        .on_click(move |info| counted.set(counted.get() + info.click_count as u32))
        .children([text("Continue")
            .size(15.0)
            .weight(700)
            .color(Color::WHITE)
            .margin(Margin::x(28.0))])
}
