// Single responsibility: Showcase dragging a list item into a new position.

#[path = "common/mod.rs"]
mod common;
#[path = "common/interactive.rs"]
mod interactive;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use aurora::prelude::*;
use common::ExampleCli;
use interactive::{Interactive, Pointer};
use motion_ui::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 660;
/// Row height plus the list gap: the stride a drag must travel to swap places.
const STRIDE: f32 = 66.0;
/// Rows offered for reordering.
pub const ITEMS: [&str; 5] = ["Baseline", "Underline", "Strikethrough", "Highlight", "Squiggle"];

/// Index and press origin of the row currently being dragged.
struct Drag {
    item: usize,
    start_y: f32,
}

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Web 04 · Drag to reorder")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .extension(Interactive::new(reorder_driver(ITEMS.to_vec())))
        .run(move || build_ui(&ITEMS));
}

/// Follows the pointer with a compositor transform, reordering as it crosses each stride.
fn reorder_driver(items: Vec<&'static str>) -> impl FnMut(&mut Engine, &mut Pointer) -> bool {
    let mut order = items;
    let mut drag: Option<Drag> = None;

    move |engine, pointer| {
        if pointer.clicked {
            if let Some(key) = pointer.over.clone() {
                if let Some(index) = key.strip_prefix("row-").and_then(|i| i.parse::<usize>().ok()) {
                    drag = Some(Drag {
                        item: index,
                        start_y: pointer.cursor.unwrap_or_default().1,
                    });
                }
            }
        }

        if let (Some(state), Some((_, y))) = (&mut drag, pointer.cursor) {
            let delta = y - state.start_y;
            lift(engine, state.item, delta);

            let last = order.len() - 1;
            let target = ((state.item as f32 + delta / STRIDE).round() as i32).clamp(0, last as i32) as usize;
            if target != state.item {
                let moved = order.remove(state.item);
                order.insert(target, moved);
                engine.mount(build_ui(&order));
                // The row's layout slot moved too, so keep it visually under the pointer.
                let correction = (target as f32 - state.item as f32) * STRIDE;
                lift(engine, target, delta - correction);
                state.item = target;
            }
        }

        if pointer.released {
            if let Some(state) = drag.take() {
                engine
                    .animate(format!("row-{}", state.item))
                    .y(0.0)
                    .scale(1.0)
                    .spring(Spring::snappy())
                    .play();
            }
        }

        drag.is_some()
    }
}

/// Moves and slightly scales a row on the compositor, without touching layout.
fn lift(engine: &mut Engine, index: usize, delta: f32) {
    engine.update_motion(format!("row-{index}"), |motion| {
        motion.transform = Transform::from_translation(0.0, delta)
            .multiply(&Transform::from_scale(1.03, 1.03));
    });
}

/// Builds the page shown for this concept.
pub fn build_ui(order: &[&'static str]) -> impl IntoElement {
    page(
        "WEB · 04 — DRAG TO REORDER",
        "Grab a row, drop it anywhere in the list.",
        "While held, the row follows the pointer as a compositor transform, so layout never runs. Crossing a stride reorders the list once, and the FLIP transition springs every other row into place.",
        content(panel(
            strip(stat("DRAG", "compositor transform"), stat("SWAP", "FLIP on reorder")),
            list(order),
        )),
    )
}

fn list(order: &[&'static str]) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .gap(10.0)
        .layout_transition(Transition::Spring(Spring::snappy()))
        .children(order
            .iter()
            .enumerate()
            .map(|(index, label)| row(index, label))
            .collect::<Vec<_>>())
}

fn row(index: usize, label: &'static str) -> GroupDef {
    group()
        .key(format!("row-{index}"))
        .direction(Direction::Horizontal)
        .width(Size::fill())
        .height(56.0)
        .radius(12.0)
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .gap(14.0)
        .margin(Margin::x(4.0))
        .children((
            text(format!("{}", index + 1))
                .size(13.0)
                .weight(700)
                .color(Color::white_alpha(0.70))
                .margin(Margin::left(16.0)),
            text(label).size(14.0).weight(600).color(Color::WHITE),
            group().width(Size::fill()),
            text("drag me").size(12.0).color(Color::white_alpha(0.6)).margin(Margin::right(16.0)),
        ))
}
