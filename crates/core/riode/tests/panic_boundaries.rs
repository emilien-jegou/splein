// Single responsibility: Panic boundaries — mount, remount, and frame never propagate.
use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{component_scope, group, IntoElement};
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this test composes repr-builders directly
use riode::reactive::ReactiveRuntime;
use riode::runtime::Engine;

fn engine() -> (Rc<RefCell<ReactiveRuntime>>, Engine) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let root = cx.enter(|| group().width(400.0).height(400.0).into_element());
    let engine = Engine::with_runtime(Rc::clone(&runtime), root, 400, 400);
    (runtime, engine)
}

#[test]
#[ntest::timeout(5_000)]
fn a_panicking_component_body_mounts_a_placeholder() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let el: riode::dsl::Element = cx.enter(|| {
        component_scope(|| -> riode::dsl::Element {
            panic!("boom in body");
        })
    });
    // Mounted *something* layout-safe rather than propagating.
    assert_eq!(el.key(), None);
}

#[test]
#[ntest::timeout(5_000)]
fn a_panicking_remount_keeps_the_old_root() {
    let (_rt, mut engine) = engine();
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(engine.last_frame_error().is_none());

    // The children builder panics during reconcile: set_root catches it,
    // keeps the old root, and records the error — nothing propagates.
    engine.mount(group().children_of(|| -> Vec<riode::dsl::Element> {
        panic!("boom in rebuild");
    }));
    let msg = engine
        .last_frame_error()
        .expect("remount must record the panic")
        .to_string();
    assert!(msg.contains("boom in rebuild"), "unexpected message: {msg}");

    // The kept root still frames cleanly, which clears the recorded error.
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(engine.last_frame_error().is_none());
}

#[test]
#[ntest::timeout(5_000)]
fn a_panicking_binding_is_recorded_not_propagated() {
    let (runtime, mut engine) = engine();
    let cx = Cx::new(Rc::clone(&runtime));
    // Mount evaluates the binding eagerly and the remount boundary records it.
    engine.mount(cx.enter(|| {
        group()
            .width(400.0)
            .height(400.0)
            .x(riode::reactive::computed(|| -> f32 {
                panic!("boom in binding")
            }))
            .into_element()
    }));
    let msg = engine
        .last_frame_error()
        .expect("mount must record the panic")
        .to_string();
    assert!(msg.contains("boom in binding"), "unexpected message: {msg}");

    // Old root survives and frames cleanly afterwards.
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(engine.last_frame_error().is_none());
}

#[test]
#[ntest::timeout(5_000)]
fn a_panic_inside_frame_passes_is_caught_and_reported() {
    use std::cell::Cell;
    let (runtime, mut engine) = engine();
    // Panics on the *second* evaluation: mount (first eval) succeeds, the
    // frame pass (re-eval of the dirty binding) unwinds into the boundary.
    let calls = Rc::new(Cell::new(0usize));
    let probe = Rc::clone(&calls);
    let cx = Cx::new(Rc::clone(&runtime));
    let tick = riode::reactive::Signal::new(Rc::clone(&runtime), 0u32);
    let tick_in = tick.clone();
    engine.mount(cx.enter(|| {
        group()
            .width(400.0)
            .height(400.0)
            .x(riode::reactive::computed(move || -> f32 {
                tick_in.get();
                let n = probe.get();
                probe.set(n + 1);
                if n >= 1 {
                    panic!("boom in frame");
                }
                0.0
            }))
            .into_element()
    }));
    assert!(engine.last_frame_error().is_none(), "mount must succeed");

    // Dirty the binding so the frame pass re-evaluates it (and unwinds).
    tick.set(1);
    engine.advance(1.0 / 60.0);
    engine.frame(); // must return, not unwind
    let msg = engine
        .last_frame_error()
        .expect("frame must record the panic")
        .to_string();
    assert!(msg.contains("boom in frame"), "unexpected message: {msg}");
}
