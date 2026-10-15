// Single responsibility: use_effect lifecycle — mount run, teardown-before-setup on
// rebuild, re-run on a tracked read, and post-layout registration that does not grow.
use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{group, IntoElement};
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this test composes repr-builders directly
use riode::reactive::{run_post_layout, Cx, ReactiveRuntime, Signal};
use riode_tiny_skia::testing::Harness;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static PROBE: RefCell<Option<Signal<u32>>> = const { RefCell::new(None) };
    static MIRROR_SRC: RefCell<Option<Signal<u32>>> = const { RefCell::new(None) };
    static MIRROR_OUT: RefCell<Option<Signal<u32>>> = const { RefCell::new(None) };
}

fn log() -> Vec<String> {
    LOG.with(|l| l.borrow().clone())
}

fn clear() {
    LOG.with(|l| l.borrow_mut().clear());
}

fn probe() -> Signal<u32> {
    PROBE.with(|p| p.borrow().clone().expect("component body must have run"))
}

fn note(msg: impl Into<String>) {
    LOG.with(|l| l.borrow_mut().push(msg.into()));
}

/// Effect with nothing to read: runs on mount, and again (teardown-first) on rebuild.
#[component]
fn Static() -> impl IntoElement {
    use_effect(move || {
        note("setup");
        move || note("teardown")
    });
    group().width(10.0).height(10.0).into_element()
}

/// Effect whose body tracks a signal: the write re-runs it, teardown-first.
#[component]
fn Reactive() -> impl IntoElement {
    let n = use_signal(0u32);
    PROBE.with(|p| *p.borrow_mut() = Some(n.clone()));
    use_effect(move || {
        let v = n.get();
        note(format!("setup:{v}"));
        move || note(format!("teardown:{v}"))
    });
    group().width(10.0).height(10.0).into_element()
}

/// A row that mirrors one signal into another every frame, the `motion_reorder` shape.
/// The registration and its unregister ride the same teardown, so a rebuild cannot stack.
#[component]
fn Mirrored() -> impl IntoElement {
    let src = use_signal(1u32);
    let out = use_signal(0u32);
    MIRROR_SRC.with(|p| *p.borrow_mut() = Some(src.clone()));
    MIRROR_OUT.with(|p| *p.borrow_mut() = Some(out.clone()));
    use_effect_post_layout(move |_dt| {
        out.set_if_changed(src.get());
    });
    group().width(10.0).height(10.0).into_element()
}

fn src() -> Signal<u32> {
    MIRROR_SRC.with(|p| p.borrow().clone().expect("Mirrored body must have run"))
}

fn out() -> Signal<u32> {
    MIRROR_OUT.with(|p| p.borrow().clone().expect("Mirrored body must have run"))
}

/// The body runs exactly once on the first build of a stable scope.
#[test]
#[ntest::timeout(5_000)]
fn effect_runs_once_on_mount() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    clear();

    cx.enter_keyed("test-root", || Static());

    assert_eq!(log(), ["setup"], "mount must run the body exactly once");
}

/// A rebuild recalls the cell, tears the previous run down, then runs the new body:
/// explicit cleanup + setup, not a second registration stacked on the first.
#[test]
#[ntest::timeout(5_000)]
fn rebuild_runs_teardown_before_next_setup() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    clear();

    cx.enter_keyed("test-root", || Static());
    cx.enter_keyed("test-root", || Static());

    assert_eq!(
        log(),
        ["setup", "teardown", "setup"],
        "a rebuild must run the old teardown before the new setup"
    );
}

/// Writing a signal the body read re-runs the effect, teardown-first.
#[test]
#[ntest::timeout(5_000)]
fn tracked_read_change_reruns_effect() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    clear();

    cx.enter_keyed("test-root", || Reactive());
    assert_eq!(log(), ["setup:0"], "mount runs the body at the initial value");

    probe().set(1);
    assert_eq!(
        log(),
        ["setup:0", "teardown:0", "setup:1"],
        "a tracked read change must re-run the effect, tearing down first"
    );
}

/// Two sibling instances run their own bodies and never share a cell.
#[test]
#[ntest::timeout(5_000)]
fn sibling_instances_run_independently() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    clear();

    cx.enter_keyed("parent", || {
        Static();
        Static();
    });

    assert_eq!(
        log(),
        ["setup", "setup"],
        "each instance mounts its own effect exactly once"
    );
}

/// The registry is append-only without a teardown; a rebuild must not grow it by one.
/// This is the `motion_reorder` F9 leak in miniature: 24 drags took the list from 10 to 50.
#[test]
#[ntest::timeout(5_000)]
fn post_layout_registration_does_not_grow_across_rebuilds() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));

    cx.enter_keyed("test-root", || Mirrored());
    assert_eq!(
        runtime.borrow().post_layout_count(),
        1,
        "the first build registers exactly one post-layout callback"
    );

    for rebuild in 1..=3 {
        cx.enter_keyed("test-root", || Mirrored());
        assert_eq!(
            runtime.borrow().post_layout_count(),
            1,
            "rebuild #{rebuild} must unregister the previous callback, not stack a new one"
        );
    }
}

/// The registered frame actually runs in the post-layout slot and mirrors the source.
#[test]
#[ntest::timeout(5_000)]
fn post_layout_frame_runs_and_mirrors_the_source() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));

    cx.enter_keyed("test-root", || Mirrored());
    assert_eq!(out().get(), 0, "nothing has run yet");

    run_post_layout(&runtime, 1.0 / 60.0);
    assert_eq!(out().get(), 1, "the frame must mirror the source once it runs");

    src().set(7);
    run_post_layout(&runtime, 1.0 / 60.0);
    assert_eq!(
        out().get(),
        7,
        "a rebuilt source must be picked up by the next frame"
    );
}

/// A handle takes a callback back out; unregistering twice is a no-op.
#[test]
#[ntest::timeout(5_000)]
fn unregister_post_layout_removes_the_callback_and_is_idempotent() {
    let runtime = ReactiveRuntime::new();
    let id = runtime.register_post_layout(|_dt| {});

    assert_eq!(runtime.post_layout_count(), 1);
    assert!(runtime.unregister_post_layout(id), "the first remove finds it");
    assert_eq!(runtime.post_layout_count(), 0);
    assert!(
        !runtime.unregister_post_layout(id),
        "a second remove is a no-op, so a teardown racing a sweep cannot double-fire"
    );
}

/// A keyed row that leaves the list must run its teardown: React semantics — the unmounted
/// row takes its state, and with it its post-layout callback. Without the unmount sweep the
/// registry only ever grows, which is the leak `use_effect_post_layout` exists to close.
#[component]
fn MirrorRow(_index: usize, label: String) -> impl IntoElement {
    let src = use_signal(1u32);
    let out = use_signal(0u32);
    use_effect_post_layout(move |_dt| {
        out.set_if_changed(src.get());
    });
    group()
        .key(format!("row-{label}"))
        .width(10.0)
        .height(10.0)
        .into_element()
}

#[test]
#[ntest::timeout(5_000)]
fn a_removed_keyed_row_unregisters_its_post_layout_callback() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let labels = Signal::new(
        Rc::clone(&rt),
        vec!["a".to_string(), "b".to_string()],
    );
    let list = || {
        group()
            .width(Size::fill())
            .height(Size::fill())
            .direction(Direction::Vertical)
            .children_labels_keyed(labels.clone(), |s: &String| s.clone(), MirrorRow)
            .into_element()
    };
    let mut h = Harness::from_runtime(Rc::clone(&rt), Cx::new(Rc::clone(&rt)).enter(list), 300, 400);
    h.steps(4);

    assert_eq!(
        rt.borrow().post_layout_count(),
        2,
        "two live rows register two post-layout callbacks"
    );

    labels.set(vec!["b".to_string()]);
    h.steps(8);

    assert_eq!(
        rt.borrow().post_layout_count(),
        1,
        "the removed row must unregister its callback, not leave it mirroring a dead node"
    );

    labels.set(vec!["b".to_string(), "a".to_string()]);
    h.steps(8);

    assert_eq!(
        rt.borrow().post_layout_count(),
        2,
        "the re-added row mounts fresh and registers exactly one callback again"
    );
}
