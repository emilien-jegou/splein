// Single responsibility: Playback page shell, status chip, restart pill, and sliding card.

use aurora::prelude::*;
use aurora::reactive::Signal;

use super::motion_ui::*;

/// Builds the page shown for this concept.
pub fn build_ui(status: Signal<String>) -> impl IntoElement {
    page(
        "MOTION · 03D — PLAYBACK",
        "Hover pauses. The pill restarts.",
        "The card loops between both ends of the arena. Resting the pointer on it holds it still and releases it when you leave; the restart pill cancels the current slide and picks the far end.",
        content(panel(strip(eyebrow("CONTROLLER"), controls(status)), card())),
    )
}

/// Status chip and restart pill sitting at the right of the card header.
fn controls(status: Signal<String>) -> GroupDef {
    group()
        .direction(Direction::Horizontal)
        .gap(10.0)
        .alignment(Alignment::Center)
        .children((
            status_chip(text(status).size(11.0).weight(700).color(Color::white_alpha(0.85))),
            restart_pill(),
        ))
}

/// Clickable restart target, addressable by hit-testing.
fn restart_pill() -> impl IntoElement {
    group()
        .key("restart")
        .height(26.0)
        .radius(Radius::max())
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .children([text("Restart")
            .size(11.0)
            .weight(700)
            .color(Color::WHITE)
            .margin(Margin::x(12.0))])
}

fn card() -> impl IntoElement {
    group()
        .key("card")
        .direction(Direction::Vertical)
        .width(140.0)
        .height(Size::fill())
        .radius(16.0)
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .gap(6.0)
        .children((
            text("HOVER ME").size(11.0).weight(700).color(Color::white_alpha(0.70)),
            text("pause on hover").size(15.0).weight(700).color(Color::WHITE),
            text("click Restart to replay").size(12.0).color(Color::white_alpha(0.72)),
        ))
}
