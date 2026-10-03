// Single responsibility: The diagnostics harness itself: freezing, tracing, filmstrips, overflow scans.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::{group, text, IntoElement};
use aurora::foundation::{Color, Direction, Margin};
use aurora::input::PointerPhase;
use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use aurora::testing::{report, Escape, Filmstrip, Harness, Located, Rgba, Snapshot};

/// A card that slides between two positions on a spring, so there is always motion to freeze.
#[component]
fn Slider() -> impl IntoElement {
    let to_right = use_signal(false);
    let target = to_right.map(|right| if right { 120.0 } else { 0.0 });
    let x = Transition::spring(Spring::snappy()).on(target);
    let toggle = to_right.clone();
    let state = to_right;

    group()
        .width(400.0)
        .height(300.0)
        .fill(Color::WHITE)
        .children([group()
            .margin(aurora::foundation::Margin::all(40.0))
            .direction(Direction::Vertical)
            .children([group()
                .key("card")
                .direction(Direction::Vertical)
                .width(80.0)
                .height(60.0)
                .fill(Color::hex(0x1D4ED8))
                .x(x)
                .on_click(move |_| toggle.set(!state.get()))
                .children([text("drag me").size(12.0).color(Color::WHITE)])])])
}

fn harness() -> Harness {
    let mut h = Harness::page(Slider, 400, 300);
    h.steps(2);
    h
}

#[test]
#[ntest::timeout(30_000)]
fn the_harness_freezes_time_without_freezing_the_frame() {
    let mut h = harness();
    h.click("card");
    h.steps(4);

    h.pause();
    assert!(h.is_paused(), "the harness must report being paused");

    let before = h.transform("card");
    h.steps(10);
    let after = h.transform("card");
    assert_eq!(
        before.tx, after.tx,
        "a frozen engine must not integrate: {} -> {}",
        before.tx, after.tx
    );

    // The frame still renders while paused, which is the whole point: you can photograph a
    // transition mid-flight rather than only before or after it.
    let shot = h.snapshot();
    assert!(
        shot.width() > 0 && shot.height() > 0,
        "frames still rasterize"
    );

    h.resume();
    assert!(!h.is_paused());
    h.steps(10);
    assert!(
        (h.transform("card").tx - after.tx).abs() > 1.0,
        "resuming must let the spring continue"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn slow_motion_stretches_a_transition_over_more_frames() {
    let frames_to_arrive = |h: &mut Harness, budget: u32| -> u32 {
        h.click("card");
        let mut n = 0;
        while !h.is_settled() && n < budget {
            h.step();
            n += 1;
        }
        n
    };
    let fast_frames = frames_to_arrive(&mut harness(), 600);
    let mut slow = harness();
    slow.slow_motion(0.1);
    let slow_frames = frames_to_arrive(&mut slow, 6000);

    assert!(
        slow_frames > fast_frames * 2,
        "a tenth-speed clock must take far more frames: {fast_frames} vs {slow_frames}"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn settle_reports_whether_it_actually_settled() {
    let mut h = harness();
    assert!(h.is_settled(), "a fresh scene is already at rest");

    h.click("card");
    assert!(!h.is_settled(), "a spring in flight is not settled");
    assert!(h.settle(600), "it must settle inside the budget");
    assert!(h.is_settled());
}

#[test]
#[ntest::timeout(30_000)]
fn a_trace_reveals_the_shape_of_motion_not_just_its_endpoints() {
    let mut h = harness();
    h.click("card");
    let trace = h.trace("card", 40);

    let xs = trace.xs();
    assert!(xs.len() >= 40, "the trace must record every frame");
    assert!(
        trace.worst_jump() < 30.0,
        "a spring must not teleport: worst frame moved {}",
        trace.worst_jump()
    );
    assert!(
        trace.path_length() > 100.0,
        "the card must actually travel, path was {}",
        trace.path_length()
    );

    let (lo, hi) = trace.range(&xs);
    assert!(
        hi > lo,
        "the trace must span more than one value, got {lo}..{hi}"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn located_names_the_three_coordinate_spaces_separately() {
    let h = harness();
    let here: Located = h.located("card");

    // A view inside a parent at a non-zero origin: its flow box is parent-local, so it must
    // not equal its screen box. This is the distinction that silently mis-aims pointers.
    assert_ne!(
        here.flow, here.absolute,
        "a nested view's flow box must differ from its on-screen box"
    );
    assert!(!here.is_moving(), "a settled view is not moving");
    assert!(
        here.absolute.contains(here.center()),
        "center() must land inside the view it names"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn clicking_by_key_lands_on_the_view_and_nowhere_else() {
    let mut h = harness();
    let card = h.located("card").center();

    // The click is aimed from the key's own screen box, so it cannot miss.
    assert_eq!(
        h.engine().hit_test(card.x, card.y),
        Some(aurora::foundation::Key::from("card")),
        "the harness must aim at the box it resolved"
    );
    h.click("card");
    h.settle(600);
    assert!(
        h.transform("card").tx > 60.0,
        "the click must have toggled the card right, at {}",
        h.transform("card").tx
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_filmstrip_composites_and_labels_its_frames() {
    let mut h = harness();
    let mut frames = Vec::new();
    h.click("card");
    for target in [0u64, 4, 10, 20, 40] {
        h.step_to(target);
        frames.push((target, h.snapshot()));
    }

    let strip = Filmstrip::new(frames);
    assert_eq!(strip.len(), 5);
    let rendered = strip.render();
    assert!(
        rendered.width() >= 5 * 400,
        "a row strip must be at least as wide as its frames, got {}",
        rendered.width()
    );

    // Downscaling keeps a whole transition reviewable in one image.
    let small = strip.scaled(2).column().render();
    assert!(
        small.height() > rendered.height() / 2,
        "a column stacks frames"
    );

    strip.save("/tmp/opencode/harness-strip.png");
    assert!(
        std::path::Path::new("/tmp/opencode/harness-strip.png").exists(),
        "the strip must be written to disk"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn snapshots_detect_change_and_equality() {
    let mut h = harness();
    h.click("card");
    let a = h.snapshot();

    h.step();
    let b = h.snapshot();
    assert!(
        !a.matches(&b, 0),
        "a frame mid-transition must differ from the last"
    );

    h.settle(600);
    let settled = h.snapshot();
    h.step();
    let after = h.snapshot();
    assert!(
        settled.matches(&after, 0),
        "once settled, further frames must be pixel-identical"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn runs_read_a_stack_of_cards_as_solid_runs() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let card = |i: u32| {
        group()
            .key(format!("c{i}"))
            .width(200.0)
            .height(50.0)
            .fill(Color::WHITE)
            .margin(Margin::all(10.0))
    };
    let mut h = Harness::from_runtime(
        Rc::clone(&runtime),
        cx.enter(|| {
            group()
                .width(400.0)
                .height(400.0)
                .fill(Color::hex(0x111111))
                .direction(Direction::Vertical)
                .children((card(0), card(1), card(2)))
        }),
        400,
        400,
    );
    h.step();

    let white = |p: Rgba| {
        p.near(
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            12,
        )
    };
    let shot: Snapshot = h.snapshot();
    let runs = shot.runs_down(200, white);
    assert_eq!(
        runs.len(),
        3,
        "three cards must read as three runs, got {runs:?}"
    );
    assert!(
        shot.widest_run(200, white).is_some(),
        "the widest run must be found"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn the_overflow_scan_finds_content_escaping_an_unclipped_parent() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let mut h = Harness::from_runtime(
        Rc::clone(&runtime),
        cx.enter(|| {
            group()
                .width(400.0)
                .height(400.0)
                .fill(Color::WHITE)
                .children([group()
                    .width(100.0)
                    .height(100.0)
                    // Absolute so the shrink pass cannot pull it back inside: the point is
                    // a child that genuinely reaches past a parent that does not clip.
                    .children([group()
                        .key("escaping")
                        .absolute()
                        .anchor(aurora::foundation::Anchor::TopLeft)
                        .margin(aurora::foundation::Margin::top(150.0))
                        .width(40.0)
                        .height(40.0)
                        .fill(Color::BLACK)
                        .into_element()])
                    .into_element()])
                .into_element()
        }),
        400,
        400,
    );
    h.step();

    let escapes = report(h.engine());
    assert!(
        escapes
            .iter()
            .any(|e| matches!(e, Escape::ContentOutsideParent { .. })),
        "a 400-wide child in a 100-wide unclipped parent must be reported, got {:?}",
        escapes.iter().map(|e| e.describe()).collect::<Vec<_>>()
    );
    assert!(
        escapes.iter().all(|e| !e.describe().is_empty()),
        "every escape must describe itself"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn the_overflow_scan_is_quiet_when_nothing_escapes() {
    let mut h = harness();
    h.step();
    let escapes = report(h.engine());
    assert!(
        escapes.is_empty(),
        "a simple nested scene must not report escapes, got {:?}",
        escapes.iter().map(|e| e.describe()).collect::<Vec<_>>()
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_pointer_away_from_the_view_changes_nothing() {
    let mut h = harness();
    let before = h.transform("card");
    h.pointer(
        aurora::foundation::Point::new(390.0, 290.0),
        PointerPhase::Down,
    );
    h.step();
    h.pointer(
        aurora::foundation::Point::new(390.0, 290.0),
        PointerPhase::Up,
    );
    h.step();
    assert_eq!(
        before.tx,
        h.transform("card").tx,
        "a press on empty space must not move the view"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_drag_by_key_is_delivered_in_visible_increments() {
    let mut h = harness();
    let before = h.located("card").flow;
    h.drag_by("card", 100.0, 0.0, 8);
    assert!(
        h.frame() >= 10,
        "a drag must span many frames, took {}",
        h.frame()
    );

    // A drag is a compositor transform. The layout box must be untouched, which is only
    // checkable by comparing the flow rect across time — comparing it to the absolute rect
    // would compare two different coordinate spaces and prove nothing.
    assert_eq!(
        before,
        h.located("card").flow,
        "a drag must not move a view in layout"
    );
    h.settle(600);
    assert!(
        (h.transform("card").tx).abs() < 1.0,
        "with no layout motion the card returns to rest, at {}",
        h.transform("card").tx
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_fresh_harness_has_already_rendered_one_frame() {
    // Regression: a harness that had not stepped anything reported every rect as zero, and
    // `settle` returned immediately because nothing was moving yet. Both looked like a page
    // that had laid out to nothing.
    let mut h = Harness::page(Slider, 400, 300);
    assert_eq!(h.frame(), 1, "the constructor must render one frame");
    let here = h.located("card");
    assert!(
        here.flow.width > 0.0 && here.flow.height > 0.0,
        "a mounted view must have a real box, got {here:?}"
    );
    assert!(h.snapshot().width() > 0, "and a real frame");
}

/// A card that follows the pointer while it is held, so a drag's held state is observable.
#[component]
fn Draggable() -> impl IntoElement {
    let held = use_signal(0.0f32);
    let step = held.clone();
    let ty = held.clone();

    group()
        .width(400.0)
        .height(300.0)
        .fill(Color::WHITE)
        .children([group()
            .margin(aurora::foundation::Margin::all(40.0))
            .width(80.0)
            .height(60.0)
            .fill(Color::hex(0x1D4ED8))
            .key("card")
            .y(ty)
            .on_drag_step(move |info| step.set(info.delta.y))
            .children([text("drag me").size(12.0).color(Color::WHITE)])])
}

#[test]
#[ntest::timeout(30_000)]
fn a_drag_can_be_inspected_while_it_is_still_held() {
    // The reason `drag_holding` exists. `drag_by` releases before returning, so by the time
    // it returns the card has usually sprung most of the way home and the held reading is
    // gone; asserting on it measures the release, not the drag.
    let mut h = Harness::page(Draggable, 400, 300);
    h.steps(2);

    h.drag_holding("card", 0.0, 70.0, 7);
    assert!(
        h.transform("card").ty.abs() > 1.0,
        "the card must be displaced while the pointer is still down, it is at {}",
        h.transform("card").ty
    );

    // Releasing must lift exactly where the drag stopped, not at the view's new centre --
    // the card has moved, so a release aimed at its current centre would be somewhere the
    // pointer never went.
    h.release_at();
    assert!(
        h.transform("card").ty.abs() > 1.0,
        "the release must not have snapped the card back before the next frame"
    );
}
