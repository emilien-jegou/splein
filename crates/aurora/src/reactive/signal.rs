// Single responsibility: Signal handle with two-phase topological glitch-free dispatch.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use crate::reactive::derived::Derived;
use crate::reactive::id::SignalId;
use crate::reactive::observer::Observer;
use crate::reactive::runtime::ReactiveRuntime;

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
        Self { id, runtime, _marker: std::marker::PhantomData }
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

    /// Updates signal value and dispatches two-phase invalidation notifications.
    pub fn set(&self, new_value: T) {
        self.runtime.borrow_mut().values[self.id.0] = Box::new(new_value);

        let mut visited = HashSet::new();
        self.mark_derived_dirty(self.id, &mut visited);

        let mut effects_to_run = Vec::new();
        for &sig in &visited {
            let observers = self.runtime.borrow().subscribers.get(&sig).cloned().unwrap_or_default();
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
        if !visited.insert(sig) { return; }
        let observers = self.runtime.borrow().subscribers.get(&sig).cloned().unwrap_or_default();
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
