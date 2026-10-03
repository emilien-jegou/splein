// Single responsibility: Signal handle with two-phase topological glitch-free dispatch.

use crate::reactive::derived::Derived;
use crate::reactive::id::SignalId;
use crate::reactive::observer::Observer;
use crate::reactive::readable::Readable;
use crate::reactive::runtime::ReactiveRuntime;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// Reactive variable handle enabling tracked reads and notifications on write.
#[derive(Clone)]
pub struct Signal<T: 'static> {
    id: SignalId,
    runtime: Rc<RefCell<ReactiveRuntime>>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Clone + 'static> Signal<T> {
    /// Creates a new reactive signal with an initial value.
    pub fn new(runtime: Rc<RefCell<ReactiveRuntime>>, initial: T) -> Self {
        let id = runtime.borrow_mut().create_signal(Box::new(initial));
        Self {
            id,
            runtime,
            _marker: std::marker::PhantomData,
        }
    }

    /// Reads the current signal value while subscribing the active observer.
    pub fn get(&self) -> T {
        self.runtime.borrow_mut().track_read(self.id);
        self.runtime.borrow().read_signal::<T>(self.id).clone()
    }

    /// Derives a signal recomputing `f` whenever this signal changes.
    pub fn map<U: Clone + 'static>(&self, f: impl Fn(T) -> U + 'static) -> Derived<U> {
        let source = self.clone();
        Derived::new(Rc::clone(&self.runtime), move || f(source.get()))
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
        let a = self.clone();
        Derived::new(Rc::clone(&self.runtime), move || f(a.get(), other.read()))
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
        self.runtime
            .borrow_mut()
            .set_motion_handoff(self.id, velocity);
        self.write(new_value);
    }

    /// Shared write path for both plain and velocity-carrying updates.
    fn write(&self, new_value: T) {
        self.runtime.borrow_mut().values[self.id.0] = Box::new(new_value);

        let mut visited = HashSet::new();
        self.mark_derived_dirty(self.id, &mut visited);

        let mut effects_to_run = Vec::new();
        for &sig in &visited {
            let observers = self
                .runtime
                .borrow()
                .subscribers
                .get(&sig)
                .cloned()
                .unwrap_or_default();
            for obs in observers {
                match obs {
                    Observer::Subscriber(sub_id) => {
                        self.runtime.borrow_mut().notify_subscriber(sub_id);
                        if let Some(action) = self.runtime.borrow().listeners.get(&sub_id) {
                            effects_to_run.push(Rc::clone(action));
                        }
                    }
                    Observer::Effect(eff_id) => {
                        if let Some(action) = self.runtime.borrow().effects.get(&eff_id) {
                            effects_to_run.push(Rc::clone(action));
                        }
                    }
                    Observer::Derived(_) => {}
                }
            }
        }

        for effect_action in effects_to_run {
            effect_action();
        }
    }

    fn mark_derived_dirty(&self, sig: SignalId, visited: &mut HashSet<SignalId>) {
        if !visited.insert(sig) {
            return;
        }
        let observers = self
            .runtime
            .borrow()
            .subscribers
            .get(&sig)
            .cloned()
            .unwrap_or_default();
        for obs in observers {
            if let Observer::Derived(dep_sig) = obs {
                self.runtime.borrow_mut().derived_dirty.insert(dep_sig);
                self.mark_derived_dirty(dep_sig, visited);
            }
        }
    }

    /// Modifies the signal in-place using a mutating closure.
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        let mut val = self.get();
        f(&mut val);
        self.set(val);
    }

    /// Returns the unique signal identifier.
    pub fn id(&self) -> SignalId {
        self.id
    }

    /// The runtime this signal belongs to, for owners that need to register work.
    pub fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        Rc::clone(&self.runtime)
    }

    /// Registers a frame-driven ticker on this signal's runtime.
    ///
    /// This is how animation adapts to the reactive graph without either layer knowing
    /// about the other: the adapter is driven by time, and publishes through a signal.
    pub fn register_ticker(&self, ticker: Box<dyn crate::reactive::ticker::Ticker>) {
        self.runtime.borrow_mut().register_ticker(ticker);
    }
}

impl<T: PartialEq + Clone + 'static> Signal<T> {
    /// Updates the signal value only if it differs from the current value.
    pub fn set_if_changed(&self, new_value: T) {
        let runtime = self.runtime.borrow();
        let current = runtime.read_signal::<T>(self.id);
        if *current == new_value {
            return;
        }
        drop(runtime);
        self.set(new_value);
    }
}
