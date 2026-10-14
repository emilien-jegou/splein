// Single responsibility: Stable component identity — remounts recall, siblings stay apart.
use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{group, IntoElement};
use riode::prelude::*;
use riode::reactive::{Cx, ReactiveRuntime, Signal};

thread_local! {
    static PROBE: RefCell<Option<Signal<u32>>> = const { RefCell::new(None) };
    static PROBE_B: RefCell<Option<Signal<u32>>> = const { RefCell::new(None) };
}

fn probe() -> Signal<u32> {
    PROBE.with(|p| p.borrow().clone().expect("component body must have run"))
}

fn probe_b() -> Signal<u32> {
    PROBE_B.with(|p| p.borrow().clone().expect("component body must have run"))
}

#[component]
fn Stateful() -> impl IntoElement {
    let n = use_signal(0u32);
    PROBE.with(|p| *p.borrow_mut() = Some(n.clone()));
    group()
        .width(10.0 + n.get() as f32)
        .height(10.0)
        .into_element()
}

#[component]
fn Twin(first: bool) -> impl IntoElement {
    let n = use_signal(0u32);
    if first {
        PROBE.with(|p| *p.borrow_mut() = Some(n.clone()));
    } else {
        PROBE_B.with(|p| *p.borrow_mut() = Some(n.clone()));
    }
    group().width(10.0).height(10.0).into_element()
}

/// Remounting a component recalls its ordinal hooks: the scope is keyed by
/// `module_path!()::Name`, not by a throwaway counter.
#[test]
#[ntest::timeout(5_000)]
fn remount_recalls_ordinal_signal() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));

    cx.enter_keyed("test-root", || Stateful());
    probe().set(42);

    // Re-run the body (what a remount does): the signal must be recalled.
    cx.enter_keyed("test-root", || Stateful());
    assert_eq!(probe().get(), 42, "remount must recall the ordinal hook");
}

/// Two sibling instances of one component never share slots, and each keeps
/// its own state across a joint rebuild.
#[test]
#[ntest::timeout(5_000)]
fn same_name_siblings_hold_independent_state() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));

    cx.enter_keyed("parent", || {
        Twin(true);
        Twin(false);
    });
    probe().set(1);
    probe_b().set(2);

    cx.enter_keyed("parent", || {
        Twin(true);
        Twin(false);
    });
    assert_eq!(probe().get(), 1, "first sibling keeps its own value");
    assert_eq!(probe_b().get(), 2, "second sibling keeps its own value");
}

/// A stable component root cascades stability to ordinal hooks below it.
///
/// Engine-level remount (the real-world path through `set_root`): rebuild the
/// element by re-running the body, remount, and the signal must survive.
#[test]
#[ntest::timeout(5_000)]
fn engine_remount_preserves_component_state() {
    use riode::runtime::Engine;
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));

    let root = cx.enter_keyed("test-root", || Stateful());
    probe().set(9);
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), root, 300, 300);

    engine.mount(cx.enter_keyed("test-root", || Stateful()));
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(
        probe().get(),
        9,
        "remount through the engine must recall hooks"
    );
}
