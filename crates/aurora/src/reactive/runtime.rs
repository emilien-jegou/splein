// Single responsibility: Reactive signal dependency graph, batches, and notification dispatch.

use crate::foundation::snapshot::GeometrySnapshot;
use crate::reactive::id::{SignalId, SubscriberId};
use crate::reactive::observer::Observer;
use crate::reactive::ticker::{Ticker, TickerRegistry};
use crate::tree::NodeId;
use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

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
    tickers: TickerRegistry,
    frame_seconds: f32,
    /// Motion hand-off attached to the next write of each signal, consumed once.
    ///
    /// The reactive graph stores this as an opaque number and never interprets it: only a
    /// physics adapter does. Keeping it here rather than on the signal is what lets a
    /// gesture hand its velocity to a spring without the graph knowing what a spring is.
    motion_handoff: HashMap<SignalId, f32>,
    /// Hook state kept per component instance, so a rebuilt component finds its own state.
    ///
    /// Keyed by the instance identity a scope was entered under, then by hook call order.
    /// A component body always calls its hooks in the same order, so the index is a stable
    /// name for "the second signal this component allocates" across rebuilds.
    pub(crate) hook_states: HashMap<Rc<str>, Vec<Option<Rc<dyn Any>>>>,
    /// Where every element holding a geometry handle ended up this frame.
    ///
    /// The one place the reactive layer touches geometry. It is a plain table rather than a
    /// signal graph precisely because reading it must stay a read: a client asking where a row
    /// landed is not a change to anything, and turning it into a notification would let a
    /// measurement schedule the frame that invalidates the measurement.
    snapshot: GeometrySnapshot,
    /// Arena indices whose geometry is published each frame, because something asked to read it.
    ///
    /// A set rather than a list: the publish pass writes each one into a table keyed by the same
    /// index, so the order it visits them in cannot matter, and a handle that is bound and
    /// unbound repeatedly must not grow the list.
    watched: HashSet<NodeId>,
    /// Work that has to happen after layout has placed things and before the frame is compiled.
    ///
    /// This exists for one reason: a client that reacts to where layout put something cannot use
    /// a ticker, because tickers run *before* layout. A FLIP is the case in point — it has to
    /// compare the position from last frame against the position layout just chose, and the
    /// result has to reach the transform before the frame is compiled. Sampling from a ticker
    /// would compare against a stale position and apply its answer a frame late, which shows up
    /// as the element visibly sitting at its destination for one frame before jumping back to
    /// animate.
    ///
    /// It is an ordering seam and nothing more. No animation state lives here, and the callbacks
    /// are ordinary client code: the engine runs them in a defined order because that order is
    /// part of the frame's contract, not because it wants to know what they do.
    post_layout: RefCell<Vec<Rc<dyn Fn(f32)>>>,
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
            tickers: TickerRegistry::default(),
            frame_seconds: 0.0,
            motion_handoff: HashMap::new(),
            hook_states: HashMap::new(),
            snapshot: GeometrySnapshot::new(),
            watched: HashSet::new(),
            post_layout: RefCell::new(Vec::new()),
        }
    }

    /// The frame's geometry table, shared with every element handle on this runtime.
    ///
    /// Returned by value because the table is a shared view rather than the data itself: handing
    /// out a clone costs a refcount, and two callers must not be able to diverge.
    pub fn snapshot(&self) -> GeometrySnapshot {
        self.snapshot.clone()
    }

    /// Asks for an element's box to be published after every layout.
    ///
    /// Called by the reconciler when an element binds a handle. Idempotent, because a rebuild
    /// binds the same handle again.
    pub fn watch_geometry(&mut self, node: NodeId) {
        self.watched.insert(node);
    }

    /// Whether anything is watching geometry at all.
    ///
    /// The publish pass is skipped outright when nothing is, so the common case of a scene with
    /// no handles costs one branch.
    pub fn geometry_watched(&self) -> bool {
        !self.watched.is_empty()
    }

    /// Drops last frame's measurements, before layout writes this frame's.
    pub fn begin_geometry_frame(&mut self) {
        if self.watched.is_empty() {
            return;
        }
        self.snapshot.clear();
    }

    /// The watched indices, cloned out so publication can read the arena while iterating.
    ///
    /// Exposed as a slice-producing accessor rather than handing out the set, because the
    /// publish pass holds a borrow of the arena at the same time.
    pub fn watched_geometry(&self) -> Vec<NodeId> {
        self.watched.iter().copied().collect()
    }

    /// Registers work to run after layout, before the frame is compiled.
    ///
    /// Called once per owning component rather than per frame, so this list grows with the number
    /// of components that need the ordering and not with the number of frames.
    pub fn register_post_layout(&self, work: impl Fn(f32) + 'static) {
        self.post_layout.borrow_mut().push(Rc::new(work));
    }

    /// A copy of the registered callbacks, sharing each one rather than moving it.
    ///
    /// The list is *not* drained: registration is permanent, and a callback that ran once has to run
    /// again next frame. Cloning the `Rc`s is what lets the borrow be released before any of them
    /// runs, so a callback that reads a signal cannot re-enter the `RefCell` this list lives in.
    fn post_layout_snapshot(&self) -> Vec<Rc<dyn Fn(f32)>> {
        self.post_layout.borrow().clone()
    }

    /// Number of registered post-layout callbacks. Tests and diagnostics only.
    pub fn post_layout_count(&self) -> usize {
        self.post_layout.borrow().len()
    }

    /// Attaches a motion hand-off to the next write of `id`.
    ///
    /// Used when a gesture ends: the value says where to go, and the hand-off says with what
    /// velocity to get there, which is the difference between a flick that carries and a
    /// card that stops dead.
    pub fn set_motion_handoff(&mut self, id: SignalId, velocity: f32) {
        self.motion_handoff.insert(id, velocity);
    }

    /// Takes the pending hand-off for `id`, if any, clearing it.
    ///
    /// Single-shot by design: a hand-off describes one gesture ending, so a transition must
    /// consume it once rather than reapplying the same velocity every frame.
    pub fn take_motion_handoff(&mut self, id: SignalId) -> Option<f32> {
        self.motion_handoff.remove(&id)
    }

    /// Sets global invalidation callback invoked whenever subscribers become dirty.
    pub fn set_on_dirty<F: Fn() + 'static>(&mut self, hook: F) {
        self.on_dirty = Some(Rc::new(hook));
    }

    /// Registers a frame-driven ticker.
    pub fn register_ticker(&mut self, ticker: Box<dyn Ticker>) {
        self.tickers.push(ticker);
    }

    /// Whether any registered ticker still wants frames.
    pub fn has_active_tickers(&self) -> bool {
        self.tickers.has_active()
    }

    /// Monotonic seconds advanced by the last `tick`, for components that need wall time.
    pub fn frame_seconds(&self) -> f32 {
        self.frame_seconds
    }

    /// Requests a frame without changing any state.
    ///
    /// Lets an owner that mutates private state (a playback control, for instance) still
    /// wake a sleeping event loop.
    pub fn wake(&mut self) {
        if let Some(hook) = &self.on_dirty {
            let cb = Rc::clone(hook);
            cb();
        }
    }

    /// Moves the ticker registry out so it can be advanced without holding this borrow.
    ///
    /// Tickers write signals, so advancing them under a live `borrow_mut` would re-enter
    /// the same `RefCell`. Callers must pair this with [`Self::restore_tickers`].
    pub(crate) fn take_tickers(&mut self) -> TickerRegistry {
        std::mem::take(&mut self.tickers)
    }

    /// Returns a registry after ticking, keeping tickers registered during the tick.
    pub(crate) fn restore_tickers(&mut self, ticked: TickerRegistry) {
        let registered = std::mem::take(&mut self.tickers);
        self.tickers = ticked;
        self.tickers.prepend(registered);
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
            self.dirty_subscribers
                .extend(self.pending_subscribers.drain());
            if let Some(hook) = &self.on_dirty {
                let cb = Rc::clone(hook);
                cb();
            }
        }
    }

    pub fn dispose_effect(&mut self, id: SubscriberId) {
        self.effects.remove(&id);
        for subs in self.subscribers.values_mut() {
            subs.remove(&Observer::Effect(id));
        }
    }

    pub fn dispose_subscriber(&mut self, id: SubscriberId) {
        self.listeners.remove(&id);
        self.dirty_subscribers.remove(&id);
        self.pending_subscribers.remove(&id);
        for subs in self.subscribers.values_mut() {
            subs.remove(&Observer::Subscriber(id));
        }
    }

    pub fn dispose_derived(&mut self, id: SignalId) {
        self.subscribers.remove(&id);
        self.derived_dirty.remove(&id);
        self.motion_handoff.remove(&id);
        for subs in self.subscribers.values_mut() {
            subs.remove(&Observer::Derived(id));
        }
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
        self.values[id.0]
            .downcast_ref::<T>()
            .expect("Signal type mismatch")
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

    /// Returns skipped subscribers to the dirty set so the next drain pass retries them.
    pub fn requeue_dirty_subscribers(&mut self, subs: HashSet<SubscriberId>) {
        if subs.is_empty() {
            return;
        }
        self.dirty_subscribers.extend(subs);
    }
}

/// Runs the work registered for after layout, with the runtime's borrow released.
///
/// `dt` is the same animation delta the tickers were given, so a transition that lives here
/// integrates at the same rate as one that lives there and lands in the same place at 30fps as at
/// 144fps.
pub fn run_post_layout(runtime: &Rc<RefCell<ReactiveRuntime>>, dt: f32) {
    // Copied out before anything runs: a callback that touches a signal would otherwise re-enter
    // a `RefCell` the caller is still holding.
    let work = runtime.borrow().post_layout_snapshot();
    for entry in work {
        entry(dt);
    }
}

/// Advances every frame-driven ticker by `dt` seconds.
///
/// This is the single seam between the frame loop and time-based work. Tickers own
/// signals and write them here, so animation, clocks, and timers need no knowledge of
/// the frame loop, and the frame loop needs no knowledge of them.
pub fn tick(runtime: &Rc<RefCell<ReactiveRuntime>>, dt: f32) {
    let mut tickers = runtime.borrow_mut().take_tickers();
    if !tickers.is_empty() {
        tickers.tick(dt);
        runtime.borrow_mut().restore_tickers(tickers);
    }
    runtime.borrow_mut().frame_seconds += dt.max(0.0);
}

impl Default for ReactiveRuntime {
    fn default() -> Self {
        Self::new()
    }
}
