// Single responsibility: Regression tests for `Copy` signal handle safety.
//
// Handles are `Copy` ids + epochs resolved ambiently. Each test pins one
// failure mode to a loud, located panic (or a compile-time property):
// cross-engine writes, stale slots, missing ambient, thread escape, unwound
// scopes, and mixed-table reads. None may corrupt silently.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::prelude::*;
use aurora::reactive::{Cx, ReactiveRuntime, Signal};

fn assert_copy<T: Copy>() {}

/// The guarantee itself: handles are `Copy` for every payload, including heap types.
#[test]
fn signals_are_copy_for_all_payloads() {
    assert_copy::<Signal<u32>>();
    assert_copy::<Signal<bool>>();
    assert_copy::<Signal<String>>();
    assert_copy::<Signal<Vec<String>>>();
    assert_copy::<Signal<Option<(u32, String)>>>();
}

/// Copies are free: no `.clone()` needed to share a handle across closures.
#[test]
#[ntest::timeout(5_000)]
fn zero_clone_sharing() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let flag = Signal::new(Rc::clone(&rt), false);
    // Each closure copies the handle implicitly; no clone lines exist.
    let f1 = move || flag.set(true);
    let f2 = move || flag.set(false);
    let f3 = move || flag.get();
    let cx = Cx::new(Rc::clone(&rt));
    cx.enter(|| {
        f1();
        assert!(f3());
        f2();
        assert!(!f3());
    });
    // The original is still usable: copies never moved it.
    let _ = flag;
}

/// Cross-engine use panics naming both epochs instead of writing the wrong graph.
#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "wrong engine")]
fn cross_engine_write_panics() {
    let rt_a = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let rt_b = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let sig_a = Signal::new(Rc::clone(&rt_a), 0u32);
    // Drive B's region, then touch A's handle inside it.
    let _guard = aurora::reactive::ambient_guard(&rt_b);
    sig_a.set(1);
}

/// A stale handle (slot disposed and reused) panics on next access.
#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "stale SignalId")]
fn stale_handle_panics_after_slot_reuse() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let stale = Signal::new(Rc::clone(&rt), 111u32);
    rt.borrow_mut().dispose_signal(stale.id());
    // Reuse the slot with a new occupant.
    let _fresh = Signal::new(Rc::clone(&rt), 222u32);
    // The old handle must not read the new occupant's value.
    let _ = stale.get();
}

/// A double dispose panics instead of freeing a live successor slot.
#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "stale SignalId")]
fn double_dispose_panics() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let sig = Signal::new(Rc::clone(&rt), 0u32);
    rt.borrow_mut().dispose_signal(sig.id());
    rt.borrow_mut().dispose_signal(sig.id());
}

/// A panic inside a guarded region pops the ambient: later access resolves
/// to the outer record, not the dead region.
#[test]
#[ntest::timeout(5_000)]
fn unwind_restores_ambient() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let sig = Signal::new(Rc::clone(&rt), 0u32);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = aurora::reactive::ambient_guard(&rt);
        panic!("boom");
    }));
    assert!(result.is_err());
    // LAST still records this runtime (guard recorded on push): external access works.
    sig.set(5);
    assert_eq!(sig.get(), 5);
}

/// Using a handle on a thread with no runtime panics deterministically.
///
/// Handles are `Send` (no `Rc` inside to forbid it) but never `Sync`, and
/// thread-locals are per-thread: the spawned thread has neither stack nor
/// record, so resolution fails loudly instead of touching another engine.
#[test]
#[ntest::timeout(5_000)]
fn cross_thread_use_panics() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let sig = Signal::new(Rc::clone(&rt), 0u32);
    let handle = std::thread::spawn(move || {
        let _ = sig.get();
    });
    assert!(
        handle.join().is_err(),
        "cross-thread signal use must panic, never resolve"
    );
}

/// A handle used after its runtime is dropped panics instead of aliasing.
#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "dropped")]
fn dropped_runtime_panics() {
    let leaked = {
        let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
        Signal::new(Rc::clone(&rt), 0u32)
        // `rt` drops here; nothing keeps it alive (Copy handle holds no Rc).
    };
    // Seed another runtime so the last-active record points elsewhere.
    let _other = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let _ = leaked.get();
}

/// External access (outside frames/dispatch) works for the single-engine case
/// via the last-active record — no guard ceremony in tests and setup code.
#[test]
#[ntest::timeout(5_000)]
fn external_access_needs_no_guard_single_engine() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let flag = Signal::new(Rc::clone(&rt), true);
    // No guard pushed anywhere: creation recorded this runtime.
    flag.set(false);
    assert!(!flag.get());
    flag.update(|v| *v = !*v);
    assert!(flag.get());
}
