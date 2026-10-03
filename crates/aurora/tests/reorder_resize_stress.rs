// Single responsibility: Drag arithmetic that survives the window changing shape mid-gesture.

#[path = "../examples/motion_reorder.rs"]
mod reorder;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::prelude::*;
use aurora::reactive::{Cx, ReactiveRuntime};
use aurora::testing::Harness;

fn moved(x: f32, y: f32) -> winit::event::WindowEvent {
    winit::event::WindowEvent::CursorMoved {
        position: winit::dpi::PhysicalPosition::new(x as f64, y as f64),
        device_id: unsafe { winit::event::DeviceId::dummy() },
    }
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

/// Distance between the tops of two adjacent rows, measured from the laid-out tree.
fn pitch(h: &Harness, a: &str, b: &str) -> f32 {
    h.located(&format!("row-{a}")).flow.y - h.located(&format!("row-{b}")).flow.y
}

/// A drag that starts, is interrupted by a resize, and continues.
///
/// The resize is the whole test. A drag whose arithmetic was computed from a pitch captured at
/// the grab has no answer for a window that changed shape halfway through: the row's own box
/// moved, the pointer's target did not, and every offset downstream is measured from a stride that
/// no longer exists. Nothing crashes and nothing logs — the row just drifts away from the cursor.
#[test]
#[ntest::timeout(30_000)]
fn a_resize_in_the_middle_of_a_drag_does_not_detach_the_row_from_the_pointer() {
    let mut h = {
        let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let tree = Cx::new(Rc::clone(&rt)).enter(reorder::Page);
        let mut h = Harness::from_runtime(Rc::clone(&rt), tree, reorder::WIDTH, 760)
            .with_background(Color::hex(0xF4F5F7));
        assert!(h.settle(900), "the list must settle before it is aimed at");
        h
    };

    let start = h.located("row-Strikethrough").center();
    let stride = pitch(&h, "Squiggle", "Strikethrough");

    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());

    let mut worst: f32 = 0.0;
    // Four heights, each held for a few pointer samples, so the gesture spans three resizes.
    let mut y = start.y;
    for height in [760u32, 420, 900, 300] {
        h.engine().resize(reorder::WIDTH, height);
        h.step();
        for _ in 0..4 {
            y -= stride / 5.0;
            bridge.dispatch(&moved(start.x, y), h.engine());
            h.step();
            let loc = h.located("row-Strikethrough").absolute;
            worst = worst.max(((loc.y + loc.height * 0.5) - y).abs());
        }
    }
    bridge.dispatch(&release(), h.engine());

    assert!(
        worst <= 1.0,
        "the held row drifted {worst:.1}px from the cursor across three resizes. The drag offset \
         has to be derived from the pointer's intent and the row's *current* box, not accumulated \
         from a pitch measured when the gesture began"
    );
    assert!(h.settle(900), "the released row must come to rest");
    assert!(
        h.transform("row-Strikethrough").ty.abs() < 2.0,
        "the released row must spring back onto its slot, offset {}",
        h.transform("row-Strikethrough").ty
    );
}

/// A resize that leaves the rows too short for the list leaves them *unequal*.
///
/// This is the case a single derived pitch cannot survive. Fill rows share whatever space is
/// left, but each is then clamped at its own min-content floor — so past some height there is no
/// single stride left to divide a distance by, and arithmetic built on one is simply wrong.
#[test]
#[ntest::timeout(30_000)]
fn rows_stay_usable_when_a_squashed_window_makes_them_unequal() {
    let mut h = {
        let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let tree = Cx::new(Rc::clone(&rt)).enter(reorder::Page);
        let mut h = Harness::from_runtime(Rc::clone(&rt), tree, reorder::WIDTH, 760);
        assert!(h.settle(900));
        h
    };

    for height in [760u32, 300, 200] {
        h.engine().resize(reorder::WIDTH, height);
        h.step();
        h.step();

        // Every row must still be reachable: non-empty, inside the window, and in list order.
        let mut rows: Vec<(f32, f32, &str)> = [
            "Baseline",
            "Underline",
            "Strikethrough",
            "Highlight",
            "Squiggle",
        ]
        .iter()
        .map(|l| {
            let r = h.located(&format!("row-{l}")).absolute;
            (r.y, r.height, *l)
        })
        .collect();
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));

        for (y, hgt, label) in &rows {
            assert!(
                *hgt > 0.0 && *y < height as f32,
                "at {height}px the row {label} is {hgt}px tall at y={y}: a squashed list must \
                 still produce rows a pointer can grab"
            );
        }
    }
}

/// Every row is still a distinct target after a squashed resize, which is what lets the gesture
/// pick the slot the pointer is over without a pitch.
#[test]
#[ntest::timeout(30_000)]
fn a_squashed_list_still_reorders_by_measured_slot() {
    let mut h = {
        let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let tree = Cx::new(Rc::clone(&rt)).enter(reorder::Page);
        let mut h = Harness::from_runtime(Rc::clone(&rt), tree, reorder::WIDTH, 760);
        assert!(h.settle(900));
        h
    };

    // Short enough that the rows cannot all be the same size.
    h.engine().resize(reorder::WIDTH, 220);
    h.step();
    h.step();

    let start = h.located("row-Baseline").center();
    let span = h.located("row-Squiggle").absolute.y - start.y;
    assert!(
        span > 0.0,
        "the list must still have a height to drag across"
    );

    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    for i in 1..=10 {
        bridge.dispatch(
            &moved(start.x, start.y + span * i as f32 / 10.0),
            h.engine(),
        );
        h.step();
    }
    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900));

    let ys: Vec<(f32, &str)> = [
        "Baseline",
        "Underline",
        "Strikethrough",
        "Highlight",
        "Squiggle",
    ]
    .iter()
    .map(|l| (h.located(&format!("row-{l}")).absolute.y, *l))
    .collect();
    let bottom = ys
        .iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, l)| *l)
        .expect("rows exist");
    assert_eq!(
        bottom, "Baseline",
        "a drag from the top row to the bottom of a squashed list must land it at the bottom"
    );
}

/// Reordering repeatedly must not leak per-rebuild animation work.
///
/// A transition is never retired — its target may move again at any time — so anything registered
/// per rebuild grows without bound. Invisible until a long session starts dropping frames.
#[test]
#[ntest::timeout(30_000)]
fn repeated_reorders_do_not_accumulate_animation_work() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let tree = Cx::new(Rc::clone(&rt)).enter(reorder::Page);
    let mut h = Harness::from_runtime(Rc::clone(&rt), tree, reorder::WIDTH, 760);
    assert!(h.settle(900));

    let before = rt.borrow().post_layout_count();
    for i in 0..24 {
        let label = if i % 2 == 0 { "Baseline" } else { "Squiggle" };
        let at = h.located(&format!("row-{label}")).center();
        let to = h.located("row-Squiggle").center();
        let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
        bridge.dispatch(&moved(at.x, at.y), h.engine());
        bridge.dispatch(&press(), h.engine());
        for step in 1..=6 {
            let f = step as f32 / 6.0;
            bridge.dispatch(&moved(at.x, at.y + (to.y - at.y) * f), h.engine());
            h.step();
        }
        bridge.dispatch(&release(), h.engine());
        h.settle(900);
    }

    assert_eq!(
        rt.borrow().post_layout_count(),
        before,
        "twenty-four drags must not add post-layout work; the count went {before} -> {}",
        rt.borrow().post_layout_count()
    );
}

/// The rows a dragged row passes are the ones that move; the ones it does not pass keep their slot.
///
/// The assertion is on *which* rows moved, not just that something did: asserting on the last row
/// would pass whenever the double-counted target happened to shuffle the whole list.
#[test]
#[ntest::timeout(30_000)]
fn a_reorder_moves_exactly_the_rows_the_drag_passed() {
    let mut h = {
        let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let tree = Cx::new(Rc::clone(&rt)).enter(reorder::Page);
        let mut h = Harness::from_runtime(Rc::clone(&rt), tree, reorder::WIDTH, 760);
        assert!(h.settle(900));
        h
    };

    let labels = [
        "Baseline",
        "Underline",
        "Strikethrough",
        "Highlight",
        "Squiggle",
    ];
    let slot = |h: &Harness, label: &str| -> usize {
        let mut rows: Vec<(f32, &str)> = labels
            .iter()
            .map(|l| (h.located(&format!("row-{l}")).absolute.y, *l))
            .collect();
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        rows.iter()
            .position(|(_, l)| *l == label)
            .expect("row exists")
    };

    let at = h.located("row-Baseline").center();
    let stride = pitch(&h, "Underline", "Baseline");
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
    bridge.dispatch(&moved(at.x, at.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    for step in 1..=8 {
        bridge.dispatch(
            &moved(at.x, at.y + 2.0 * stride * step as f32 / 8.0),
            h.engine(),
        );
        h.step();
    }
    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900));

    assert_eq!(
        slot(&h, "Baseline"),
        2,
        "the dragged row must land two slots down"
    );
    assert_eq!(slot(&h, "Underline"), 0, "the row it passed must move up");
    assert_eq!(slot(&h, "Strikethrough"), 1, "and so must the next one");
    assert_eq!(
        slot(&h, "Squiggle"),
        4,
        "a row the drag never reached must keep its slot"
    );
}

/// The demo no longer declares a row height or derives a pitch, so the invariant that replaced
/// both is that the rows divide the space they were given and agree with each other.
#[test]
#[ntest::timeout(30_000)]
fn the_rows_divide_the_space_the_list_was_given() {
    let mut h = {
        let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let tree = Cx::new(Rc::clone(&rt)).enter(reorder::Page);
        let mut h = Harness::from_runtime(Rc::clone(&rt), tree, reorder::WIDTH, 760);
        assert!(h.settle(900));
        h
    };

    for height in [900u32, 760, 560, 420] {
        h.engine().resize(reorder::WIDTH, height);
        h.step();
        h.step();

        let list = h.located("list").absolute;
        let rows: Vec<_> = [
            "Baseline",
            "Underline",
            "Strikethrough",
            "Highlight",
            "Squiggle",
        ]
        .iter()
        .map(|l| h.located(&format!("row-{l}")).flow)
        .collect();

        let total: f32 = rows.iter().map(|r| r.height).sum();
        let gaps = reorder::GAP * (rows.len() - 1) as f32;
        assert!(
            (total + gaps - list.height).abs() < 0.5,
            "at {height}px the rows and gaps total {:.2} but the list is {:.2} tall: the rows must \
             divide the space they were actually given",
            total + gaps,
            list.height
        );
    }
}
