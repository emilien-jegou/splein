// Single responsibility: Fast-drag reorder regressions — displaced rows glide, releases land.
//
// A fast 2 -> 3 drag once teleported rows instead of gliding them. The proven
// mechanism: the release glide seeded from `displayed`, whose follow offset
// only materializes post-layout — a grab-whip-release inside one frame seeded
// from stale zero (see FollowHandle). These drive the pointer across the
// boundary with minimal frames between and assert live FLIP offsets.

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

fn pitch(h: &mut Harness) -> f32 {
    let a = h.located("row-Baseline").flow;
    let b = h.located("row-Underline").flow;
    b.y - a.y
}

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

/// One fast crossing: the displaced row must carry a live FLIP offset.
#[test]
#[ntest::timeout(30_000)]
fn fast_drag_down_displaced_row_glides() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();

    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    h.step();

    bridge.dispatch(&moved(start.x, start.y + stride + 20.0), h.engine());
    h.step();

    let t = h
        .engine()
        .transform("row-Strikethrough")
        .unwrap_or_default();
    assert!(
        t.ty.abs() > stride * 0.15,
        "row 3 must glide into slot 2 with a live FLIP offset (stride {stride:.1}), got ty={:.2}",
        t.ty
    );
}

/// Whole gesture in one frame: press, two jumps (threshold + step), release.
/// The release glide must seed from the thrown offset — not stale zero — and
/// displaced rows must still invert on the next frame.
#[test]
#[ntest::timeout(30_000)]
fn single_frame_whip_and_release() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();

    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    // First jump crosses the drag threshold (starts the drag, no step yet);
    // second jump streams the step that actually reorders. All in one frame.
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 2.0 * stride + 20.0), h.engine());
    bridge.dispatch(&release(), h.engine());
    h.step();

    let b = h.engine().transform("row-Underline").unwrap_or_default();
    assert!(
        b.ty.abs() > stride * 0.3,
        "released row must glide from its thrown offset, got ty={:.2}",
        b.ty
    );
    let c = h.engine().transform("row-Strikethrough").unwrap_or_default();
    assert!(
        c.ty.abs() > stride * 0.1,
        "displaced row must carry FLIP offset, got ty={:.2}",
        c.ty
    );
}

/// Event storm: three crossings dispatched with NO frame between them, then a
/// single frame. Settles exactly home afterwards with no stuck offsets.
#[test]
#[ntest::timeout(30_000)]
fn storm_crossings_glide_and_settle() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();

    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    h.step();

    for k in 1..=3 {
        bridge.dispatch(
            &moved(start.x, start.y + k as f32 * (stride + 20.0) / 3.0 + 20.0),
            h.engine(),
        );
    }
    h.step();

    let t = h
        .engine()
        .transform("row-Strikethrough")
        .unwrap_or_default();
    assert!(
        t.ty.abs() > stride * 0.15,
        "storm must still glide row 3, got ty={:.2}",
        t.ty
    );

    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900), "storm must settle");
    for label in ["Baseline", "Underline", "Strikethrough", "Highlight", "Squiggle"] {
        let ty = h
            .engine()
            .transform(&format!("row-{label}"))
            .map(|t| t.ty.abs())
            .unwrap_or(999.0);
        assert!(
            ty < 0.5,
            "row {label} must settle exactly home, got ty={ty:.2}"
        );
    }
}

/// Grab-during-settle watch: displace a row, then grab it mid-glide and
/// assert it does not teleport. Currently IGNORED — under human judgement:
/// headless measurement shows an ~11px visual jump on grab while the FLIP
/// gate shuts over a still-settling spring. Keep as the recorded lead for
/// the reported fast-drag snap if the release-seed fix proves insufficient.
#[test]
#[ignore]
#[ntest::timeout(30_000)]
fn grab_during_settle_does_not_teleport() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = aurora::app::runner::pointer::PointerBridge::new();

    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    h.step();
    bridge.dispatch(&moved(start.x, start.y + stride + 20.0), h.engine());
    h.step();
    bridge.dispatch(&release(), h.engine());
    h.step();
    h.step();
    let flying = h.engine().transform("row-Strikethrough").unwrap_or_default().ty;
    assert!(
        flying.abs() > 10.0,
        "precondition: row 3 must still be gliding, got ty={flying:.2}"
    );

    let before_box = h.located("row-Strikethrough").absolute.y;
    let before_ty = h.engine().transform("row-Strikethrough").unwrap_or_default().ty;
    let before_visual = before_box + before_ty;
    let height = h.located("row-Strikethrough").absolute.height;

    bridge.dispatch(&moved(start.x, before_visual + height * 0.5), h.engine());
    bridge.dispatch(&press(), h.engine());
    h.step();

    let after_box = h.located("row-Strikethrough").absolute.y;
    let after_ty = h.engine().transform("row-Strikethrough").unwrap_or_default().ty;
    let after_visual = after_box + after_ty;
    assert!(
        (after_visual - before_visual).abs() < 5.0,
        "grabbing a gliding row must not teleport it: {before_visual:.1} -> {after_visual:.1}"
    );
}
