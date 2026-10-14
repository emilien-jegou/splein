// Single responsibility: Deep reactive chains evaluate iteratively without
// overflowing the call stack, and diamonds evaluate glitch-free.
//
// Regression coverage for the P0 conversion of `Signal::write` /
// `Derived::get` from call-stack recursion to an explicit work stack: a
// 20,000-node linear chain exceeds what nested `get()` frames can survive
// on a default test-thread stack.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use riode::reactive::{batch, Derived, ReactiveRuntime, Signal};

fn new_runtime() -> Rc<RefCell<ReactiveRuntime>> {
    Rc::new(RefCell::new(ReactiveRuntime::new()))
}

/// A 20k linear chain must set and pull correctly: depth that nested
/// `Derived::get -> compute -> get` frames cannot survive.
///
/// Runs on an explicitly large stack because dropping 20k refcounted links
/// nests one destructor per link — inherent to any owned chain, unrelated
/// to evaluation. What this test proves is that *frames* (set/get) stay
/// iterative and correct at depths that used to abort the process mid-frame.
#[test]
#[ntest::timeout(5_000)]
fn deep_linear_chain_set_and_pull_is_correct() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            const N: usize = 20_000;
            let rt = new_runtime();
            let root = Signal::new(Rc::clone(&rt), 0u64);
            let r = root.clone();
            let mut leaf = Derived::new(Rc::clone(&rt), move || r.get().wrapping_add(1));
            for _ in 2..N {
                let prev = leaf.clone();
                leaf = Derived::new(Rc::clone(&rt), move || prev.get().wrapping_add(1));
            }

            root.set(1);
            assert_eq!(leaf.get(), 1 + (N as u64 - 1));

            root.set(41);
            assert_eq!(leaf.get(), 41 + (N as u64 - 1));
        })
        .expect("spawn deep-chain thread")
        .join()
        .expect("deep-chain thread completed without overflowing");
}

/// Frames stay flat regardless of chain depth: a 3k chain builds, sets,
/// and pulls on a 256 KiB stack.
///
/// Recursive marking or pull-evaluation needs ~a frame per link and aborts
/// here. The handles are deliberately forgotten instead of dropped: teardown
/// of an owned link chain nests one destructor per link (inherent to any
/// refcounted linked structure), which is a separate, documented property
/// from frame evaluation. Leaking a test-local runtime is harmless.
#[test]
#[ntest::timeout(5_000)]
fn deep_chain_frames_do_not_grow_the_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            const N: usize = 3_000;
            let rt = new_runtime();
            let root = Signal::new(Rc::clone(&rt), 0u64);
            let r = root.clone();
            let mut leaf = Derived::new(Rc::clone(&rt), move || r.get().wrapping_add(1));
            for _ in 2..N {
                let prev = leaf.clone();
                leaf = Derived::new(Rc::clone(&rt), move || prev.get().wrapping_add(1));
            }

            root.set(7);
            assert_eq!(leaf.get(), 7 + (N as u64 - 1));

            std::mem::forget((rt, root, leaf));
        })
        .expect("spawn small-stack thread")
        .join()
        .expect("small-stack frames completed without overflowing");
}

/// A diamond must evaluate its terminal exactly once per root change, even
/// with eager dependency-first recomputation on write.
#[test]
#[ntest::timeout(5_000)]
fn diamond_terminal_evaluates_exactly_once_per_change() {
    let rt = new_runtime();
    let evals = Rc::new(Cell::new(0usize));
    let evals_in = Rc::clone(&evals);

    let root = Signal::new(Rc::clone(&rt), 10u64);
    let ra = root.clone();
    let a = Derived::new(Rc::clone(&rt), move || ra.get().wrapping_mul(2));
    let rb = root.clone();
    let b = Derived::new(Rc::clone(&rt), move || rb.get().wrapping_add(5));
    let (ca, cb) = (a.clone(), b.clone());
    let d = Derived::new(Rc::clone(&rt), move || {
        evals_in.set(evals_in.get() + 1);
        ca.get().wrapping_add(cb.get())
    });

    assert_eq!(d.get(), (10 * 2) + (10 + 5));
    assert_eq!(evals.get(), 1);

    batch(&rt, || root.set(20));
    assert_eq!(d.get(), (20 * 2) + (20 + 5));
    assert_eq!(evals.get(), 2, "diamond terminal re-evaluated redundantly");

    root.set(30);
    assert_eq!(d.get(), (30 * 2) + (30 + 5));
    assert_eq!(evals.get(), 3, "diamond terminal re-evaluated redundantly");
}
