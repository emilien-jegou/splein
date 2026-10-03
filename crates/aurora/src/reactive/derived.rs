// Single responsibility: Memoized derived computation with clean resource deallocation on Drop.

use crate::reactive::id::SignalId;
use crate::reactive::observer::Observer;
use crate::reactive::readable::Readable;
use crate::reactive::runtime::ReactiveRuntime;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static DEFERRED_DISPOSALS: RefCell<Vec<(Rc<RefCell<ReactiveRuntime>>, SignalId)>> = const { RefCell::new(Vec::new()) };
}

/// Memoized computed value tracking upstream signal reads and updating lazily.
#[derive(Clone)]
pub struct Derived<T: 'static> {
    id: SignalId,
    compute: Rc<dyn Fn() -> T>,
    runtime: Rc<RefCell<ReactiveRuntime>>,
}

impl<T: Clone + 'static> Derived<T> {
    pub fn new<F: Fn() -> T + 'static>(runtime: Rc<RefCell<ReactiveRuntime>>, compute: F) -> Self {
        let id = runtime.borrow_mut().create_signal(Box::new(None::<T>));
        runtime.borrow_mut().push_observer(Observer::Derived(id));
        let initial = compute();
        runtime.borrow_mut().pop_observer();
        runtime.borrow_mut().values[id.0] = Box::new(Some(initial));
        Self {
            id,
            compute: Rc::new(compute),
            runtime: Rc::clone(&runtime),
        }
    }

    pub fn get(&self) -> T {
        self.runtime.borrow_mut().track_read(self.id);
        let is_dirty = self.runtime.borrow().derived_dirty.contains(&self.id);

        if is_dirty {
            self.runtime
                .borrow_mut()
                .push_observer(Observer::Derived(self.id));
            let fresh = (self.compute)();
            self.runtime.borrow_mut().pop_observer();

            let mut rt = self.runtime.borrow_mut();
            rt.values[self.id.0] = Box::new(Some(fresh.clone()));
            rt.derived_dirty.remove(&self.id);
            fresh
        } else {
            self.runtime
                .borrow()
                .read_signal::<Option<T>>(self.id)
                .as_ref()
                .unwrap()
                .clone()
        }
    }

    pub fn id(&self) -> SignalId {
        self.id
    }

    /// Derives a further value from this one.
    ///
    /// Same shape as [`Signal::map`], so a derivation can be refined without going back to
    /// a writable signal. Memoized on the same terms: it recomputes only when this
    /// derivation is dirty.
    pub fn map<U: Clone + 'static>(&self, f: impl Fn(T) -> U + 'static) -> Derived<U> {
        let source = self.clone();
        Derived::new(Rc::clone(&self.runtime), move || f(source.get()))
    }

    /// Derives a value from two upstream signals at once.
    ///
    /// `map` covers one source; this covers the case that genuinely needs two. A FLIP offset is
    /// the motivating example: it is `animated_slot - current_slot`, where the first signal is a
    /// spring the client drives and the second is the order that just changed. Reading one signal
    /// and reaching for the other inside a `map` closure would recompute only when the first
    /// moves, which happens to work for a spring that ticks every frame and silently does not
    /// for anything else. Depending on both explicitly makes the recompute condition correct by
    /// construction rather than by coincidence.
    pub fn combine<B2: Clone + 'static, B: Readable<B2> + Clone + 'static, U: Clone + 'static>(
        &self,
        other: B,
        f: impl Fn(T, B2) -> U + 'static,
    ) -> Derived<U> {
        let a = self.clone();
        let b = other;
        // `read` rather than `get`, so a plain `Signal` on the right tracks its reads the same
        // way this derivation does on the left. The recompute condition then depends on both
        // sources regardless of which of them happens to be writable.
        Derived::new(Rc::clone(&self.runtime), move || f(a.get(), b.read()))
    }

    /// The runtime this derivation belongs to, for owners that need to register work.
    pub fn runtime_handle(&self) -> Rc<RefCell<ReactiveRuntime>> {
        Rc::clone(&self.runtime)
    }
}

impl<T: 'static> Drop for Derived<T> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.compute) == 1 {
            match self.runtime.try_borrow_mut() {
                Ok(mut rt) => rt.dispose_derived(self.id),
                Err(_) => DEFERRED_DISPOSALS
                    .with(|q| q.borrow_mut().push((Rc::clone(&self.runtime), self.id))),
            }
        }
    }
}

/// Flushes and disposes any derived signals deferred during active runtime borrows.
pub fn drain_deferred_disposals() {
    DEFERRED_DISPOSALS.with(|q| {
        let mut items = q.borrow_mut();
        items.retain(|(runtime, id)| {
            if let Ok(mut rt) = runtime.try_borrow_mut() {
                rt.dispose_derived(*id);
                false
            } else {
                true
            }
        });
    });
}
