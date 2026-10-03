// Single responsibility: Retained damage-only canvas must equal a from-scratch repaint.
//
// The comparison itself lives in `aurora::testing::damage`, because it is the only reliable
// way to observe damage tracking and it was previously re-derivable only by reading this
// file. These tests are the two cases that motivated it.

#[allow(dead_code)]
#[path = "../examples/motion_accordion/main.rs"]
mod accordion;

#[allow(dead_code)]
#[path = "../examples/motion_reorder.rs"]
mod reorder;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::*;
use aurora::foundation::{Color, Direction};
use aurora::reactive::{Cx, ReactiveRuntime, Signal};
use aurora::testing::Harness;

fn page() -> Color {
    Color::hex(0xF4F5F7)
}

/// The accordion demo, with the signal that drives which panel is open.
///
/// Built through [`Harness::from_runtime`] rather than `Harness::page`, because the test
/// drives the signal itself and the tree has to be bound to that exact signal.
fn accordion_harness() -> (Harness, Signal<Option<usize>>) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(0usize));
    let harness = Harness::from_runtime(
        Rc::clone(&runtime),
        aurora::reactive::Cx::new(Rc::clone(&runtime)).enter(|| accordion::Page(open.clone())),
        accordion::WIDTH,
        accordion::HEIGHT,
    )
    .with_background(page());
    (harness, open)
}

/// A moving surface, positioned by bound signals rather than by an engine override.
///
/// The original drove the reorder row through `update_motion`, an engine hook that no longer
/// exists. This builds the same shape directly -- a rounded, slightly scaled surface whose offset
/// and scale come from signals -- because that is now the only way to move a view, and it goes
/// through the same path a client animation takes.
fn moving_surface_harness() -> (Harness, Signal<f32>, Signal<f32>) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let dy = Signal::new(Rc::clone(&runtime), 0.0f32);
    let scale = Signal::new(Rc::clone(&runtime), 1.0f32);
    let tree = {
        let dy = dy.clone();
        let scale = scale.clone();
        move || {
            group()
                .direction(Direction::Vertical)
                .width(600.0)
                .height(600.0)
                .fill(page())
                .children([group()
                    .key("row-0")
                    .width(200.0)
                    .height(60.0)
                    .radius(12.0)
                    .fill(Color::hex(0x4F46E5))
                    .y(dy.clone())
                    .scale(scale.clone())])
                .into_element()
        }
    };
    let harness = Harness::from_runtime(
        Rc::clone(&runtime),
        Cx::new(Rc::clone(&runtime)).enter(tree),
        600,
        600,
    )
    .with_background(page());
    (harness, dy, scale)
}

#[test]
#[ntest::timeout(30_000)]
fn dragging_a_row_repaints_every_pixel_it_vacates() {
    let (mut h, dy, scale) = moving_surface_harness();
    h.step();

    let mut worst = 0u32;
    for f in 0..45 {
        // Driven directly rather than through a pointer: this is a test of whether damage
        // *follows* a moving surface, and a gesture would confound that with input recognition.
        dy.set(f as f32 * 8.0);
        scale.set(1.0 + f as f32 * 0.001);
        h.step();
        worst = worst.max(h.damage_parity().uncovered);
    }

    // Before clip-collapsed bounds were fixed this grew to 25632 (an uncleared drag trail).
    // What remains is a constant handful of anti-aliased corner pixels.
    h.damage_parity().assert_no_strays(128, "dragging a row");
    assert!(
        worst <= 128,
        "drag left {worst} changed pixels outside the redrawn tiles"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn switching_accordion_panels_repaints_every_pixel_they_vacate() {
    let (mut h, open) = accordion_harness();

    let mut worst = 0u32;
    // Item 3's body overruns the content clip, which is the case that can strand pixels.
    open.set(Some(3));
    for _ in 0..45 {
        h.step();
        worst = worst.max(h.damage_parity().uncovered);
    }
    assert!(
        worst <= 128,
        "accordion left {worst} changed pixels outside the redrawn tiles"
    );
}
