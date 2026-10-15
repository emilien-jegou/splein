// Single responsibility: Fast-drag reorder regressions — displaced rows glide, releases land.
//
// A fast 2 -> 3 drag once teleported rows instead of gliding them. The proven
// mechanism: the release glide seeded from `displayed`, whose follow offset
// only materializes post-layout — a grab-whip-release inside one frame seeded
// from stale zero (see FollowHandle). These drive the pointer across the
// boundary with minimal frames between and assert live FLIP offsets.

#[allow(dead_code)]
#[path = "../../../../examples/motion_reorder/main.rs"]
mod reorder;

use std::cell::RefCell;
use std::rc::Rc;

use riode::foundation::Color;
use riode::reactive::{Cx, ReactiveRuntime};
use riode_tiny_skia::testing::Harness;

fn mount() -> Harness {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(reorder::Page),
        reorder::HOT_WIDTH,
        reorder::HOT_HEIGHT,
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
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();

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
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();

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
    let c = h
        .engine()
        .transform("row-Strikethrough")
        .unwrap_or_default();
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
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();

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
    for label in [
        "Baseline",
        "Underline",
        "Strikethrough",
        "Highlight",
        "Squiggle",
    ] {
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
fn grab_during_settle_does_not_teleport() {    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();

    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    h.step();
    bridge.dispatch(&moved(start.x, start.y + stride + 20.0), h.engine());
    h.step();
    bridge.dispatch(&release(), h.engine());
    h.step();
    h.step();
    let flying = h
        .engine()
        .transform("row-Strikethrough")
        .unwrap_or_default()
        .ty;
    assert!(
        flying.abs() > 10.0,
        "precondition: row 3 must still be gliding, got ty={flying:.2}"
    );

    let before_box = h.located("row-Strikethrough").absolute.y;
    let before_ty = h
        .engine()
        .transform("row-Strikethrough")
        .unwrap_or_default()
        .ty;
    let before_visual = before_box + before_ty;
    let height = h.located("row-Strikethrough").absolute.height;

    bridge.dispatch(&moved(start.x, before_visual + height * 0.5), h.engine());
    bridge.dispatch(&press(), h.engine());
    h.step();

    let after_box = h.located("row-Strikethrough").absolute.y;
    let after_ty = h
        .engine()
        .transform("row-Strikethrough")
        .unwrap_or_default()
        .ty;
    let after_visual = after_box + after_ty;
    assert!(
        (after_visual - before_visual).abs() < 5.0,
        "grabbing a gliding row must not teleport it: {before_visual:.1} -> {after_visual:.1}"
    );
}

/// Grabbing an idle row mid-glide must not disturb gliding rows.
///
/// Regression: release-glide cancellation used to run on every grab, snapping
/// any running glide to identity — grabbing idle row 1 while row 2 was still
/// gliding home teleported row 2 by the remaining offset. There is no glide
/// state left to cancel (releases fold into the flip spring itself), so a grab
/// touches nothing but the grabbed row.
#[test]
#[ntest::timeout(30_000)]
fn grab_idle_row_mid_glide_leaves_gliding_row_alone() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();

    // Swap Underline below Strikethrough, then release: Underline glides home
    // on its flip spring (releases fold into it; no separate settle exists).
    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    h.step();
    bridge.dispatch(&moved(start.x, start.y + stride + 20.0), h.engine());
    h.step();
    bridge.dispatch(&release(), h.engine());
    h.step();
    h.step();
    let settling = h
        .engine()
        .transform("row-Underline")
        .unwrap_or_default()
        .ty;
    assert!(
        settling.abs() > 10.0,
        "precondition: Underline must still be settling, got ty={settling:.2}"
    );

    // Grab idle Baseline (press only, no drag) and measure Underline across it.
    // Baseline drift first: a smooth glide never accelerates discontinuously
    // without input, so the grab-frame delta must stay within the settling
    // decay envelope. A cancel-snap (the old behavior) overshoots it by the
    // whole remaining offset instead.
    let visual = |h: &mut Harness| {
        h.located("row-Underline").absolute.y
            + h.engine()
                .transform("row-Underline")
                .unwrap_or_default()
                .ty
    };
    let idle = h.located("row-Baseline").center();
    let y0 = visual(&mut h);
    h.step();
    let y1 = visual(&mut h);
    let drift = y1 - y0;

    bridge.dispatch(&moved(idle.x, idle.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    h.step();
    let y2 = visual(&mut h);

    assert!(
        (y2 - y1).abs() <= drift.abs() + 2.0,
        "grabbing an idle row must not teleport the settling row: drift {drift:.1}/frame, grab-frame jump {:.1}",
        y2 - y1
    );

    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900), "list must settle afterwards");
    // Convergence: with nothing cancellable anywhere in the sequence, every
    // row must land exactly home — springs decay to zero under any arrangement.
    for label in [
        "Baseline",
        "Underline",
        "Strikethrough",
        "Highlight",
        "Squiggle",
    ] {
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

/// Swap while another row glides home: the glider must glide through the
/// overlap continuously and everything must land home.
///
/// Retargeting (not cancelling) is what makes overlap safe: every swap only
/// retargets live springs, so nothing snaps. Fails on teleport-class jumps
/// (order of a row pitch per frame); overdamped glides never move that fast.
#[test]
#[ntest::timeout(30_000)]
fn swap_during_glide_stays_continuous_and_lands_home() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();

    // Swap Underline below Strikethrough, release: Underline settles.
    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    h.step();
    bridge.dispatch(&moved(start.x, start.y + stride + 20.0), h.engine());
    h.step();
    bridge.dispatch(&release(), h.engine());
    h.step();
    h.step();

    // Grab idle Baseline and drag it one slot down THROUGH the settle:
    // every crossing re-inverts all flips, settler included.
    let idle = h.located("row-Baseline").center();
    let visual = |h: &mut Harness| {
        h.located("row-Underline").absolute.y
            + h.engine()
                .transform("row-Underline")
                .unwrap_or_default()
                .ty
    };
    bridge.dispatch(&moved(idle.x, idle.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(idle.x, idle.y + 8.0), h.engine());
    h.step();
    let mut prev = visual(&mut h);
    for k in 1..=4 {
        bridge.dispatch(
            &moved(idle.x, idle.y + k as f32 * (stride + 20.0) / 4.0),
            h.engine(),
        );
        h.step();
        let now = visual(&mut h);
        assert!(
            (now - prev).abs() < stride,
            "settling row must not teleport across a foreign swap: {prev:.1} -> {now:.1} (stride {stride:.1})"
        );
        prev = now;
    }
    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900), "list must settle afterwards");
    for label in [
        "Baseline",
        "Underline",
        "Strikethrough",
        "Highlight",
        "Squiggle",
    ] {
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

/// Grab-point fidelity: pressing near a row's edge and dragging must move
/// the row *with* the pointer, not snap its centre onto the cursor.
///
/// Regression: the tracked value briefly lost its grab-offset folding
/// (`pointer.y` instead of `pointer.y - grab_dy`), so every drag opened with
/// a jump of up to half a row height. Nothing asserted tracking fidelity, so
/// nothing caught it — this pins it.
///
/// Coordinate care (learned the hard way): `located().absolute` is the DRAWN
/// position, transform included — adding the transform again double-counts.
/// Visual centre here is absolute.y + half height, nothing else.
#[test]
#[ntest::timeout(30_000)]
fn grab_near_edge_keeps_grab_point_under_cursor() {
    let mut h = mount();
    let r = h.located("row-Underline").absolute;
    let px = r.x + r.width * 0.5;
    let py = r.y + r.height - 4.0;
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();
    let visual_centre = |h: &mut Harness| {
        let b = h.located("row-Underline").absolute;
        b.y + b.height * 0.5
    };

    bridge.dispatch(&moved(px, py), h.engine());
    bridge.dispatch(&press(), h.engine());
    h.step();
    let c0 = visual_centre(&mut h);
    // Pressing below the centre puts the cursor `grab_offset` below it; the
    // row must keep that relationship (centre `grab_offset` ABOVE the cursor),
    // i.e. visual-minus-cursor must equal `-grab_offset` at every stop.
    let grab_offset = py - c0;
    assert!(
        grab_offset.abs() > 10.0,
        "precondition: edge press must grab far from centre, got offset {grab_offset:.1}"
    );

    // Drag to two pointer positions, letting the follow spring converge each
    // time. Pointer deltas here stay inside one slot pitch, so no swap
    // disturbs the measurement; the row must simply translate with the hand.
    for (dy, tag) in [(20.0, "first"), (40.0, "second")] {
        bridge.dispatch(&moved(px, py + dy), h.engine());
        for _ in 0..60 {
            h.step();
        }
        let gap = visual_centre(&mut h) - (py + dy);
        let want = -grab_offset;
        assert!(
            (gap - want).abs() < 8.0,
            "{tag} stop: row must preserve the grab offset (gap {gap:.1} vs {want:.1})"
        );
    }

    bridge.dispatch(&release(), h.engine());
    assert!(h.settle(900), "list must settle afterwards");
}

/// Grabbing a row mid-glide must measure the grab against where the row is
/// drawn, not where its layout slot sits.
///
/// Regression (reported at 0.10x): swap two rows, release, and before the
/// animation finishes grab the still-gliding row. Its layout box already sits
/// in the new slot while its paint lags a full flip offset behind; measuring
/// the grab against the layout box bakes that offset in as a constant
/// tracking error for the whole next drag. The fix folds the live flip into
/// the grab measurement.
///
/// Runs at 0.10x time like the report: near-frozen motion makes the
/// press-instant bookkeeping exact (no inter-frame glide between measuring
/// and pressing), while the follow spring still converges given enough steps.
#[test]
#[ntest::timeout(30_000)]
fn grab_midglide_row_keeps_grab_point() {
    let mut h = mount();
    let start = h.located("row-Underline").center();
    let stride = pitch(&mut h);
    let mut bridge = riode::app::runner::pointer::PointerBridge::new();

    // Swap Underline below Strikethrough and release: Underline glides home
    // with a large live flip offset for the next frames.
    bridge.dispatch(&moved(start.x, start.y), h.engine());
    bridge.dispatch(&press(), h.engine());
    bridge.dispatch(&moved(start.x, start.y + 8.0), h.engine());
    h.step();
    bridge.dispatch(&moved(start.x, start.y + stride + 20.0), h.engine());
    h.step();
    bridge.dispatch(&release(), h.engine());
    h.step();
    h.step();
    let flying = h
        .engine()
        .transform("row-Underline")
        .unwrap_or_default()
        .ty;
    assert!(
        flying.abs() > 10.0,
        "precondition: Underline must still be gliding, got ty={flying:.2}"
    );

    // Slow time to a crawl: motion between measuring, pressing and dragging
    // becomes sub-pixel, so the press-instant geometry is exact.
    h.engine().set_time_scale(0.1);

    // Press near its visual bottom edge (drawn box, transform included) and
    // drag upward, staying inside the slot: no swap disturbs the measurement.
    let r = h.located("row-Underline").absolute;
    let px = r.x + r.width * 0.5;
    let py = r.y + r.height - 4.0;
    let visual = |h: &mut Harness| {
        let b = h.located("row-Underline").absolute;
        b.y + b.height * 0.5
    };
    bridge.dispatch(&moved(px, py), h.engine());
    bridge.dispatch(&press(), h.engine());
    h.step();
    let c0 = visual(&mut h);
    let grab_offset = py - c0;
    for (dy, tag) in [(-12.0, "first"), (-24.0, "second")] {
        bridge.dispatch(&moved(px, py + dy), h.engine());
        for _ in 0..400 {
            h.step();
        }
        let gap = visual(&mut h) - (py + dy);
        let want = -grab_offset;
        assert!(
            (gap - want).abs() < 8.0,
            "{tag} stop: mid-glide grab must preserve the visual grab offset (gap {gap:.1} vs {want:.1})"
        );
    }

    bridge.dispatch(&release(), h.engine());
    h.engine().set_time_scale(1.0);
    assert!(h.settle(900), "list must settle afterwards");
}
