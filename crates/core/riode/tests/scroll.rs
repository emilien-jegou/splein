// Single responsibility: Scroll behavior contract — viewport, offset, damage,
// replay, input, and virtualization.
//
// Written after human visual validation of the scroll_demo (per AGENTS.md,
// tests lock behavior only once eyes have approved it). Every test drives the
// engine headlessly through public queues and asserts geometry, damage, and
// replay counters — never pixels (damage_parity owns pixel-exactness).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use riode::dsl::*;
use riode::foundation::{Anchor, Color, Point, Size};
use riode::input::{KeyCode, KeyEvent, WheelInfo};
use riode::prelude::{ScrollHandle, Scrollbar};
use riode::reactive::{Cx, ReactiveRuntime, Signal};
use riode_tiny_skia::testing::Harness;

const ROW_H: f32 = 48.0;
const VIEW_W: f32 = 400.0;
const VIEW_H: f32 = 600.0;

fn new_runtime() -> Rc<RefCell<ReactiveRuntime>> {
    Rc::new(RefCell::new(ReactiveRuntime::new()))
}

fn handle_for(rt: &Rc<RefCell<ReactiveRuntime>>, initial: f32) -> ScrollHandle {
    ScrollHandle {
        offset: Signal::new(Rc::clone(rt), initial),
        max: Signal::new(Rc::clone(rt), 0.0),
    }
}

fn fill_rows(n: usize, prefix: &str) -> Vec<Element> {
    (0..n)
        .map(|i| {
            group()
                .key(format!("{prefix}-{i:04}"))
                .width(Size::fill())
                .height(Size::Fixed(ROW_H))
                .fill(Color::hex(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC }))
                .into_element()
        })
        .collect()
}

fn mount_scrollview(
    rt: &Rc<RefCell<ReactiveRuntime>>,
    handle: ScrollHandle,
    rows: Vec<Element>,
    scrollbar: Option<Scrollbar>,
    bounce: Option<bool>,
) -> Harness {
    let root = Cx::new(Rc::clone(rt)).enter(|| {
        ScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            None,
            None,
            None,
            Some(handle),
            None,
            None,
            scrollbar,
            bounce,
            rows,
        )
    });
    Harness::from_runtime(Rc::clone(rt), root, VIEW_W as u32, VIEW_H as u32)
}

/// Rows land at their index pitch, full viewport width; content spans them.
#[test]
#[ntest::timeout(5_000)]
fn scrollview_mounts_rows_at_pitch_with_width() {
    let rt = new_runtime();
    let h = handle_for(&rt, 0.0);
    let mut h = mount_scrollview(&rt, h, fill_rows(20, "srow"), Some(Scrollbar::Hidden), None);
    h.steps(2);
    for i in [0, 3, 19] {
        let at = h
            .at(&format!("srow-{i:04}"))
            .unwrap_or_else(|| panic!("row {i} must be mounted"));
        assert_eq!(
            (at.flow.y, at.flow.height),
            (i as f32 * ROW_H, ROW_H),
            "row {i} must sit at its index pitch"
        );
        assert_eq!(at.flow.width, VIEW_W, "row {i} must span the viewport");
    }
}

/// A scroll tick moves content with no layout, one strip of damage, one shift.
#[test]
#[ntest::timeout(5_000)]
fn scroll_tick_moves_content_without_layout() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_scrollview(&rt, handle, fill_rows(20, "srow"), Some(Scrollbar::Hidden), None);
    h.steps(2);

    offset.set(100.0);
    let report = h.engine().frame();
    assert!(
        !report.diagnostics.layout.laid_out,
        "a scroll tick must never lay out"
    );
    assert_eq!(offset.get(), 100.0);
    h.damage_parity().assert_no_strays(128, "scrolling 100px");

    let damage = h.engine().current_damage();
    assert_eq!(
        damage.rects().len(),
        1,
        "a pure scroll damages exactly the exposed strip"
    );
    let strip = damage.rects()[0];
    assert!(
        (strip.bottom() - VIEW_H).abs() < 3.0 && strip.y > VIEW_H - 140.0,
        "the strip must sit at the revealed edge, got {strip:?}"
    );
    assert_eq!(
        damage.scroll_shifts().len(),
        1,
        "a pure scroll records exactly one shift"
    );
    let shift = damage.scroll_shifts()[0];
    assert_eq!((shift.delta.x, shift.delta.y), (0.0, -100.0));
}

/// Steady scroll ticks replay every row: zero new drawing commands.
#[test]
#[ntest::timeout(5_000)]
fn scroll_tick_replays_all_rows() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_scrollview(&rt, handle, fill_rows(20, "srow"), Some(Scrollbar::Hidden), None);
    h.steps(2);

    offset.set(64.0);
    let report = h.engine().frame();
    assert_eq!(
        report.diagnostics.compile.commands_emitted, 0,
        "clean rows must replay, not re-encode"
    );
    assert!(
        report.diagnostics.compile.cached_chunks > 0,
        "replays must be observed"
    );
    h.damage_parity().assert_no_strays(128, "replayed scroll");
}

/// The post-layout clamp publishes max and pulls the offset into range.
#[test]
#[ntest::timeout(5_000)]
fn scroll_clamp_pulls_into_range() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let mut h = mount_scrollview(&rt, handle, fill_rows(20, "srow"), Some(Scrollbar::Hidden), None);
    h.steps(2);
    // 20 rows * 48 - 600 = 360.
    assert_eq!(max.get(), 360.0, "max must equal content minus viewport");

    offset.set(9999.0);
    h.engine().frame();
    assert_eq!(offset.get(), 360.0, "overshoot clamps to max same-frame");

    offset.set(-50.0);
    h.engine().frame();
    assert_eq!(offset.get(), 0.0, "undershoot clamps to zero same-frame");
}

/// Wheel scrolls the innermost viewport and never chains outward (contain).
#[test]
#[ntest::timeout(5_000)]
fn scroll_wheel_moves_and_contains() {
    let rt = new_runtime();
    let outer = handle_for(&rt, 0.0);
    let inner = handle_for(&rt, 0.0);
    let (outer_off, inner_off) = (outer.offset.clone(), inner.offset.clone());
    let root = Cx::new(Rc::clone(&rt)).enter(|| {
        ScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            None,
            None,
            None,
            Some(outer),
            None,
            None,
            Some(Scrollbar::Hidden),
            None,
            vec![
                group()
                    .width(Size::fill())
                    .height(700.0)
                    .into_element(),
                ScrollView(
                    Size::Fixed(VIEW_W),
                    Size::Fixed(200.0),
                    None,
                    None,
                    None,
                    Some(inner),
                    None,
                    None,
                    Some(Scrollbar::Hidden),
                    None,
                    fill_rows(10, "inrow"),
                ),
                group()
                    .width(Size::fill())
                    .height(300.0)
                    .into_element(),
            ],
        )
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), root, VIEW_W as u32, VIEW_H as u32);
    h.steps(2);

    // Inner viewport sits at outer-content y=700; outer scrolled 300 shows
    // outer-content 300..900, so the inner occupies screen 400..600.
    // Wheel at its center.
    // Each mutation gets exactly one frame and one parity paint: parity
    // repaints the current damage itself, so painting manually as well would
    // apply scroll blits twice (tile repaints are idempotent, moves are not).
    outer_off.set(300.0);
    h.engine().frame();
    h.damage_parity().assert_no_strays(128, "outer reveal");
    h.engine().queue_wheel_event(WheelInfo {
        position: Point::new(200.0, 500.0),
        delta_x: 0.0,
        delta_y: -96.0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    });
    h.engine().frame();
    assert_eq!(inner_off.get(), 96.0, "wheel scrolls the inner viewport");
    assert_eq!(
        outer_off.get(),
        300.0,
        "contain: the outer viewport never chains"
    );
    h.damage_parity().assert_no_strays(128, "nested wheel scroll");
}

/// A drag starting on a clickable-but-undraggable row still scrolls
/// (gesture transfer), and a plain press still clicks.
#[test]
#[ntest::timeout(5_000)]
fn scroll_drag_from_bare_row_moves_click_preserved() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let taps = Rc::new(Cell::new(0i32));
    let taps_in = Rc::clone(&taps);
    // Hard clamp here: bounce overshoot would only noise up the transfer and
    // click assertions (covered separately).
    let root = Cx::new(Rc::clone(&rt)).enter(|| {
        ScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            None,
            None,
            None,
            Some(handle),
            None,
            None,
            Some(Scrollbar::Hidden),
            Some(false),
            vec![
                group()
                    .key("taprow")
                    .width(Size::fill())
                    .height(Size::Fixed(ROW_H))
                    .fill(Color::hex(0x4F46E5))
                    .on_click(move |_| {
                        taps_in.set(taps_in.get() + 1);
                    })
                    .into_element(),
            ]
            .into_iter()
            .chain(fill_rows(19, "srow").into_iter())
            .collect::<Vec<_>>(),
        )
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), root, VIEW_W as u32, VIEW_H as u32);
    h.steps(2);

    // Plain press+release clicks.
    h.engine().queue_pointer_down(200.0, 24.0);
    h.engine().queue_pointer_up(200.0, 24.0);
    h.engine().frame();
    assert_eq!(taps.get(), 1, "a clean press must click");

    // Press, drag down 100px, release: scrolls (clamped at top), does not click.
    h.engine().queue_pointer_down(200.0, 24.0);
    for i in 1..=10 {
        h.engine().queue_pointer_move(200.0, 24.0 + i as f32 * 10.0);
        h.engine().frame();
    }
    assert_eq!(
        offset.get(),
        0.0,
        "dragging down at the top clamps at zero"
    );
    h.engine().queue_pointer_up(200.0, 224.0);
    h.engine().frame();
    assert_eq!(taps.get(), 1, "a drag must not click");
    // Drag up instead: content must follow (asserted before release, so the
    // release fling cannot pollute the measurement).
    h.engine().queue_pointer_down(200.0, 300.0);
    for i in 1..=10 {
        h.engine().queue_pointer_move(200.0, 300.0 - i as f32 * 10.0);
        h.engine().frame();
    }
    assert!(
        (offset.get() - 100.0).abs() < 2.0,
        "dragging up 100px must scroll ~100px, got {}",
        offset.get()
    );
    h.engine().queue_pointer_up(200.0, 200.0);
    h.engine().frame();
    assert_eq!(taps.get(), 1, "the second gesture must not click either");
}

/// Pressing empty scroll area focuses the viewport; keys then scroll it.
#[test]
#[ntest::timeout(5_000)]
fn scroll_keys_after_press() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let mut h = mount_scrollview(&rt, handle, fill_rows(20, "srow"), Some(Scrollbar::Hidden), None);
    h.steps(2);

    h.engine().queue_pointer_down(200.0, 100.0);
    h.engine().queue_pointer_up(200.0, 100.0);
    h.engine().frame();
    let mut key = |code| {
        h.engine().queue_key_event(KeyEvent {
            code,
            shift: false,
            alt: false,
            ctrl: false,
            meta: false,
            repeat: false,
        });
        h.engine().frame();
    };
    use riode::input::KeyCode::*;
    key(Down);
    assert_eq!(offset.get(), 40.0, "Down steps one line");
    key(PageDown);
    assert_eq!(offset.get(), max.get(), "PageDown reaches max");
    key(Home);
    assert_eq!(offset.get(), 0.0, "Home reaches zero");
    key(End);
    assert_eq!(offset.get(), max.get(), "End reaches max");
    key(Up);
    assert_eq!(offset.get(), max.get() - 40.0, "Up steps back one line");
}

/// A virtual list mounts only its window; jumping deep slides it with exact
/// pitches, continuity, and no stragglers.
#[test]
#[ntest::timeout(30_000)]
fn virtual_window_mounts_band_only_and_slides_exact() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let row = RowFn::from(|i: usize| {
        group()
            .width(Size::fill())
            .height(Size::Fixed(40.0))
            .fill(Color::hex(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC }))
            .into_element()
    });
    let root = Cx::new(Rc::clone(&rt)).enter(|| {
        VirtualScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            2000usize,
            40.0,
            row,
            None,
            None,
            None,
            Some(handle),
            None,
            None,
            Some(Scrollbar::Hidden),
            None,
        )
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), root, VIEW_W as u32, VIEW_H as u32);
    h.steps(3);
    assert_eq!(max.get(), 2000.0 * 40.0 - VIEW_H, "virtual max spans all rows");

    let mounted = mounted_vrows(&mut h);
    assert!(
        mounted.len() < 60,
        "the window must stay bounded, got {}",
        mounted.len()
    );
    assert!(
        mounted.iter().all(|(i, _)| *i < 60),
        "at rest the window sits at the top"
    );

    // Jump to row ~500 and let the window catch up (signal -> tracker ->
    // rebuild takes two frames).
    offset.set(20_000.0);
    h.engine().frame();
    h.engine().frame();
    h.steps(2);
    let mounted = mounted_vrows(&mut h);
    assert!(
        mounted.len() < 60,
        "the slid window must stay bounded, got {}",
        mounted.len()
    );
    let mut idx: Vec<usize> = mounted.iter().map(|(i, _)| *i).collect();
    idx.sort_unstable();
    assert!(
        idx.first().copied().unwrap_or(0) >= 480,
        "the window must have slid deep, starts at {:?}",
        idx.first()
    );
    for w in idx.windows(2) {
        assert_eq!(w[1], w[0] + 1, "window indices must be continuous");
    }
    for (i, y) in &mounted {
        assert_eq!(
            *y,
            *i as f32 * 40.0,
            "row {i} must sit at its index pitch"
        );
    }
    assert!(
        h.at("vrow-0").is_none() && h.at("vrow-10").is_none(),
        "rows that left the window must unmount"
    );
    h.damage_parity().assert_no_strays(128, "slid virtual window");
}

/// Virtual clamp: overshoot pulls to total-minus-view.
#[test]
#[ntest::timeout(5_000)]
fn virtual_clamp_pulls_into_range() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let row = RowFn::from(|_: usize| {
        group()
            .width(Size::fill())
            .height(Size::Fixed(40.0))
            .into_element()
    });
    let root = Cx::new(Rc::clone(&rt)).enter(|| {
        VirtualScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            100usize,
            40.0,
            row,
            None,
            None,
            None,
            Some(handle),
            None,
            None,
            Some(Scrollbar::Hidden),
            None,
        )
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), root, VIEW_W as u32, VIEW_H as u32);
    h.steps(3);
    assert_eq!(max.get(), 100.0 * 40.0 - VIEW_H);
    offset.set(999_999.0);
    h.engine().frame();
    assert_eq!(offset.get(), max.get());
}

/// Mounted virtual rows: (index, layout y), via the arena.
fn mounted_vrows(h: &mut Harness) -> Vec<(usize, f32)> {
    h.engine().with_arena(|arena, root| {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if let Some(key) = node.key.clone() {
                let name = format!("{key:?}");
                if let Some(p) = name.find("vrow-") {
                    let digits: String = name[p + 5..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let Ok(i) = digits.parse::<usize>() {
                        out.push((i, node.resolved_rect.y));
                    }
                }
            }
            stack.extend(arena.children(id).iter().copied());
        }
        out
    })
}

/// Finds the scrollbar thumb by its top-right anchor (stable across
/// presence states — a faded thumb has a zero rect). Returns
/// (opacity, visible-presence).
fn thumb_state(h: &mut Harness) -> Option<(f32, bool)> {
    use riode::foundation::LayoutPresence;
    h.engine().with_arena(|arena, root| {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if node.style.anchor == Some(Anchor::TopRight) {
                return Some((
                    node.style.appearance.opacity,
                    node.state.core.presence == LayoutPresence::Present,
                ));
            }
            stack.extend(arena.children(id).iter().copied());
        }
        None
    })
}

fn mount_default_scrollview(rt: &Rc<RefCell<ReactiveRuntime>>, handle: ScrollHandle) -> Harness {
    let root = Cx::new(Rc::clone(rt)).enter(|| {
        ScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            None,
            None,
            None,
            Some(handle),
            None,
            None,
            None,
            None,
            fill_rows(20, "srow"),
        )
    });
    Harness::from_runtime(Rc::clone(rt), root, VIEW_W as u32, VIEW_H as u32)
}

/// The thumb fades after a second idle and returns on activity.
#[test]
#[ntest::timeout(30_000)]
fn scroll_thumb_fades_after_idle_and_returns_on_activity() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    // Fresh mount never scrolled: faded out from the start.
    assert_eq!(
        thumb_state(&mut h),
        Some((0.0, false)),
        "a fresh thumb starts faded and unmounted"
    );

    offset.set(100.0);
    // Ramp needs real time: steps (not bare frames) advance the clock.
    h.steps(15);
    let (op, present) = thumb_state(&mut h).expect("thumb must exist while active");
    assert_eq!(op, 0.5, "activity shows the thumb");
    assert!(present);

    // Mid-fade: strictly between, then fully gone with presence withdrawn.
    // ~70 frames after the last activity lands inside the 1.0-1.3s window.
    h.steps(55);
    let (op, _) = thumb_state(&mut h).expect("thumb present mid-fade");
    assert!(
        op > 0.0 && op < 0.5,
        "fade must pass through partial opacity, got {op}"
    );
    h.steps(40);
    let (op, present) = thumb_state(&mut h).expect("thumb node still mounted");
    assert_eq!(op, 0.0, "idle fades the thumb out");
    assert!(!present, "a faded thumb must not intercept clicks");

    // New activity brings it straight back (ramp needs real time).
    offset.set(200.0);
    h.steps(15);
    assert_eq!(thumb_state(&mut h).map(|(o, _)| o), Some(0.5));
}

/// Clicking the track above/below the thumb pages toward the pressed side.
#[test]
#[ntest::timeout(5_000)]
fn scroll_track_click_pages() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    assert_eq!(max.get(), 360.0);

    // Thumb at the bottom; click above it in the track zone.
    offset.set(360.0);
    h.engine().frame();
    h.engine().queue_pointer_down(388.0, 100.0);
    h.engine().queue_pointer_up(388.0, 100.0);
    h.engine().frame();
    assert_eq!(offset.get(), 0.0, "track click above pages up (clamped)");

    // Thumb back at top; click below it.
    h.engine().queue_pointer_down(388.0, 590.0);
    h.engine().queue_pointer_up(388.0, 590.0);
    h.engine().frame();
    assert_eq!(offset.get(), 360.0, "track click below pages down (clamped)");

    // Clicks outside the track zone never page.
    offset.set(100.0);
    h.engine().frame();
    h.engine().queue_pointer_down(100.0, 100.0);
    h.engine().queue_pointer_up(100.0, 100.0);
    h.engine().frame();
    assert_eq!(offset.get(), 100.0, "content clicks must not page");
}

/// Releasing a fast thumb drag continues with momentum, then settles.
#[test]
#[ntest::timeout(30_000)]
fn scroll_thumb_release_continues_with_momentum() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    // Thumb geometry at offset 100 of max 360: travel = 600-375 = 225,
    // thumb_y = 100/360*225 = 62.5, center ~250. The ramp needs real time,
    // so settle with steps (also flips presence on).
    offset.set(100.0);
    h.steps(15);
    h.engine().queue_pointer_down(392.0, 250.0);
    for i in 1..=5 {
        h.engine().queue_pointer_move(392.0, 250.0 + i as f32 * 20.0);
        h.step();
    }
    h.engine().queue_pointer_up(392.0, 350.0);
    h.step();
    let released = offset.get();
    assert!(
        released > 100.0,
        "thumb drag must move the offset, got {released}"
    );
    // No more input: fling carries it further, then dies before max.
    h.steps(60);
    let settled = offset.get();
    assert!(
        settled > released,
        "momentum must carry past release ({released} -> {settled})"
    );
    assert!(
        settled <= max.get(),
        "momentum must respect the clamp, got {settled}"
    );
    h.steps(30);
    assert_eq!(
        offset.get(),
        settled,
        "the fling must settle, not drift forever"
    );
}

/// Dragging past the top yields with resistance, then springs back on release.
#[test]
#[ntest::timeout(30_000)]
fn scroll_bounce_overshoots_and_returns() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_scrollview(&rt, handle, fill_rows(20, "srow"), Some(Scrollbar::Hidden), None);
    h.steps(3);

    // Drag down 100px from the top: raw -100, rubber-banded to about -55.
    h.engine().queue_pointer_down(200.0, 300.0);
    for i in 1..=10 {
        h.engine().queue_pointer_move(200.0, 300.0 + i as f32 * 10.0);
        h.step();
    }
    let held = offset.get();
    assert!(
        held < -20.0 && held > -90.0,
        "overshoot must yield with resistance, got {held}"
    );
    h.engine().queue_pointer_up(200.0, 400.0);
    h.steps(90);
    assert_eq!(
        offset.get(),
        0.0,
        "release must spring back to the edge exactly"
    );
}

/// With bounce off, the same gesture hard-clamps at zero throughout.
#[test]
#[ntest::timeout(5_000)]
fn scroll_bounce_off_clamps_hard() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_scrollview(
        &rt,
        handle,
        fill_rows(20, "srow"),
        Some(Scrollbar::Hidden),
        Some(false),
    );
    h.steps(3);

    h.engine().queue_pointer_down(200.0, 300.0);
    for i in 1..=10 {
        h.engine().queue_pointer_move(200.0, 300.0 + i as f32 * 10.0);
        h.step();
    }
    assert_eq!(
        offset.get(),
        0.0,
        "hard clamp holds zero while held past the edge"
    );
    h.engine().queue_pointer_up(200.0, 400.0);
    h.steps(10);
    assert_eq!(offset.get(), 0.0);
}

/// A fling into the bottom edge overshoots transiently, then settles at max.
#[test]
#[ntest::timeout(30_000)]
fn scroll_fling_overshoots_and_settles_at_max() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let mut h = mount_scrollview(&rt, handle, fill_rows(20, "srow"), Some(Scrollbar::Hidden), None);
    h.steps(3);
    assert_eq!(max.get(), 360.0);

    // Start at the bottom, drag up fast, release: fling carries past max.
    offset.set(360.0);
    h.engine().frame();
    h.engine().queue_pointer_down(200.0, 300.0);
    for i in 1..=5 {
        h.engine().queue_pointer_move(200.0, 300.0 - i as f32 * 30.0);
        h.step();
    }
    h.engine().queue_pointer_up(200.0, 150.0);
    h.step();
    assert!(
        offset.get() > 360.0,
        "a fast fling must overshoot the edge, got {}",
        offset.get()
    );
    h.steps(120);
    assert_eq!(
        offset.get(),
        360.0,
        "the spring must settle exactly at max"
    );
}

/// Small moves inside the window band rebuild nothing.
#[test]
#[ntest::timeout(30_000)]
fn virtual_window_hysteresis_skips_small_moves() {
    use riode::dsl::RowFn;
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let row = RowFn::from(|_: usize| {
        group()
            .width(Size::fill())
            .height(Size::Fixed(40.0))
            .into_element()
    });
    let root = Cx::new(Rc::clone(&rt)).enter(|| {
        VirtualScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            2000usize,
            40.0,
            row,
            None,
            None,
            None,
            Some(handle),
            None,
            None,
            Some(Scrollbar::Hidden),
            None,
        )
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), root, VIEW_W as u32, VIEW_H as u32);
    h.steps(3);
    let before = mounted_vrows(&mut h);

    // One row (40px) stays inside hysteresis: identical window, no layout.
    offset.set(40.0);
    let report = h.engine().frame();
    assert!(
        !report.diagnostics.layout.laid_out,
        "a one-row move must not rebuild the window"
    );
    assert_eq!(mounted_vrows(&mut h), before);

    // Twenty rows cross the margin: the window slides.
    offset.set(840.0);
    h.engine().frame();
    h.engine().frame();
    let after = mounted_vrows(&mut h);
    assert_ne!(
        after.iter().map(|(i, _)| *i).min(),
        before.iter().map(|(i, _)| *i).min()
    );
}

/// `scroll_to_index` lands a row at the viewport top (clamped).
#[test]
#[ntest::timeout(5_000)]
fn scroll_handle_scroll_to_index() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let row = riode::dsl::RowFn::from(|_: usize| {
        group()
            .width(Size::fill())
            .height(Size::Fixed(40.0))
            .into_element()
    });
    let root = Cx::new(Rc::clone(&rt)).enter(|| {
        VirtualScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(VIEW_H),
            100usize,
            40.0,
            row,
            None,
            None,
            None,
            Some(handle.clone()),
            None,
            None,
            Some(Scrollbar::Hidden),
            None,
        )
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), root, VIEW_W as u32, VIEW_H as u32);
    h.steps(3);
    assert_eq!(max.get(), 100.0 * 40.0 - VIEW_H);

    handle.scroll_to_index(10, 40.0);
    h.engine().frame();
    assert_eq!(offset.get(), 400.0);

    handle.scroll_to_index(5000, 40.0);
    h.engine().frame();
    assert_eq!(offset.get(), max.get(), "deep links clamp to max");
}

/// `scroll_into_view` moves least-scroll: above, below, and inside cases.
#[test]
#[ntest::timeout(5_000)]
fn scroll_handle_scroll_into_view() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    max.set(1000.0);

    // Band above the viewport scrolls up to its top.
    offset.set(500.0);
    handle.scroll_into_view(400.0, 20.0, 600.0);
    assert_eq!(offset.get(), 400.0);

    // Band below scrolls just enough to reveal its bottom.
    handle.scroll_into_view(1100.0, 20.0, 600.0);
    assert_eq!(offset.get(), 520.0);

    // Visible band is a no-op.
    handle.scroll_into_view(600.0, 20.0, 600.0);
    assert_eq!(offset.get(), 520.0);

    // Clamped at both ends.
    handle.scroll_into_view(-100.0, 20.0, 600.0);
    assert_eq!(offset.get(), 0.0);
    handle.scroll_into_view(5000.0, 20.0, 600.0);
    assert_eq!(offset.get(), 1000.0);
}

/// Hovering the viewport shows a faded thumb; leaving resumes the fade.
#[test]
#[ntest::timeout(30_000)]
fn scroll_hover_shows_thumb() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    assert_eq!(
        thumb_state(&mut h),
        Some((0.0, false)),
        "fresh thumb starts hidden"
    );

    // Move in with no press: enter fires, the ramp runs on real time.
    h.engine().queue_pointer_move(200.0, 300.0);
    h.steps(15);
    let (op, present) = thumb_state(&mut h).expect("thumb node exists");
    assert_eq!(op, 0.5, "hover shows the thumb without scrolling");
    assert_eq!(offset.get(), 0.0, "hover alone never moves content");
    assert!(present);

    // Move out: fade resumes and completes.
    h.engine().queue_pointer_move(700.0, 700.0);
    h.steps(100);
    assert_eq!(
        thumb_state(&mut h),
        Some((0.0, false)),
        "leaving fades the thumb out"
    );
}

/// Live motion keeps frames requested; settled viewports go quiet.
#[test]
#[ntest::timeout(30_000)]
fn scroll_motion_pumps_frames() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    assert!(
        !h.engine().has_active_motion(),
        "a settled viewport wants no frames"
    );

    // Fade in progress pumps.
    offset.set(100.0);
    h.engine().frame();
    assert!(
        h.engine().has_active_motion(),
        "fading thumb must keep frames coming"
    );
    h.steps(120);
    assert!(
        !h.engine().has_active_motion(),
        "a faded viewport goes quiet"
    );

    // Fling pumps until it settles.
    offset.set(100.0);
    h.engine().frame();
    h.engine().queue_pointer_down(200.0, 400.0);
    for i in 1..=5 {
        h.engine().queue_pointer_move(200.0, 400.0 - i as f32 * 30.0);
        h.step();
    }
    h.engine().queue_pointer_up(200.0, 250.0);
    h.step();
    assert!(
        h.engine().has_active_motion(),
        "a live fling must keep frames coming"
    );
    h.steps(180);
    assert!(
        !h.engine().has_active_motion(),
        "a settled fling goes quiet"
    );
}

/// Scrolling with a visible thumb leaves no ghost copies: the blit moves
/// overlay pixels that do not move with content, so their destinations must
/// be repainted (overlay repair). Pixel-proved via parity.
#[test]
#[ntest::timeout(30_000)]
fn scroll_overlay_repair_no_ghosts() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    // Default scrollbar: the thumb shows, moves, and would ghost without
    // repair the moment the blit shifts its old pixels. Settle with steps
    // (real time) so the thumb is actually present, then tick with the
    // parity protocol: one unpainted frame + parity paints it exactly once.
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    offset.set(100.0);
    h.steps(15);
    offset.set(220.0);
    h.engine().frame();
    h.damage_parity().assert_no_strays(128, "scroll with visible thumb");
}

/// Touchpad-style wheel past the top rubber-bands, then springs back.
#[test]
#[ntest::timeout(30_000)]
fn scroll_wheel_overshoots_and_returns() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);

    h.engine().queue_wheel_event(WheelInfo {
        position: Point::new(200.0, 300.0),
        delta_x: 0.0,
        delta_y: 100.0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    });
    h.engine().frame();
    let held = offset.get();
    assert!(
        held < -20.0 && held > -90.0,
        "wheel past the top must rubber-band, got {held}"
    );
    h.steps(90);
    assert_eq!(offset.get(), 0.0, "release must spring back to zero");
}

/// Arrow keys hard-clamp at the edges (discrete jumps never overshoot:
/// holding an arrow at the end must sit still, not judder).
#[test]
#[ntest::timeout(5_000)]
fn scroll_keys_clamp_hard() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);

    // Focus the viewport with a press, then push past both ends.
    h.engine().queue_pointer_down(200.0, 100.0);
    h.engine().queue_pointer_up(200.0, 100.0);
    h.engine().frame();
    let send = |h: &mut Harness, code| {
        h.engine().queue_key_event(KeyEvent {
            code,
            shift: false,
            alt: false,
            ctrl: false,
            meta: false,
            repeat: false,
        });
    };
    use riode::input::KeyCode::*;
    send(&mut h, Up);
    h.engine().frame();
    assert_eq!(offset.get(), 0.0, "Up at the top stays put");
    offset.set(max.get());
    h.engine().frame();
    send(&mut h, Down);
    h.engine().frame();
    assert_eq!(offset.get(), max.get(), "Down at the bottom stays put");
}

/// Finds the scrollbar thumb rect in window coordinates, if mounted.
fn thumb_rect(h: &mut Harness) -> Option<(f32, f32, f32, f32)> {
    use riode::foundation::Anchor;
    h.engine().with_arena(|arena, root| {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if node.style.anchor == Some(Anchor::TopRight) {
                let p = arena.node_absolute_point(id);
                return Some((
                    p.x,
                    p.y,
                    node.resolved_rect.width,
                    node.resolved_rect.height,
                ));
            }
            stack.extend(arena.children(id).iter().copied());
        }
        None
    })
}

/// Finds the scrollbar thumb center in window coordinates, if mounted.
fn thumb_center(h: &mut Harness) -> Option<(f32, f32)> {
    thumb_rect(h).map(|(x, y, w, hh)| (x + w * 0.5, y + hh * 0.5))
}

/// Moving between thumb and content never flaps visibility: the thumb
/// counts as viewport chrome for hover.
#[test]
#[ntest::timeout(60_000)]
fn scroll_hover_thumb_content_dance_stays_visible() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    offset.set(100.0);
    h.engine().frame();
    h.engine().frame();
    let (tx, ty) = thumb_center(&mut h).expect("thumb mounted");

    // Dance across the boundary for ~3s; opacity must never touch 0 nor jump.
    let mut prev = 0.5f32;
    for i in 0..20 {
        let (x, y) = if i % 2 == 0 { (tx, ty) } else { (100.0, 300.0) };
        h.engine().queue_pointer_move(x, y);
        h.steps(10);
        let (op, _) = thumb_state(&mut h).expect("thumb node exists");
        assert!(op > 0.0, "hover dance must never hide the thumb (step {i})");
        assert!(
            (op - prev).abs() < 0.15,
            "no opacity pops while dancing (step {i}: {prev} -> {op})"
        );
        prev = op;
    }
}

/// Appearing from hidden ramps over ~0.2s instead of popping.
#[test]
#[ntest::timeout(30_000)]
fn scroll_thumb_fades_in() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    assert_eq!(thumb_state(&mut h), Some((0.0, false)));

    offset.set(100.0);
    // One stepped frame: the ramp advances by real dt (bare frames carry
    // dt=0 and would freeze it at zero).
    h.step();
    let (op, _) = thumb_state(&mut h).expect("thumb node exists");
    assert!(
        op > 0.0 && op < 0.5,
        "first frame ramps, not pops (got {op})"
    );
    h.steps(20);
    assert_eq!(thumb_state(&mut h).map(|(o, _)| o), Some(0.5));
}

/// Overshoot-and-return repaints exactly: the thumb's destinations are
/// covered while the spring brings content home (pixel proof).
#[test]
#[ntest::timeout(60_000)]
fn scroll_overshoot_return_parity() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);

    // Drag down 100px past the top (rubber). Each frame is advanced,
    // framed, and parity-checked with no extra paints in between.
    h.engine().queue_pointer_down(200.0, 300.0);
    for i in 1..=10 {
        h.engine().queue_pointer_move(200.0, 300.0 + i as f32 * 10.0);
        h.engine().advance(1.0 / 60.0);
        h.engine().frame();
        h.damage_parity()
            .assert_no_strays(128, "overshoot outbound");
    }
    assert!(offset.get() < 0.0, "must actually overshoot");
    h.engine().queue_pointer_up(200.0, 400.0);
    for i in 0..60 {
        h.engine().advance(1.0 / 60.0);
        h.engine().frame();
        h.damage_parity()
            .assert_no_strays(128, &format!("spring return frame {i}"));
    }
    assert_eq!(offset.get(), 0.0);
}

/// The thumb travels a lane inset from both viewport edges, clearing
/// rounded-corner clips instead of slicing against them.
#[test]
#[ntest::timeout(30_000)]
fn scroll_thumb_lane_insets_from_edges() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let (offset, max) = (handle.offset.clone(), handle.max.clone());
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    // Show the thumb first (it starts faded with a zero rect).
    offset.set(100.0);
    h.steps(15);
    offset.set(0.0);
    h.steps(15);
    let (_, y, _, _) = thumb_rect(&mut h).expect("thumb mounted");
    assert!(
        (y - 6.0).abs() < 1.0,
        "resting thumb starts at the top inset, got y={y}"
    );
    offset.set(max.get());
    h.steps(15);
    let (_, y, _, hh) = thumb_rect(&mut h).expect("thumb mounted");
    assert!(
        (y + hh - (VIEW_H - 6.0)).abs() < 1.0,
        "bottomed thumb ends at the bottom inset, got y={y} h={hh}"
    );
}

/// A grabbed thumb cannot throw content past half the viewport, however
/// hard the drag — small viewports included.
#[test]
#[ntest::timeout(30_000)]
fn scroll_thumb_overshoot_capped_at_half_viewport() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    // 110-tall viewport: the cap is 55px, well under rubber's ~120 asymptote.
    let root = Cx::new(Rc::clone(&rt)).enter(|| {
        ScrollView(
            Size::Fixed(VIEW_W),
            Size::Fixed(110.0),
            None,
            None,
            None,
            Some(handle),
            None,
            None,
            None,
            None,
            fill_rows(20, "srow"),
        )
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), root, VIEW_W as u32, 110);
    h.steps(3);
    // Show the thumb, then grab it mid-lane: at offset 400 of max 850 the
    // thumb sits at y=6+400/850*74 ~= 41, center ~(392, 53).
    offset.set(400.0);
    h.steps(15);
    h.engine().queue_pointer_down(392.0, 53.0);
    for i in 1..=10 {
        h.engine().queue_pointer_move(392.0, 53.0 - i as f32 * 30.0);
        h.step();
    }
    let held = offset.get();
    assert!(
        held < -20.0 && held >= -55.0 - 1.0,
        "thumb overshoot capped at half the viewport, got {held}"
    );
    h.engine().queue_pointer_up(392.0, -247.0);
    h.steps(120);
    assert_eq!(offset.get(), 0.0, "release springs back exactly");
}

/// A huge frame dt eases motion back in instead of teleporting (and never
/// fast-forwards the fade-in ramp past its curve).
#[test]
#[ntest::timeout(30_000)]
fn scroll_giant_dt_never_teleports() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let offset = handle.offset.clone();
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    // Fade-in ramp after a 5s stall: capped dt advances it a single step.
    offset.set(100.0);
    h.engine().advance(5.0);
    h.engine().frame();
    let (op, _) = thumb_state(&mut h).expect("thumb node exists");
    assert!(
        op > 0.0 && op < 0.5,
        "a stalled frame must not skip the ramp (got {op})"
    );
}

/// Moving offscreen resolves hover outward (the window-switch path).
#[test]
#[ntest::timeout(30_000)]
fn scroll_offscreen_move_clears_hover() {
    let rt = new_runtime();
    let handle = handle_for(&rt, 0.0);
    let mut h = mount_default_scrollview(&rt, handle);
    h.steps(3);
    h.engine().queue_pointer_move(200.0, 300.0);
    h.steps(15);
    assert_eq!(thumb_state(&mut h).map(|(o, _)| o), Some(0.5));
    // Off-window point: hit chain empties, leave fires, fade completes.
    h.engine().queue_pointer_move(-10.0, -10.0);
    h.steps(100);
    assert_eq!(
        thumb_state(&mut h),
        Some((0.0, false)),
        "offscreen move must fade the thumb out"
    );
}
