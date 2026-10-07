// Single responsibility: A released row eases into its slot instead of snapping,
// and hands control back to the FLIP with no residual jump.
//
// Dropping a held row clears the pointer follow in one frame. Without a glide the row
// snaps home; with a glide that ends on a fixed timer alone, the FLIP gate reopens
// while the spring is still decaying from the drag's last reorder, and the residual
// snaps in right as the glide looks finished. This pins both halves: the offset must
// decay monotonically after release, and the settled transform must be exactly zero.

#[allow(dead_code)]
#[path = "../examples/motion_reorder.rs"]
mod reorder;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::foundation::Color;
use aurora::reactive::{Cx, ReactiveRuntime};
use aurora_tiny_skia::testing::Harness;

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

fn moved(x: f32, y: f32) -> winit::event::WindowEvent {
    winit::event::WindowEvent::CursorMoved {
        position: winit::dpi::PhysicalPosition::new(x as f64, y as f64),
        device_id: unsafe { winit::event::DeviceId::dummy() },
    }
}

#[test]
#[ntest::timeout(5_000)]
fn a_released_row_glides_home_without_a_residual_jump() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();
    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());

    // Down past one row, in sub-stride increments so a reorder fires and the FLIP holds
    // a real decaying offset — the exact condition the glide must wait out.
    for i in 1..=12 {
        bridge.dispatch(&moved(start.x, start.y + 12.0 * i as f32), h.engine());
        h.step();
    }
    bridge.dispatch(&release(), h.engine());
    h.step();

    let f0 = h.transform("row-Underline").ty;
    assert!(
        f0.abs() > 5.0,
        "releasing mid-drag must leave a real offset to glide from, got {f0:.1}px"
    );

    // ~100ms into the 220ms glide: strictly decayed, and nowhere near home. A snap
    // would read ~0 here (or the FLIP residual); a stalled glide would read ~f0.
    h.steps(6);
    let f1 = h.transform("row-Underline").ty;
    assert!(
        f1.abs() > 0.0 && f1.abs() < f0.abs() * 0.5,
        "mid-glide offset must be strictly between release ({f0:.1}px) and home, got {f1:.1}px"
    );
    assert!(
        f0.signum() == f1.signum(),
        "the glide must ease toward the slot, not overshoot through it ({f0:.1} -> {f1:.1})"
    );

    // Frame by frame to rest: the offset must never grow. Ending the glide on the timer
    // alone reopens the FLIP gate onto a still-decaying spring, and the residual snaps
    // in as an upward jump right where the glide looks finished.
    let mut prev = f1.abs();
    for _ in 0..40 {
        h.step();
        let cur = h.transform("row-Underline").ty.abs();
        assert!(
            cur <= prev + 1e-3,
            "the landing must decay monotonically, but the offset grew {prev:.2}px -> {cur:.2}px"
        );
        prev = cur;
    }

    // Settled: exactly home, so the FLIP gate reopened onto a spring at rest.
    assert!(h.settle(900), "the glide must terminate");
    let f2 = h.transform("row-Underline").ty;
    assert_eq!(
        f2, 0.0,
        "a landed row must sit exactly in its slot, got {f2:.3}px"
    );
}
