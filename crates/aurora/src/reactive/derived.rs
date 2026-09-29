// Single responsibility: Memoized derived computation with clean resource deallocation on Drop.

use std::cell::RefCell;
use std::rc::Rc;
use crate::reactive::id::SignalId;
use crate::reactive::observer::Observer;
use crate::reactive::runtime::ReactiveRuntime;

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
        Self { id, compute: Rc::new(compute), runtime: Rc::clone(&runtime) }
    }

    pub fn get(&self) -> T {
        self.runtime.borrow_mut().track_read(self.id);
        let is_dirty = self.runtime.borrow().derived_dirty.contains(&self.id);

        if is_dirty {
            self.runtime.borrow_mut().push_observer(Observer::Derived(self.id));
            let fresh = (self.compute)();
            self.runtime.borrow_mut().pop_observer();

            let mut rt = self.runtime.borrow_mut();
            rt.values[self.id.0] = Box::new(Some(fresh.clone()));
            rt.derived_dirty.remove(&self.id);
            fresh
        } else {
            self.runtime.borrow().read_signal::<Option<T>>(self.id).as_ref().unwrap().clone()
        }
    }

    pub fn id(&self) -> SignalId { self.id }
}

impl<T: 'static> Drop for Derived<T> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.compute) == 1 {
            match self.runtime.try_borrow_mut() {
                Ok(mut rt) => rt.dispose_derived(self.id),
                Err(_) => DEFERRED_DISPOSALS.with(|q| q.borrow_mut().push((Rc::clone(&self.runtime), self.id))),
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
            } else { true }
        });
    });
}
