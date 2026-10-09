// Single responsibility: Signal handle with two-phase topological glitch-free dispatch.

use crate::ambient;
use crate::derived::Derived;
use crate::id::SignalId;
use crate::readable::Readable;
use crate::runtime::ReactiveRuntime;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Reactive variable handle enabling tracked reads and notifications on write.
///
/// A `Copy` arena handle, not an `Rc`: the id names the slot (generationally),
/// the epoch names the runtime. Resolution goes through the ambient runtime —
/// the driven region's stack top, or the last-active record for external access —
/// with every mismatch panicking loudly. Copy one freely into closures and
/// props; there is nothing to clone.
pub struct Signal<T: 'static> {
    id: SignalId,
    epoch: u64,
    // `fn() -> T` is `Copy` for every `T` (function pointers always are) and
    // keeps the handle covariant in `T`, so this stays `Copy` for `String`,
    // `Vec`, and friends. `Cell<()>` contributes `!Sync` (a `Copy` type that
    // still refuses to sit in a `static`), narrowing cross-thread misuse to a
    // move-then-use that the ambient resolution panics on deterministically:
    // thread-locals are per-thread, so another thread finds no ambient runtime.
    // Full `!Send` is unrepresentable alongside `Copy` (no such std marker
    // exists, and `derive(Copy)` would demand `T: Copy` anyway); the runtime
    // panic is the backstop, covered by a regression test.
    //
    // `Clone`/`Copy` are implemented by hand: `derive` would add a `T: Copy`
    // bound the fields do not need, silently un-`Copy`ing `Signal<String>`.
    _marker: std::marker::PhantomData<(fn() -> T, Cell<()>)>,
}

impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for Signal<T> {}

impl<T: Clone + 'static> Signal<T> {
    /// Creates a new reactive signal with an initial value.
    pub fn new(runtime: Rc<RefCell<ReactiveRuntime>>, initial: T) -> Self {
        ambient::record_creation(&runtime);
        let id = runtime.borrow_mut().create_signal(Box::new(initial));
        let epoch = runtime.borrow().epoch;
        Self {
            id,
            epoch,
            _marker: std::marker::PhantomData,
        }
    }

    /// Resolves the runtime this handle names, panicking on any mismatch.
    ///
    /// One `Rc` bump per call site (not per access): callers resolve once, then
    /// borrow the runtime for the reads/writes they need.
    fn runtime_ref(&self) -> Rc<RefCell<ReactiveRuntime>> {
        ambient::resolve(self.epoch)
    }

    /// Reads the current signal value while subscribing the active observer.
    pub fn get(&self) -> T {
        let runtime = self.runtime_ref();
        runtime.borrow_mut().track_read(self.id);
        // Bound to a local (not the tail expression): the borrow temporary must
        // drop before `runtime` does at block end.
        let value = runtime.borrow().read_signal::<T>(self.id).clone();
        value
    }

    /// Derives a signal recomputing `f` whenever this signal changes.
    pub fn map<U: Clone + 'static>(&self, f: impl Fn(T) -> U + 'static) -> Derived<U> {
        let source = *self;
        Derived::new(self.runtime_ref(), move || f(source.get()))
    }

    /// Derives a value from this signal and another at once.
    ///
    /// See [`Derived::combine`]; this is the same capability from the writable side.
    pub fn combine<B2: Clone + 'static, B: Readable<B2> + Clone + 'static, U: Clone + 'static>(
        &self,
        other: B,
        f: impl Fn(T, B2) -> U + 'static,
    ) -> Derived<U> {
        // Built as a fresh derivation rather than a `map` over `self`, so both reads register as
        // dependencies and the result recomputes when *either* source changes.
        let a = *self;
        Derived::new(self.runtime_ref(), move || f(a.get(), other.read()))
    }

    /// Updates the signal value and dispatches two-phase invalidation notifications.
    pub fn set(&self, new_value: T) {
        self.write(new_value);
    }

    /// Updates the value while handing a physics adapter the velocity to reach it with.
    ///
    /// This is the gesture release path. A drag ends with the pointer moving at some speed,
    /// and if only the target value survives that handoff then any spring watching it starts
    /// from zero velocity and the flick visibly stops dead. Attaching the velocity to the
    /// write is what lets the value say where to go and the velocity say how fast to arrive.
    ///
    /// The hand-off is consumed by the first adapter that reads it, so it applies to this
    /// one change rather than every frame after it.
    pub fn set_with_velocity(&self, new_value: T, velocity: f32) {
        let runtime = self.runtime_ref();
        runtime.borrow_mut().set_motion_handoff(self.id, velocity);
        self.write_inner(&runtime, new_value);
    }

    /// Shared write path for both plain and velocity-carrying updates.
    ///
    /// Marks transitively affected derivations dirty over an explicit work
    /// stack rather than recursion, so arbitrarily deep chains cannot
    /// overflow the call stack on write. Evaluation itself stays lazy:
    /// `Derived::get` refreshes through `ensure_fresh`, so a burst of
    /// writes with no intermediate reads pays marking only.
    fn write(&self, new_value: T) {
        let runtime = self.runtime_ref();
        self.write_inner(&runtime, new_value);
    }

    /// Write body over an already-resolved runtime: one resolution per `set`,
    /// not one per borrow.
    fn write_inner(&self, runtime: &Rc<RefCell<ReactiveRuntime>>, new_value: T) {
        let effects = {
            let mut rt = runtime.borrow_mut();
            *rt.slot_mut(self.id) = Box::new(new_value);
            rt.note_write(self.id)
        };
        for effect_action in effects {
            effect_action();
        }
    }

    /// Modifies the signal in-place using a mutating closure.
    ///
    /// Lends a mutable reference directly into the arena slot — no clone of `T`.
    /// The closure runs while the runtime is mutably borrowed, so it must not
    /// call `.get()` or `.set()` on any signal (that would panic on re-borrow).
    /// Subscribers are notified exactly as `set` would.
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        let runtime = self.runtime_ref();
        let effects = {
            let mut rt = runtime.borrow_mut();
            let slot = rt.slot_mut(self.id);
            let val = slot.downcast_mut::<T>().expect("Signal type mismatch");
            f(val);
            rt.note_write(self.id)
        };
        for effect_action in effects {
            effect_action();
        }
    }

    /// Returns the unique signal identifier.
    pub fn id(&self) -> SignalId {
        self.id
    }

    /// Returns the epoch of the runtime this handle belongs to.
    ///
    /// Handles are views, not owners: code that needs to register work should
    /// either resolve the ambient runtime or keep the `Rc` it allocated from.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The runtime this signal belongs to, for owners that need to register work.
    ///
    /// Resolves ambiently (one `Rc` bump): prefer keeping the allocating `Rc`
    /// on setup paths that already hold it; this exists for call sites that
    /// only have the handle.
    pub fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        self.runtime_ref()
    }

    /// Registers a frame-driven ticker on this signal's runtime.
    ///
    /// This is how animation adapts to the reactive graph without either layer knowing
    /// about the other: the adapter is driven by time, and publishes through a signal.
    pub fn register_ticker(&self, ticker: Box<dyn crate::ticker::Ticker>) {
        self.runtime_ref().borrow_mut().register_ticker(ticker);
    }
}

impl<T: PartialEq + Clone + 'static> Signal<T> {
    /// Updates the signal value only if it differs from the current value.
    pub fn set_if_changed(&self, new_value: T) {
        let runtime = self.runtime_ref();
        let changed = runtime.borrow().read_signal::<T>(self.id).clone() != new_value;
        if changed {
            self.write_inner(&runtime, new_value);
        }
    }
}
