// Single responsibility: Call-site-keyed hooks survive conditional execution,
// with located panics for duplicate keys, loops, and type changes.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{group, Element, IntoElement};
use riode::foundation::Size;
use riode::prelude::*;
use riode::reactive::{Cx, ReactiveRuntime, Signal};
use riode_tiny_skia::testing::Harness;

fn mount(root: impl FnOnce() -> Element, rt: Rc<RefCell<ReactiveRuntime>>) -> Harness {
    let mut h = Harness::from_runtime(Rc::clone(&rt), Cx::new(rt).enter(root), 300, 400);
    h.steps(4);
    h
}

/// Explicit keys recall across builds even when a middle hook is skipped.
#[test]
#[ntest::timeout(5_000)]
fn keyed_hooks_survive_a_skipped_conditional() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&rt));

    cx.enter_keyed("panel", || {
        let a = use_signal!("a", 0u32);
        a.set(1);
        let b = use_signal!("b", 0u32);
        b.set(2);
        let c = use_signal!("c", 0u32);
        c.set(3);
    });
    // Rebuild with `b` skipped: `c` must keep its value, not rebind to `b`'s slot.
    cx.enter_keyed("panel", || {
        let a = use_signal!("a", 0u32);
        assert_eq!(a.get(), 1, "a must survive the rebuild");
        let c = use_signal!("c", 99u32);
        assert_eq!(c.get(), 3, "c must keep its value when b is skipped");
    });
}

/// Explicit keys recall across rebuilds.
///
/// The same key re-entered recalls the same slot — which is exactly what a
/// component body re-running is. (The old `file:line`-derived arm is gone:
/// moving a line must never reset state.)
fn build_named_bump() -> u32 {
    let v = use_signal!("named-bump", 10u32);
    let cur = v.get();
    v.set(cur + 1);
    cur
}

#[test]
#[ntest::timeout(5_000)]
fn named_keys_recall_across_rebuilds() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&rt));

    assert_eq!(cx.enter_keyed("auto", build_named_bump), 10);
    assert_eq!(
        cx.enter_keyed("auto", build_named_bump),
        11,
        "same key re-entered must recall its value"
    );
}

/// Two identical keys in one build panic naming the key.
#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "duplicate hook key")]
fn duplicate_key_panics() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&rt));
    cx.enter_keyed("dup", || {
        let _a = use_signal!("same", 0u32);
        let _b = use_signal!("same", 0u32);
    });
}

/// A hook in a loop shares one call site across iterations and must panic.
#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "duplicate hook key")]
fn hook_in_loop_panics() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&rt));
    cx.enter_keyed("loop", || {
        for _ in 0..3 {
            let _s = use_signal!("looped", 0u32);
        }
    });
}

/// A changed type on an existing key panics naming both types.
#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "changed type")]
fn type_change_panics_with_both_types() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&rt));
    cx.enter_keyed("typed", || {
        let _v = use_signal!("t", 0u32);
    });
    cx.enter_keyed("typed", || {
        let _v = use_signal!("t", "now a string");
    });
}

/// Panel body with a conditional keyed hook, built under a stable identity.
/// Height tracks `c`, so the layout literally reports which value `c` holds.
///
/// No writes happen during the build itself: writing a signal the same body
/// also binds (even via a fresh `map`) resubscribes the list rebuild and loops.
/// Clicks move `c` off zero instead, and the element key flips with the flag so
/// the node remounts and adopts the rebuilt bindings. After the flip, recalled /
/// rebound-to-`b`'s-slot / never-rebuilt read 62 / 40 / missing-key.
/// No vacuous passes.
fn cond_panel(flag: Signal<bool>) -> Element {
    Cx::expect_current().enter_keyed("cond-panel", || {
        let _a = use_signal!("a", 0u32);
        if flag.get() {
            let _b = use_signal!("b", 0u32);
        }
        let c = use_signal!("c", 0u32);
        let cc = c.clone();
        group()
            .key(if flag.get() {
                "cond-panel-on"
            } else {
                "cond-panel-off"
            })
            .width(Size::fill())
            .height(c.map(|v| 40.0 + v as f32))
            .on_click(move |_| cc.set(cc.get() + 11))
            .into_element()
    })
}

fn cond_root(flag: Signal<bool>) -> Element {
    group()
        .width(Size::fill())
        .height(Size::fill())
        .children_of(move || {
            flag.get();
            vec![cond_panel(flag.clone())]
        })
        .into_element()
}

/// End to end: flipping the flag rebuilds the panel without `b`, and `c` holds.
#[test]
#[ntest::timeout(5_000)]
fn conditional_keyed_hook_survives_rebuild_in_engine() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let flag = Signal::new(Rc::clone(&rt), true);
    let mut h = mount(|| cond_root(flag.clone()), rt);

    let before = h.located("cond-panel-on").flow.height;
    assert!(
        (before - 40.0).abs() < 0.5,
        "c must start at 0, height {before}"
    );

    // Move `c` off zero through the click handler (never during a build).
    h.click("cond-panel-on");
    h.click("cond-panel-on");
    h.steps(4);
    assert!((h.located("cond-panel-on").flow.height - 62.0).abs() < 0.5);

    flag.set(false);
    h.steps(8);
    let after = h.located("cond-panel-off").flow.height;
    assert!(
        (after - 62.0).abs() < 0.5,
        "c must still be 22 after b is skipped (got {after}: 40 means slot rebind, missing key means no rebuild)"
    );
}

/// Control: the ordinal variant silently rebinds `c` to `b`'s old slot.
fn ordinal_panel(flag: Signal<bool>) -> Element {
    Cx::expect_current().enter_keyed("ord-panel", || {
        let _a = use_signal(0u32);
        if flag.get() {
            let _b = use_signal(0u32);
        }
        let c = use_signal(0u32);
        let cc = c.clone();
        group()
            .key(if flag.get() {
                "ord-panel-on"
            } else {
                "ord-panel-off"
            })
            .width(Size::fill())
            .height(c.map(|v| 40.0 + v as f32))
            .on_click(move |_| cc.set(cc.get() + 11))
            .into_element()
    })
}

fn ordinal_root(flag: Signal<bool>) -> Element {
    group()
        .width(Size::fill())
        .height(Size::fill())
        .children_of(move || {
            flag.get();
            vec![ordinal_panel(flag.clone())]
        })
        .into_element()
}

#[test]
#[ntest::timeout(5_000)]
fn ordinal_conditional_rebinds_silently() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let flag = Signal::new(Rc::clone(&rt), true);
    let mut h = mount(|| ordinal_root(flag.clone()), rt);
    assert!((h.located("ord-panel-on").flow.height - 40.0).abs() < 0.5);

    h.click("ord-panel-on");
    h.click("ord-panel-on");
    h.steps(4);
    assert!((h.located("ord-panel-on").flow.height - 62.0).abs() < 0.5);

    // `c` falls back to slot 1 — `b`'s untouched signal holding 0 — with no error.
    flag.set(false);
    h.steps(8);
    let ord_after = h.located("ord-panel-off").flow.height;
    assert!(
        (ord_after - 40.0).abs() < 0.5,
        "documents the ordinal hazard keyed hooks replace: c reads b's stale 0, got {ord_after}"
    );
}
