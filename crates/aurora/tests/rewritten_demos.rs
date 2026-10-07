// Single responsibility: Verify each rewritten demo actually animates, reacts, and reorders.
//
// The colours asserted here were measured off rendered frames, not read off the demos'
// source, so that changing the shared palette does not silently turn these into assertions
// that pass on nothing. See the note on `accent_area` for the bug that motivated that.

#[allow(dead_code)]
#[path = "../examples/motion_imperative.rs"]
mod imperative;
#[allow(dead_code)]
#[path = "../examples/motion_interruption.rs"]
mod interruption;
#[allow(dead_code)]
#[path = "../examples/motion_layout_split.rs"]
mod layout_split;
#[allow(dead_code)]
#[path = "../examples/motion_press.rs"]
mod press;
#[allow(dead_code)]
#[path = "../examples/motion_reorder.rs"]
mod reorder;
#[allow(dead_code)]
#[path = "../examples/motion_signals.rs"]
mod signals;

use aurora::foundation::Color;
use aurora_tiny_skia::testing::{report, Harness, Rgba};

/// `common::motion_ui::ACCENT`, measured off a rendered frame.
const ACCENT: Rgba = Rgba {
    r: 0x10,
    g: 0x19,
    b: 0xEC,
    a: 255,
};

/// `common::motion_ui::PAGE`. The demos share one page colour, which makes it a reliable
/// way to ask "did this frame draw anything at all" without naming a hue.
const PAGE: Rgba = Rgba {
    r: 0xF4,
    g: 0xF5,
    b: 0xF7,
    a: 255,
};

/// Sampled pixels of the accent colour.
fn accent_area(h: &mut Harness) -> usize {
    h.view().count_matching(3, |p| p.near(ACCENT, 12))
}

/// Sampled pixels that are not the page colour, i.e. everything the demo actually drew.
///
/// Deliberately hue-agnostic: the demos do not share an accent. `motion_imperative` paints
/// `#10B981` and a dark tinted surface rather than the palette accent, so an assertion that
/// every demo paints `ACCENT` is simply false, not a real signal.
fn drawn_area(h: &mut Harness) -> usize {
    h.view().count_matching(4, |p| !p.near(PAGE, 8))
}

#[test]
#[ntest::timeout(5_000)]
fn layout_split_moves_its_card_without_relayout() {
    let mut h = Harness::page(
        layout_split::Page,
        layout_split::WIDTH,
        layout_split::HEIGHT,
    )
    .with_background(Color::hex(0xF4F5F7));
    let flow_before = h.located("card").flow;

    h.steps(30);

    let moved = h.transform("card").tx;
    assert!(
        moved.abs() > 1.0,
        "the frame clock must move the card, it is at {moved}"
    );
    assert_eq!(
        h.located("card").flow,
        flow_before,
        "the card's layout box must never move: this is the whole point of the demo"
    );
    assert!(
        accent_area(&mut h) > 200,
        "the card must actually be painted, found {} accent samples",
        accent_area(&mut h)
    );
}

#[test]
#[ntest::timeout(5_000)]
fn signals_drives_every_row_from_one_spring() {
    let mut h = Harness::page(signals::Page, signals::WIDTH, signals::HEIGHT)
        .with_background(Color::hex(0xF4F5F7));
    let before = h.view();

    // Past the first reversal the puck has travelled and the swatch has recoloured.
    h.steps(130);
    let after = h.view();

    assert!(
        before.count_matching(3, |p| p.near(ACCENT, 12)) > 200,
        "the accent puck must be painted at the start"
    );
    assert!(
        after.count_matching(3, |p| p.near(ACCENT, 12)) > 200,
        "and must still be painted at the end"
    );

    // The colour row is fed by the same spring, so a large part of the frame must have
    // changed hue. Asserting that a *particular* swatch colour is present is weaker and
    // wrong: which colour is on screen depends on where the spring happens to be.
    assert!(
        before.changed_pixels(&after, 12) > 500,
        "one spring must drive the whole page; only {} samples changed",
        before.changed_pixels(&after, 12)
    );
}

#[test]
#[ntest::timeout(5_000)]
fn imperative_moves_panel_and_badge_on_their_own_curves() {
    let mut h = Harness::page(imperative::Page, imperative::WIDTH, imperative::HEIGHT)
        .with_background(Color::hex(0xF4F5F7));
    let panel_before = h.transform("panel");
    let badge_before = h.transform("badge");

    // The pose toggles every 1.7s, so the first change lands past the hundredth frame.
    h.steps(130);

    let panel_after = h.transform("panel");
    let badge_after = h.transform("badge");
    assert!(
        (panel_after.tx - panel_before.tx).abs() > 1.0
            || (panel_after.a - panel_before.a).abs() > 0.01,
        "the panel must animate its tween streams, {panel_before:?} -> {panel_after:?}"
    );
    assert!(
        (badge_after.a - badge_before.a).abs() > 0.01,
        "the badge must animate on its own spring, {badge_before:?} -> {badge_after:?}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn interruption_never_lets_the_card_snap() {
    let mut h = Harness::page(
        interruption::Page,
        interruption::WIDTH,
        interruption::HEIGHT,
    )
    .with_background(Color::hex(0xF4F5F7));

    // Retargets land every 450ms. A trace is what makes this checkable: a pop is a single
    // frame that moves the card most of the way, which comparing two endpoints cannot see.
    let trace = h.trace("card", 200);
    assert!(
        trace.worst_jump() < 90.0,
        "a retarget must not pop the card; worst single-frame jump was {:.1}px",
        trace.worst_jump()
    );
    assert!(
        trace.path_length() > 300.0,
        "the card must actually be travelling, path was {:.1}px",
        trace.path_length()
    );
}

#[test]
#[ntest::timeout(5_000)]
fn press_responds_to_a_real_press_and_springs_back() {
    let mut h = Harness::page(press::Page, press::WIDTH, press::HEIGHT)
        .with_background(Color::hex(0xF4F5F7));
    let rest = h.transform("cta").a;
    assert!(
        (rest - 1.0).abs() < 0.02,
        "the button starts at rest, {rest}"
    );

    h.press("cta");
    // The eased squeeze takes 90ms, so let it play out while the button is held.
    h.steps(10);
    let held = h.transform("cta").a;
    assert!(
        held < 0.98,
        "holding the button must squeeze it, scale is {held}"
    );

    h.release("cta");
    assert!(h.settle(600), "the button must come to rest after release");
    let released = h.transform("cta").a;
    assert!(
        (released - 1.0).abs() < 0.02,
        "releasing must spring back to rest, scale is {released}"
    );
}

/// Labels in the reorder demo, in their declared order.
const ROWS: [&str; 5] = [
    "Baseline",
    "Underline",
    "Strikethrough",
    "Highlight",
    "Squiggle",
];

/// A settled reorder page. The rows spring in from nothing at mount, and a drag aimed at a
/// moving box would land on whatever happened to be underneath instead.
/// A row's slot in the settled list, by position.
fn row_index(h: &mut Harness, label: &str) -> usize {
    let mut rows: Vec<(f32, &str)> = ROWS
        .iter()
        .map(|l| (h.located(&format!("row-{l}")).absolute.y, *l))
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    rows.iter()
        .position(|(_, l)| *l == label)
        .expect("row exists")
}

fn reorder_ready() -> Harness {
    let mut h = Harness::page(reorder::Page, reorder::WIDTH, reorder::HEIGHT)
        .with_background(Color::hex(0xF4F5F7));
    assert!(
        h.settle(900),
        "the reorder list must come to rest before anything is aimed at it"
    );
    h
}

#[test]
#[ntest::timeout(5_000)]
fn reorder_rows_actually_paint() {
    // Non-vacuous by construction: damage-parity tests compare two canvases and pass
    // when both are blank, so only an absolute pixel count catches a backend that
    // silently drops draws (e.g. tiny-skia skips draws whose mask dimensions differ
    // from the target — a blank canvas then "matches" a blank truth).
    let mut h = reorder_ready();
    let painted = accent_area(&mut h);
    assert!(
        painted > 200,
        "the settled rows must actually be painted, found {painted} accent samples"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn reorder_lifts_a_row_and_swaps_the_list() {
    let mut h = reorder_ready();

    // Travel two whole strides so the list swaps twice. `drag_holding` leaves the button down:
    // the offset only exists while the pointer is still held.
    let at = h.located("row-Baseline").center();
    h.drag_holding("row-Baseline", 0.0, 20.0 * 6.0, 6);
    // Asserted against the pointer rather than the transform offset. The offset is the pointer's
    // displacement *minus* the slot movement the reorder caused, so it is legitimately zero when
    // the row lands exactly on a new slot -- which is what two whole strides does.
    let held_mid = h.located("row-Baseline").absolute.y + row_height(&h) * 0.5;
    assert!(
        (held_mid - (at.y + 120.0)).abs() <= 1.0,
        "the held row must sit under the pointer: at {held_mid:.1}, pointer at {:.1}",
        at.y + 120.0
    );

    h.release_at();
    assert!(h.settle(900), "the reordered list must settle");
    assert!(
        h.transform("row-Baseline").ty.abs() < 2.0,
        "the released row must spring back onto its slot, offset {}",
        h.transform("row-Baseline").ty
    );

    // The rows the dragged one passed are the ones that move; the ones it did not pass keep
    // their slots. Asserting on the last row instead would only pass when the double-counted
    // target happened to shuffle the whole list.
    assert_eq!(
        row_index(&mut h, "Baseline"),
        2,
        "a drag of two whole strides must land the dragged row two slots down"
    );
    assert_eq!(
        row_index(&mut h, "Underline"),
        0,
        "the row it passed must move up"
    );
    assert_eq!(
        row_index(&mut h, "Strikethrough"),
        1,
        "and so must the next one"
    );
    assert_eq!(
        row_index(&mut h, "Squiggle"),
        4,
        "a row the drag never reached must keep its slot"
    );
    for label in ROWS {
        assert!(
            h.at(&format!("row-{label}")).is_some(),
            "{label} must still be in the list"
        );
    }
}

#[test]
#[ntest::timeout(5_000)]
fn a_reorder_uses_the_real_row_pitch() {
    let h = reorder_ready();

    let ys: Vec<f32> = ROWS
        .iter()
        .map(|l| h.located(&format!("row-{l}")).flow.y)
        .collect();
    let heights: Vec<f32> = ROWS
        .iter()
        .map(|l| h.located(&format!("row-{l}")).flow.height)
        .collect();

    // Nothing declares a row height any more: the rows fill the list and layout divides the space
    // it was actually given. So the properties worth asserting are uniformity and agreement with
    // the gap -- not equality with a constant.
    //
    // Uniformity is what the drag maths depends on. It divides a pointer delta by a single pitch
    // to decide when to swap, so if the engine ever handed rows different heights that arithmetic
    // would swap at the wrong moment on every row but one.
    let first = heights[0];
    for (label, height) in ROWS.iter().zip(&heights) {
        assert!(
            (height - first).abs() < 0.01,
            "{label} must be the same height as every other row ({first}), got {height}"
        );
    }

    // And the pitch must be exactly the row height plus the gap, for the same reason.
    let pitch = row_height(&h) + reorder::GAP;
    for measured in ys.windows(2).map(|w| w[1] - w[0]) {
        assert!(
            (measured - pitch).abs() < 0.05,
            "pitch must be row height + gap ({pitch:.2}), got {measured:.2}"
        );
    }
}

#[test]
#[ntest::timeout(5_000)]
fn a_drag_across_several_strides_lands_the_row_at_the_bottom() {
    let mut h = reorder_ready();
    let last_y = h.located("row-Squiggle").absolute.y;

    // Four strides, delivered in sub-stride increments so the row is genuinely held
    // throughout rather than teleporting slot to slot.
    h.drag_by(
        "row-Baseline",
        0.0,
        4.0 * (row_height(&h) + reorder::GAP),
        40,
    );
    assert!(h.settle(900), "the reordered list must settle");

    let moved_to = h.located("row-Baseline").absolute.y;
    assert!(
        moved_to > last_y - 1.0,
        "dragging four strides must land the row at the bottom, at {moved_to} was {last_y}"
    );

    // The untouched rows keep their relative order: a drag permutes, it does not reshuffle.
    let untouched: Vec<(String, f32)> = ["Underline", "Strikethrough", "Highlight", "Squiggle"]
        .iter()
        .map(|l| {
            let k = format!("row-{l}");
            (k.clone(), h.located(&k).absolute.y)
        })
        .collect();
    for pair in untouched.windows(2) {
        assert!(
            pair[0].1 < pair[1].1,
            "the untouched rows must keep their order, got {:?}",
            untouched
                .iter()
                .map(|(k, y)| format!("{k}@{y:.0}"))
                .collect::<Vec<_>>()
        );
    }

    assert!(
        h.transform("row-Baseline").ty.abs() < 2.0,
        "the released row must spring back onto its new slot, offset {}",
        h.transform("row-Baseline").ty
    );
}

/// Every demo paints something and reports little unexpected geometry.
///
/// The threshold is a smoke alarm, not a specification: a demo that starts drawing mostly
/// outside a non-clipping parent shows up here, and the per-demo tests above pin down
/// whether that is intended.
#[test]
#[ntest::timeout(5_000)]
fn every_demo_draws_and_reports_little_unexpected_geometry() {
    let pages: [(&str, fn() -> aurora::dsl::Element, u32, u32); 6] = [
        (
            "layout_split",
            layout_split::Page,
            layout_split::WIDTH,
            layout_split::HEIGHT,
        ),
        ("signals", signals::Page, signals::WIDTH, signals::HEIGHT),
        (
            "imperative",
            imperative::Page,
            imperative::WIDTH,
            imperative::HEIGHT,
        ),
        (
            "interruption",
            interruption::Page,
            interruption::WIDTH,
            interruption::HEIGHT,
        ),
        ("press", press::Page, press::WIDTH, press::HEIGHT),
        ("reorder", reorder::Page, reorder::WIDTH, reorder::HEIGHT),
    ];

    for (name, page_fn, w, h_) in pages {
        let mut h = Harness::page(page_fn, w, h_).with_background(Color::hex(0xF4F5F7));
        h.steps(60);
        let drawn = drawn_area(&mut h);
        assert!(drawn > 500, "{name} barely drew anything: {drawn} samples");

        let notes = report(h.engine());
        assert!(
            notes.len() < 8,
            "{name} reported {} geometry notes: {:?}",
            notes.len(),
            notes.iter().map(|e| e.describe()).collect::<Vec<_>>()
        );
    }
}

/// The height layout actually gave a row.
///
/// Read from the tree rather than imported from the demo: the demo declares no row height at all
/// any more, because the rows fill the list and divide whatever space it was given. Any test that
/// needs a distance in rows has to measure one.
fn row_height(h: &aurora_tiny_skia::testing::Harness) -> f32 {
    h.located("row-Baseline").flow.height
}
