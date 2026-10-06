// Single responsibility: A held row's drop target and offset are measured from where the
// gesture began, not from the row's current slot.
//
// The held row is drawn at its slot plus a transform offset. A reorder moves that slot, so both
// the drop target and the offset have to discount it: the pointer's displacement and the slot's
// movement are one movement, and adding them counts it twice.
//
// A third case, `a_held_row_does_not_flip_while_dragged`, pins the engine half of the fix: a
// node with a bound `x`/`y` translation must not also spring its box toward the new slot, because
// that commands the same axis twice.

#[allow(dead_code)]
#[path = "../examples/motion_reorder.rs"]
mod reorder;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::foundation::Color;
use aurora::reactive::{Cx, ReactiveRuntime};
use aurora_tiny_skia::testing::Harness;

const LABELS: [&str; 5] = [
    "Baseline",
    "Underline",
    "Strikethrough",
    "Highlight",
    "Squiggle",
];

fn mount() -> Harness {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(reorder::Page),
        reorder::WIDTH,
        reorder::HEIGHT,
    )
    .with_background(Color::hex(0xF4F5F7));
    assert!(h.settle(900), "the list must settle before it is aimed at");
    h
}

/// Index of a row by its position in the settled layout.
fn index_of(h: &mut Harness, label: &str) -> usize {
    let mut rows: Vec<(f32, &str)> = LABELS
        .iter()
        .map(|l| (h.located(&format!("row-{l}")).absolute.y, *l))
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    rows.iter()
        .position(|(_, l)| *l == label)
        .expect("row exists")
}

#[test]
#[ntest::timeout(30_000)]
fn dragging_up_keeps_the_row_under_the_pointer() {
    let mut h = mount();
    let start = h.located("row-Strikethrough").center();
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();

    // Two strides upward, in sub-stride increments so the gesture is genuinely held.
    let steps = 12;
    let total = -2.0f64 * pitch(&h) as f64;
    bridge.dispatch(
        &winit::event::WindowEvent::CursorMoved {
            position: winit::dpi::PhysicalPosition::new(start.x as f64, start.y as f64),
            device_id: unsafe { winit::event::DeviceId::dummy() },
        },
        h.engine(),
    );
    bridge.dispatch(&press(), h.engine());

    for i in 1..=steps {
        let y = start.y as f64 + total * i as f64 / steps as f64;
        bridge.dispatch(
            &winit::event::WindowEvent::CursorMoved {
                position: winit::dpi::PhysicalPosition::new(start.x as f64, y),
                device_id: unsafe { winit::event::DeviceId::dummy() },
            },
            h.engine(),
        );
        h.step();
        let row_y = h.located("row-Strikethrough").absolute.y;
        let row_mid = row_y + h.located("row-Strikethrough").absolute.height * 0.5;
        assert!(
            (row_mid - y as f32).abs() <= 1.0,
            "step {i}: pointer at {y:.1} but the held row's middle is at {row_mid:.1}"
        );
    }

    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900));
    assert_eq!(
        index_of(&mut h, "Strikethrough"),
        0,
        "two strides up from index 2 must land at index 0"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn dragging_down_keeps_the_row_under_the_pointer() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();

    let steps = 12;
    let total = 2.0f64 * pitch(&h) as f64;
    bridge.dispatch(
        &winit::event::WindowEvent::CursorMoved {
            position: winit::dpi::PhysicalPosition::new(start.x as f64, start.y as f64),
            device_id: unsafe { winit::event::DeviceId::dummy() },
        },
        h.engine(),
    );
    bridge.dispatch(&press(), h.engine());

    for i in 1..=steps {
        let y = start.y as f64 + total * i as f64 / steps as f64;
        bridge.dispatch(
            &winit::event::WindowEvent::CursorMoved {
                position: winit::dpi::PhysicalPosition::new(start.x as f64, y),
                device_id: unsafe { winit::event::DeviceId::dummy() },
            },
            h.engine(),
        );
        h.step();
        let row_y = h.located("row-Underline").absolute.y;
        let row_mid = row_y + h.located("row-Strikethrough").absolute.height * 0.5;
        assert!(
            (row_mid - y as f32).abs() <= 1.0,
            "step {i}: pointer at {y:.1} but the held row's middle is at {row_mid:.1}"
        );
    }

    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900));
    assert_eq!(
        index_of(&mut h, "Underline"),
        3,
        "two strides down from index 1 must land at 3"
    );
}

fn press() -> winit::event::WindowEvent {
    winit::event::WindowEvent::MouseInput {
        state: winit::event::ElementState::Pressed,
        button: winit::event::MouseButton::Left,
        device_id: unsafe { winit::event::DeviceId::dummy() },
    }
}

fn release() -> winit::event::WindowEvent {
    winit::event::WindowEvent::MouseInput {
        state: winit::event::ElementState::Released,
        button: winit::event::MouseButton::Left,
        device_id: unsafe { winit::event::DeviceId::dummy() },
    }
}

/// The residual, stated as its own test rather than hidden in a tolerance.
///
/// A dragged row is positioned by the pointer, so its box must not also spring to the slot the
/// reorder moved it into. It does, which is why a held row visibly lags and then catches up
/// instead of staying under the cursor.
#[test]
#[ntest::timeout(30_000)]
fn a_held_row_does_not_flip_while_dragged() {
    let mut h = mount();
    let start = h.located("row-Strikethrough").center();
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());

    let mut worst: f32 = 0.0;
    for i in 1..=12 {
        bridge.dispatch(&moved(start.x, start.y - 10.0 * i as f32), h.engine());
        h.step();
        let loc = h.located("row-Strikethrough").absolute;
        let row_mid = loc.y + loc.height * 0.5;
        worst = worst.max((row_mid - (start.y - 10.0 * i as f32)).abs());
    }
    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900));

    assert!(
        worst <= 1.0,
        "the held row drifted up to {worst:.1}px from the cursor; a dragged row is positioned \
         by the pointer and must not also spring toward the slot the reorder moved it into"
    );
}

fn moved(x: f32, y: f32) -> winit::event::WindowEvent {
    winit::event::WindowEvent::CursorMoved {
        position: winit::dpi::PhysicalPosition::new(x as f64, y as f64),
        device_id: unsafe { winit::event::DeviceId::dummy() },
    }
}

/// The reported sequence: move a row, let go, then grab that same row again.
///
/// A reorder rebuilds every row, so the second gesture runs handlers built from a *different*
/// slot than the first. Anything the first drag left behind -- a stale index baked into a
/// closure, an origin signal that outlived the gesture -- shows up here and nowhere else.
#[test]
#[ntest::timeout(30_000)]
fn regrabbing_a_row_after_it_moved_does_not_use_its_old_slot() {
    let mut h = mount();
    let labels: Vec<String> = LABELS.iter().map(|s| s.to_string()).collect();

    // First gesture: take Underline from slot 1 down to slot 4.
    let at = h.located("row-Underline").center();
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
    bridge.dispatch(&moved(at.x, at.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    for i in 1..=12 {
        bridge.dispatch(&moved(at.x, at.y + 15.0 * i as f32), h.engine());
        h.step();
    }
    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900));
    let landed = index_of(&mut h, "Underline");
    println!("after first drag, Underline is at index {landed}");
    assert_eq!(landed, 4, "180px is three strides, so slot 1 -> 4");
    let order_after_first: Vec<String> = {
        let mut rows: Vec<(f32, String)> = labels
            .iter()
            .map(|l| (h.located(&format!("row-{l}")).absolute.y, l.clone()))
            .collect();
        rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        rows.into_iter().map(|(_, l)| l).collect()
    };
    println!("order after first drag: {order_after_first:?}");

    // Second gesture: grab the same row again, now at slot 4, and move it back up one stride.
    let at2 = h.located("row-Underline").center();
    bridge.dispatch(&moved(at2.x, at2.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    for i in 1..=6 {
        bridge.dispatch(&moved(at2.x, at2.y - 10.0 * i as f32), h.engine());
        h.step();
        let loc2 = h.located("row-Underline").absolute;
        let mid = loc2.y + loc2.height * 0.5;
        assert!(
            (mid - (at2.y - 10.0 * i as f32)).abs() <= 1.0,
            "second gesture step {i}: pointer at {:.1} but the row is at {mid:.1}",
            at2.y - 10.0 * i as f32
        );
    }
    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900));

    assert_eq!(
        index_of(&mut h, "Underline"),
        3,
        "the second drag covered one stride up from slot 4, so it belongs at 3"
    );
    let _ = order_after_first;
}

/// Distance between the tops of two adjacent rows, measured from the laid-out tree.
///
/// The demo declares no row height any more -- the rows fill the list and divide whatever space
/// it was given -- so a drag distance has to be measured rather than imported. Importing a
/// constant is what made this file disagree with the demo in the first place.
fn pitch(h: &aurora_tiny_skia::testing::Harness) -> f32 {
    let a = h.located("row-Baseline").flow;
    let b = h.located("row-Underline").flow;
    b.y - a.y
}

/// A drag after the window changes shape still tracks the pointer.
///
/// This is the case that a declared stride cannot survive. Rows used to carry a fixed height, so
/// the pitch was a constant; squash the window and the layout gives the rows a different height,
/// the constant no longer matches reality, and every offset in the gesture -- the swap threshold
/// and the carry that keeps the row under the cursor -- is computed from a number that is now
/// wrong. Nothing crashes and nothing logs; the row just drifts away from the pointer.
///
/// The rows fill the list and the pitch is read back from the measured box, so the arithmetic
/// follows the window.
#[test]
#[ntest::timeout(30_000)]
fn a_drag_tracks_the_pointer_after_the_window_is_resized() {
    for height in [900u32, 560, 760] {
        let mut h = mount();
        h.engine().resize(reorder::WIDTH, height);
        h.step();
        h.step();
        h.step();

        let start = h.located("row-Strikethrough").center();
        let stride = pitch(&h);
        let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
        bridge.dispatch(&moved(start.x, start.y), h.engine());
        bridge.dispatch(&press(), h.engine());

        let mut worst: f32 = 0.0;
        // Two measured strides, delivered in sub-stride increments so the row is held throughout.
        let steps = 14;
        for i in 1..=steps {
            let y = start.y - 2.0 * stride * i as f32 / steps as f32;
            bridge.dispatch(&moved(start.x, y), h.engine());
            h.step();
            let loc = h.located("row-Strikethrough").absolute;
            worst = worst.max(((loc.y + loc.height * 0.5) - y).abs());
        }

        assert!(
            worst <= 1.0,
            "at {height}px the held row drifted {worst:.1}px from the cursor; the drag arithmetic \
             must use the measured pitch, not a constant that no longer matches the layout"
        );
    }
}
