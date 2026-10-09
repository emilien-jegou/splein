// Single responsibility: Playback page: a controllable ping-pong slide with hover and click.

use aurora::prelude::*;
use aurora::reactive::Signal;

use super::motion_ui::*;
use super::{SLIDE_SECS, TRAVEL_PX};

/// Builds the page shown for this concept.
///
/// This is the one shape a `transition.on(signal)` cannot express. A playback is not heading
/// anywhere: it has no target signal, because it is its own source of truth, and it needs
/// commands — pause, resume, restart, seek — that a value stream has nowhere to put. So it
/// yields a handle alongside the value, and the handlers below drive that handle directly.
#[component]
pub fn Page() -> impl IntoElement {
    let status = use_signal(String::from("playing"));

    // A self-driving ping-pong between both ends, with a controller for the handlers.
    let (slide, control) = use_playback(
        Transition::ease(SLIDE_SECS, Ease::InOutCubic).repeat(Repeat::PingPong),
        0.0,
        TRAVEL_PX,
    );

    page(
        "MOTION · 03D — PLAYBACK",
        "Hover pauses. The pill restarts.",
        "The card loops between both ends of the arena. Resting the pointer on it holds it still and releases it when you leave; the restart pill cancels the current slide and picks the far end.",
        content(panel(
            strip(eyebrow("CONTROLLER"), controls(status.clone(), control.clone())),
            card(control, slide),
        )),
    )
}

/// Status chip and restart pill sitting at the right of the card header.
fn controls(status: Signal<String>, control: ControllerHandle) -> GroupDef {
    let reported = status.clone();
    group()
        .direction("horizontal")
        .gap(10.0)
        .alignment("center")
        .children((
            status_chip(
                text(reported)
                    .size(11.0)
                    .weight(700)
                    .color(Color::white_alpha(0.85)),
            ),
            restart_pill(control, status),
        ))
}

/// Clickable restart target. It owns its interaction; nothing polls for clicks.
fn restart_pill(control: ControllerHandle, status: Signal<String>) -> GroupDef {
    let handler = control.clone();
    let reporter = status.clone();
    group()
        .key("restart")
        .height(26.0)
        .radius("max")
        .fill(ACCENT)
        .alignment("center")
        .on_click(move |_| {
            handler.restart();
            reporter.set(String::from("restarted"));
        })
        .children([text("Restart")
            .size(11.0)
            .weight(700)
            .color(Color::WHITE)
            .margin(Margin::x(12.0))])
}

/// The sliding card, with hover wired straight to the playback handle.
fn card(control: ControllerHandle, slide: Signal<f32>) -> GroupDef {
    let on_enter = control.clone();
    let on_leave = control.clone();
    let reporter = use_signal(String::from("playing"));
    let (say_paused, say_playing) = (reporter.clone(), reporter.clone());

    group()
        .key("card")
        .direction("vertical")
        .width(140.0)
        .height("fill")
        .radius(16.0)
        .fill(ACCENT)
        .alignment("center")
        .distribution("center")
        .gap(6.0)
        .x(slide)
        .on_pointer_enter(move |_| {
            on_enter.pause();
            say_paused.set(String::from("paused · hover"));
        })
        .on_pointer_leave(move |_| {
            on_leave.resume();
            say_playing.set(String::from("playing"));
        })
        .children((
            status_chip(text(reporter).size(10.0).weight(700).color(Color::WHITE)),
            text("HOVER ME")
                .size(11.0)
                .weight(700)
                .color(Color::white_alpha(0.70)),
            text("pause on hover")
                .size(15.0)
                .weight(700)
                .color(Color::WHITE),
            text("click Restart to replay")
                .size(12.0)
                .color(Color::white_alpha(0.72)),
        ))
}
