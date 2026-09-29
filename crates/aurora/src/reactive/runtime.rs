// Single responsibility: Reactive signal dependency graph, batches, and notification dispatch.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use crate::reactive::id::{SignalId, SubscriberId};
use crate::reactive::observer::Observer;

/// Central reactive state store managing signals, derived values, and observers.
pub struct ReactiveRuntime {
    pub(crate) values: Vec<Box<dyn Any>>,
    pub(crate) subscribers: HashMap<SignalId, HashSet<Observer>>,
    pub(crate) effects: HashMap<SubscriberId, Rc<dyn Fn()>>,
    pub(crate) listeners: HashMap<SubscriberId, Rc<dyn Fn()>>,
    pub(crate) derived_dirty: HashSet<SignalId>,
    observer_stack: Vec<Observer>,
    dirty_subscribers: HashSet<SubscriberId>,
    pending_subscribers: HashSet<SubscriberId>,
    batch_depth: usize,
    next_subscriber_id: usize,
    on_dirty: Option<Rc<dyn Fn()>>,
}

impl ReactiveRuntime {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            subscribers: HashMap::new(),
            effects: HashMap::new(),
            listeners: HashMap::new(),
            derived_dirty: HashSet::new(),
            observer_stack: Vec::new(),
            dirty_subscribers: HashSet::new(),
            pending_subscribers: HashSet::new(),
            batch_depth: 0,
            next_subscriber_id: 1,
            on_dirty: None,
        }
    }

    /// Sets global invalidation callback invoked whenever subscribers become dirty.
    pub fn set_on_dirty<F: Fn() + 'static>(&mut self, hook: F) {
        self.on_dirty = Some(Rc::new(hook));
    }

    pub fn create_signal(&mut self, initial: Box<dyn Any>) -> SignalId {
        let id = SignalId(self.values.len());
        self.values.push(initial);
        id
    }

    pub fn alloc_subscriber(&mut self) -> SubscriberId {
        let id = SubscriberId(self.next_subscriber_id);
        self.next_subscriber_id += 1;
        id
    }

    pub fn register_listener<F: Fn() + 'static>(&mut self, id: SubscriberId, action: F) {
        self.listeners.insert(id, Rc::new(action));
    }

    pub fn begin_batch(&mut self) {
        self.batch_depth += 1;
    }

    pub fn end_batch(&mut self) {
        self.batch_depth = self.batch_depth.saturating_sub(1);
        if self.batch_depth == 0 && !self.pending_subscribers.is_empty() {
            self.dirty_subscribers.extend(self.pending_subscribers.drain());
            if let Some(hook) = &self.on_dirty {
                let cb = Rc::clone(hook);
                cb();
            }
        }
    }

    pub fn dispose_effect(&mut self, id: SubscriberId) {
        self.effects.remove(&id);
        for subs in self.subscribers.values_mut() { subs.remove(&Observer::Effect(id)); }
    }

    pub fn dispose_subscriber(&mut self, id: SubscriberId) {
        self.listeners.remove(&id);
        self.dirty_subscribers.remove(&id);
        self.pending_subscribers.remove(&id);
        for subs in self.subscribers.values_mut() { subs.remove(&Observer::Subscriber(id)); }
    }

    pub fn dispose_derived(&mut self, id: SignalId) {
        self.subscribers.remove(&id);
        self.derived_dirty.remove(&id);
        for subs in self.subscribers.values_mut() { subs.remove(&Observer::Derived(id)); }
    }

    pub fn push_observer(&mut self, observer: Observer) {
        self.observer_stack.push(observer);
    }

    pub fn pop_observer(&mut self) -> Option<Observer> {
        self.observer_stack.pop()
    }

    pub fn track_read(&mut self, id: SignalId) {
        if let Some(&current) = self.observer_stack.last() {
            self.subscribers.entry(id).or_default().insert(current);
        }
    }

    pub fn read_signal<T: 'static>(&self, id: SignalId) -> &T {
        self.values[id.0].downcast_ref::<T>().expect("Signal type mismatch")
    }

    pub fn notify_subscriber(&mut self, id: SubscriberId) {
        if self.batch_depth > 0 {
            self.pending_subscribers.insert(id);
        } else {
            self.dirty_subscribers.insert(id);
            if let Some(hook) = &self.on_dirty {
                let cb = Rc::clone(hook);
                cb();
            }
        }
    }

    pub fn drain_dirty_subscribers(&mut self) -> HashSet<SubscriberId> {
        std::mem::take(&mut self.dirty_subscribers)
    }
}

impl Default for ReactiveRuntime {
    fn default() -> Self {
        Self::new()
    }
}
