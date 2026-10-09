// Single responsibility: Memoized derived computation with clean resource deallocation on Drop.

use crate::graph::id::SignalId;
use crate::graph::observer::Observer;
use crate::graph::readable::Readable;
use crate::graph::runtime::ReactiveRuntime;
use std::cell::RefCell;
use rustc_hash::FxHashSet;
use std::rc::Rc;

thread_local! {
    static DEFERRED_DISPOSALS: RefCell<Vec<(Rc<RefCell<ReactiveRuntime>>, SignalId)>> = const { RefCell::new(Vec::new()) };
}

/// Memoized computed value tracking upstream signal reads and updating lazily.
///
/// The compute closure lives in the runtime's `derived_recompute` registry,
/// never in the handle: a chain of derivations would otherwise own each
/// other through nested `Rc` closures, and dropping a deep chain would nest
/// one destructor per link until the stack overflows. Handles only share a
/// counting token so `Drop` still disposes exactly when the last clone goes
/// away.
#[derive(Clone)]
pub struct Derived<T: 'static> {
    id: SignalId,
    runtime: Rc<RefCell<ReactiveRuntime>>,
    _shared: Rc<()>,
    _marker: std::marker::PhantomData<T>,
}

/// Refreshes a dirty derivation and every dirty transitive dependency it
/// reads, dependencies first, over an explicit work stack.
///
/// Walks up through `derived_sources` (dependency direction) collecting
/// dirty derivations in post-order, so evaluation runs deepest-dependency
/// first: each recompute reads already-fresh inputs, and the nested `get()`
/// calls inside user closures hit clean values and return immediately
/// instead of recursing. Stack usage stays flat no matter how deep the
/// chain is. Each flag is claimed before its recompute runs, so a cyclic
/// graph reads a stale value and terminates instead of looping forever.
fn ensure_fresh(runtime: &Rc<RefCell<ReactiveRuntime>>, id: SignalId) {
    let order = {
        let rt = runtime.borrow();
        let mut visited = FxHashSet::default();
        let mut order = Vec::new();
        let mut stack = vec![(id, false)];
        visited.insert(id);
        while let Some((cur, expanded)) = stack.pop() {
            if expanded {
                order.push(cur);
                continue;
            }
            stack.push((cur, true));
            if let Some(srcs) = rt.derived_sources.get(&cur) {
                for src in srcs {
                    if !visited.insert(*src) {
                        continue;
                    }
                    // Only live, dirty derivations need evaluation; plain
                    // signals and fresh derivations are read as-is.
                    let needs_eval = rt.derived_recompute.contains_key(src)
                        && rt.derived_dirty.contains(src);
                    if needs_eval {
                        stack.push((*src, false));
                    }
                }
            }
        }
        order
    };

    // No runtime borrow is held across `recompute()` (arbitrary user code).
    // A slot disposed mid-loop is skipped rather than resurrected.
    for cur in order {
        // Claim the slot and take its recompute under one borrow; the flag
        // is cleared up front so a cyclic re-entry reads stale data and
        // terminates instead of looping. The observer is pushed here so the
        // recompute re-tracks the sources it actually reads.
        let recompute = {
            let mut rt = runtime.borrow_mut();
            let recompute = rt.derived_recompute.get(&cur).cloned();
            if recompute.is_some() {
                rt.derived_dirty.remove(&cur);
                rt.push_observer(Observer::Derived(cur));
            }
            recompute
        };
        let Some(recompute) = recompute else {
            continue;
        };
        let value = recompute();
        {
            let mut rt = runtime.borrow_mut();
            rt.pop_observer();
            // Store only if still live; a re-entrant dispose may have
            // unlinked the slot mid-recompute. Generation-checked: a stale id
            // must not overwrite its successor's slot.
            if rt.derived_recompute.contains_key(&cur) {
                if let Some(slot) = rt.values.get_mut(cur.index) {
                    if slot.generation == cur.generation && slot.value.is_some() {
                        slot.value = Some(value);
                    }
                }
            }
        }
    }
}

impl<T: Clone + 'static> Derived<T> {
    pub fn new<F: Fn() -> T + 'static>(runtime: Rc<RefCell<ReactiveRuntime>>, compute: F) -> Self {
        let id = runtime.borrow_mut().create_signal(Box::new(None::<T>));
        runtime.borrow_mut().push_observer(Observer::Derived(id));
        let initial = compute();
        runtime.borrow_mut().pop_observer();
        *runtime.borrow_mut().slot_mut(id) = Box::new(Some(initial));
        // Type-erased re-entry for iterative pulls: `ensure_fresh` refreshes
        // this derivation through the registry without needing the handle,
        // which keeps arbitrarily deep chains off the call stack.
        {
            let compute = Rc::new(compute);
            let recompute: Rc<dyn Fn() -> Box<dyn std::any::Any>> = Rc::new({
                let compute = Rc::clone(&compute);
                move || Box::new(Some(compute())) as Box<dyn std::any::Any>
            });
            runtime.borrow_mut().derived_recompute.insert(id, recompute);
            Self {
                id,
                runtime: Rc::clone(&runtime),
                _shared: Rc::new(()),
                _marker: std::marker::PhantomData,
            }
        }
    }

    pub fn get(&self) -> T {
        self.runtime.borrow_mut().track_read(self.id);
        if self.runtime.borrow().derived_dirty.contains(&self.id) {
            ensure_fresh(&self.runtime, self.id);
        }
        self.runtime
            .borrow()
            .read_signal::<Option<T>>(self.id)
            .as_ref()
            .unwrap()
            .clone()
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
        if Rc::strong_count(&self._shared) == 1 {
            match self.runtime.try_borrow_mut() {
                Ok(mut rt) => rt.dispose_derived(self.id),
                Err(_) => DEFERRED_DISPOSALS
                    .with(|q| q.borrow_mut().push((Rc::clone(&self.runtime), self.id))),
            }
        }
    }
}

/// Flushes and disposes any derived signals deferred during active runtime borrows.
///
/// Pops one entry at a time and releases the queue borrow before disposing:
/// disposal drops recompute closures that own further handles, and a nested
/// handle drop must find this queue unborrowed (it re-queues itself here
/// when the runtime is busy). Holding the borrow across `dispose_derived`
/// would panic on the nested re-borrow instead.
pub fn drain_deferred_disposals() {
    loop {
        let item = DEFERRED_DISPOSALS.with(|q| q.borrow_mut().pop());
        let Some((runtime, id)) = item else {
            break;
        };
        // Scoped so the guard (and its borrow of `runtime`) ends before
        // either branch below moves `runtime` again.
        let done = match runtime.try_borrow_mut() {
            Ok(mut rt) => {
                rt.dispose_derived(id);
                true
            }
            Err(_) => false,
        };
        if !done {
            DEFERRED_DISPOSALS.with(|q| q.borrow_mut().push((runtime, id)));
            break;
        }
    }
}
