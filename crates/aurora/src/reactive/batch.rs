// Single responsibility: Panic-safe RAII transactional batch scopes.

use std::cell::RefCell;
use std::rc::Rc;
use crate::reactive::derived::drain_deferred_disposals;
use crate::reactive::ReactiveRuntime;

struct BatchGuard<'a>(&'a Rc<RefCell<ReactiveRuntime>>);

impl<'a> Drop for BatchGuard<'a> {
    fn drop(&mut self) {
        self.0.borrow_mut().end_batch();
        drain_deferred_disposals();
    }
}

/// Executes closure inside atomic batch, flushing subscriber updates upon completion.
pub fn batch<F: FnOnce() -> R, R>(runtime: &Rc<RefCell<ReactiveRuntime>>, f: F) -> R {
    runtime.borrow_mut().begin_batch();
    let _guard = BatchGuard(runtime);
    f()
}
