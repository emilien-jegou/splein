// Single responsibility: Ambient runtime resolution for `Copy` signal handles.
//
// A `Copy` handle carries an id and an epoch, not an `Rc`: nothing keeps its
// runtime alive and nothing says which runtime it belongs to. This module is
// both answers. The stack says which runtime is *current* (nested engines push
// and pop around driven regions); the last-active record says which runtime to
// use *outside* driven regions (tests asserting, setup code writing), as long
// as its epoch matches the handle. Every mismatch panics loudly — resolution
// here never guesses silently.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::reactive::runtime::ReactiveRuntime;

thread_local! {
    /// Driven regions, innermost last. Pushed by [`AmbientGuard`], popped on drop.
    static STACK: RefCell<Vec<Rc<RefCell<ReactiveRuntime>>>> = const { RefCell::new(Vec::new()) };
    /// Most recently pushed or created runtime, for access outside driven regions.
    ///
    /// Single-engine programs (and tests) never notice it: creation records it,
    /// every frame re-records it, and external reads/writes just work. With two
    /// live engines the wrong-engine case fails loudly (see [`resolve`]).
    static LAST: RefCell<Option<(u64, Weak<RefCell<ReactiveRuntime>>)>> =
        const { RefCell::new(None) };
}

/// RAII ambient scope: pushes the runtime on creation, pops on drop.
///
/// Dropping runs on unwind too, so a panicking component body cannot leave a
/// stale runtime installed for whatever runs next. Nesting the same runtime
/// (frame guard around dispatch guard) is harmless: resolution only reads the top.
#[must_use = "dropping the guard immediately pops the runtime it just pushed"]
pub struct AmbientGuard {
    _private: (),
}

impl Drop for AmbientGuard {
    fn drop(&mut self) {
        STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// Pushes a runtime as ambient for the guard's lifetime. Also records it as
/// last-active, so external access after the region keeps working.
pub fn guard(runtime: &Rc<RefCell<ReactiveRuntime>>) -> AmbientGuard {
    let epoch = runtime.borrow().epoch;
    STACK.with(|stack| stack.borrow_mut().push(Rc::clone(runtime)));
    LAST.with(|last| {
        *last.borrow_mut() = Some((epoch, Rc::downgrade(runtime)));
    });
    AmbientGuard { _private: () }
}

/// Records a freshly created runtime as last-active, so handles allocated from
/// it work before any frame or dispatch has run (setup code, test prologues).
pub(crate) fn record_creation(runtime: &Rc<RefCell<ReactiveRuntime>>) {
    let epoch = runtime.borrow().epoch;
    LAST.with(|last| {
        *last.borrow_mut() = Some((epoch, Rc::downgrade(runtime)));
    });
}

/// Resolves the runtime a `Copy` handle's epoch names.
///
/// Stack top wins when present (driven regions); otherwise the last-active
/// record serves external access. Every failure mode panics with the remedy:
/// cross-engine use names both epochs, dropped runtimes name the drop, and a
/// missing record means no runtime was ever created on this thread.
pub(crate) fn resolve(epoch: u64) -> Rc<RefCell<ReactiveRuntime>> {
    // Fast path first and short: the stack is almost always non-empty inside
    // driven regions, which is where the overwhelming majority of accesses land.
    if let Some(top) = STACK.with(|stack| stack.borrow().last().cloned()) {
        let top_epoch = top.borrow().epoch;
        assert!(
            top_epoch == epoch,
            "signal used under the wrong engine (ambient epoch {top_epoch}, handle epoch {epoch}); \
             signals cannot cross engines — allocate from the engine that owns them"
        );
        return top;
    }
    match LAST.with(|last| last.borrow().clone()) {
        Some((last_epoch, weak)) if last_epoch == epoch => weak.upgrade().expect(
            "signal used after its runtime was dropped; \
             Copy handles do not keep their engine alive",
        ),
        Some((last_epoch, _)) => panic!(
            "signal used outside its engine's driven region (last-active epoch {last_epoch}, \
             handle epoch {epoch}); wrap the access in engine.guard(|| ...)"
        ),
        None => panic!(
            "signal used with no ambient runtime and no recorded runtime on this thread; \
             create the runtime through Engine (which records it) or push one with a guard"
        ),
    }
}
