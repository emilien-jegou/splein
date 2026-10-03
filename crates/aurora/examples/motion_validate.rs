// Single responsibility: Showcase an action button that grows and pushes the field aside.

#[path = "common/mod.rs"]
mod common;
#[path = "common/interactive.rs"]
mod interactive;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;
use interactive::{Interactive, Pointer};
use motion_ui::*;

pub const WIDTH: u32 = 780;
pub const HEIGHT: u32 = 460;
/// Collapsed and expanded widths of the action button.
const COLLAPSED: f32 = 52.0;
const EXPANDED: f32 = 196.0;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let width = Signal::new(Rc::clone(&runtime), COLLAPSED);

    cli.app_config("Aurora — Web 03 · Growing action")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .runtime(runtime)
        .extension(Interactive::new(driver(width.clone())))
        .run(move || build_ui(width.clone()));
}

/// Toggles the action button's declared width on click.
fn driver(width: Signal<f32>) -> impl FnMut(&mut Engine, &mut Pointer) -> bool {
    move |_engine, pointer| {
        if pointer.over_key("action") && pointer.clicked {
            let next = if width.get() < 100.0 { EXPANDED } else { COLLAPSED };
            width.set(next);
        }
        false
    }
}

/// Builds the page shown for this concept.
pub fn build_ui(width: Signal<f32>) -> impl IntoElement {
    page(
        "WEB · 03 — GROWING ACTION",
        "The button pushes the field instead of covering it.",
        "The button's width is a bound layout value, and the row that holds it declares a layout transition. Expanding therefore springs the button wider while the field beside it slides right.",
        content(panel(
            strip(stat("ROW", "fit width"), stat("MOTION", "FLIP on width change")),
            body(width),
        )),
    )
}

fn body(width: Signal<f32>) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .distribution(Distribution::Center)
        .gap(20.0)
        .children((
            row(width),
            text("press the action to expand it")
                .size(12.0)
                .color(Color::hex(MUTED)),
        ))
}

/// Fit-width row, so growth pushes content rather than squeezing it.
fn row(width: Signal<f32>) -> impl IntoElement {
    group()
        .direction(Direction::Horizontal)
        .width(Size::fit())
        .height(52.0)
        .gap(12.0)
        .alignment(Alignment::Center)
        .layout_transition(Transition::Spring(Spring::snappy()))
        .children((action(width), field()))
}

fn action(width: Signal<f32>) -> impl IntoElement {
    group()
        .key("action")
        .width(width.clone())
        .height(Size::fill())
        .radius(Radius::max())
        .clip(true)
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .children([text(width.map(|w| {
            if w > 100.0 { String::from("Validate") } else { String::from("Go") }
        }))
        .size(14.0)
        .weight(700)
        .color(Color::WHITE)])
}

fn field() -> impl IntoElement {
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
