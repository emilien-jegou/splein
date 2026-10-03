// Single responsibility: Showcase an action button that grows and pushes the field aside.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 780;
pub const HEIGHT: u32 = 460;
/// Collapsed and expanded widths of the action button.
const COLLAPSED: f32 = 52.0;
const EXPANDED: f32 = 196.0;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Web 03 · Growing action")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// A button that widens on click and pushes the field beside it rather than covering it.
///
/// The width is a plain bound layout value and the row declares the motion, so expanding
/// springs the button's render box toward its new flow box instead of re-laying out every
/// frame. The click lives on the button; nothing polls for it.
#[component]
pub fn Page() -> impl IntoElement {
    let expanded = use_signal(false);

    // The width is animated *here*, not by the engine, and the label reads the same animated
    // signal. That is the whole point: the box and its text are two views of one value, so they
    // cannot disagree. The previous version bound both to the raw `expanded` flag and let the
    // engine spring the box alone, so the label swapped to "Validate" on frame one while the box
    // was still 52px wide -- a long word laid out in a box that had not grown yet.
    let width = Transition::spring(Spring::snappy()).on(expanded.clone().map(|is_open| {
        if is_open {
            EXPANDED
        } else {
            COLLAPSED
        }
    }));

    page(
        "WEB · 03 — GROWING ACTION",
        "The button pushes the field instead of covering it.",
        "The width is a spring in the component, and the label reads that same animated signal. One value drives both the box and its text, so the word always fits the box it is drawn in.",
        content(panel(
            strip(stat("ROW", "fit width"), stat("MOTION", "spring width signal")),
            body(width, expanded),
        )),
    )
}

fn body(width: Signal<f32>, expanded: Signal<bool>) -> GroupDef {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .distribution(Distribution::Center)
        .gap(20.0)
        .children((
            row(width, expanded),
            text("press the action to expand it")
                .size(12.0)
                .color(Color::hex(MUTED))
                .into_element(),
        ))
}

/// Fit-width row, so growth pushes content rather than squeezing it.
///
/// No motion declared here. The width arriving is already an animated signal, so the row simply
/// lays out at whatever value it currently holds, and every frame of the growth is a real layout
/// rather than an animated box hiding a static one.
fn row(width: Signal<f32>, expanded: Signal<bool>) -> GroupDef {
    group()
        .direction(Direction::Horizontal)
        .width(Size::fit())
        .height(52.0)
        .gap(12.0)
        .alignment(Alignment::Center)
        .children((action(width, expanded), field()))
}

/// The action button, which owns its own toggle.
fn action(width: Signal<f32>, expanded: Signal<bool>) -> GroupDef {
    let toggle = expanded.clone();
    let state = expanded;
    // The label reads the animated width, not the target. Crossing the midpoint therefore
    // happens partway through the spring, exactly when the box is actually wide enough for the
    // longer word.
    let label = width.clone();

    group()
        .key("action")
        .width(width)
        .height(Size::fill())
        .radius(Radius::max())
        .clip(true)
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .on_click(move |_| toggle.set(!state.get()))
        .children([text(label.map(|w| {
            if w > (COLLAPSED + EXPANDED) * 0.5 {
                String::from("Validate")
            } else {
                String::from("Go")
            }
        }))
        .size(14.0)
        .weight(700)
        .color(Color::WHITE)])
}

fn field() -> GroupDef {
    group()
        .direction(Direction::Horizontal)
        .width(320.0)
        .height(Size::fill())
        .radius(Radius::max())
        .fill(Color::hex(0xF7F8FA))
        .stroke(Stroke::inside(1.0, hairline()))
        .alignment(Alignment::Center)
        .children([text("you@example.com")
            .size(14.0)
            .color(Color::hex(MUTED))
            .margin(Margin::x(16.0))])
}
