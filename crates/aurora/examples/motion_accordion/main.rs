// Single responsibility: Entrypoint and toggle driver for the accordion demo.

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/interactive.rs"]
mod interactive;
#[path = "../common/motion_ui.rs"]
mod motion_ui;

mod panels;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::prelude::*;
use aurora::reactive::{ReactiveRuntime, Signal};
use common::ExampleCli;
use interactive::{Interactive, Pointer};
use motion_ui::*;
use panels::list;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 760;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(0usize));

    cli.app_config("Aurora — Web 02 · Accordion")
        .font("Inter", include_bytes!("../../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .runtime(runtime)
        .extension(Interactive::new(driver(open.clone())))
        .run(move || build_ui(open.clone()));
}

/// Toggles the open panel when its header is pressed.
fn driver(open: Signal<Option<usize>>) -> impl FnMut(&mut Engine, &mut Pointer) -> bool {
    move |_engine, pointer| {
        if !pointer.clicked {
            return false;
        }
        for index in 0..4 {
            if pointer.over_key(&format!("item-{index}")) {
                let next = if open.get() == Some(index) {
                    None
                } else {
                    Some(index)
                };
                open.set(next);
            }
        }
        false
    }
}

/// Builds the page shown for this concept.
pub fn build_ui(open: Signal<Option<usize>>) -> impl IntoElement {
    page(
        "WEB · 02 — ACCORDION",
        "One panel open at a time.",
        "Each header owns a derived height, and the list declares a layout transition. Changing the open panel therefore springs the growing item back into shape while every sibling below slides with it.",
        content(panel(
            strip(
                stat("LAYOUT", "derived height per item"),
                stat("MOTION", "FLIP via layout_transition"),
            ),
            list(open),
        )),
    )
}
