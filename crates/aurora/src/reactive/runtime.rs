// Single responsibility: Reactive signal dependency graph, batches, and notification dispatch.

use crate::foundation::snapshot::GeometrySnapshot;
use crate::reactive::id::{SignalId, SubscriberId};
use crate::reactive::observer::Observer;
use crate::reactive::ticker::{Ticker, TickerRegistry};
use rustc_hash::{FxHashMap, FxHashSet};
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Source of unique runtime epochs, distinguishing engines thread-safely.
///
/// Epochs only need uniqueness, not order: a stale global is harmless because
/// every runtime reads it exactly once, at construction.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

/// One call-site-keyed hook slot: the filed value plus the type it was filed as.
///
/// The type name exists so a hook whose signature changed across edits fails loudly
/// with both types named, instead of silently allocating fresh state and resetting
/// the component.
pub(crate) struct KeyedSlot {
    pub(crate) value: Rc<dyn Any>,
    pub(crate) type_name: &'static str,
}

/// One value slot in the reactive store: the value plus its era.
///
/// `value` is `None` while the slot sits on the free list. The generation is
/// what makes a stale handle fail loudly instead of aliasing a reused slot.
pub(crate) struct SignalSlot {
    pub(crate) value: Option<Box<dyn Any>>,
    pub(crate) generation: u32,
}

/// Central reactive state store managing signals, derived values, and observers.
pub struct ReactiveRuntime {
    /// Unique identity of this runtime, minted once at construction.
    ///
    /// `Copy` handles name this epoch instead of holding an `Rc`: resolution
    /// matches handle epoch against runtime epoch, so cross-engine use fails
    /// loudly rather than writing into the wrong graph.
    pub(crate) epoch: u64,
    pub(crate) values: Vec<SignalSlot>,
    /// Indices of free slots; popped (LIFO) by `create_signal`.
    free_slots: Vec<usize>,
    pub(crate) subscribers: FxHashMap<SignalId, FxHashSet<Observer>>,
    pub(crate) effects: FxHashMap<SubscriberId, Rc<dyn Fn()>>,
    pub(crate) listeners: FxHashMap<SubscriberId, Rc<dyn Fn()>>,
    pub(crate) derived_dirty: FxHashSet<SignalId>,
    /// Type-erased recompute closures for live derivations, keyed by slot.
    ///
    /// Lets a write eagerly refresh every transitive derivation in
    /// dependency-first order without calling back into user handles, so
    /// deep chains cannot overflow the call stack through nested `get()`.
    /// Entries are added by `Derived::new` and removed by `dispose_derived`.
    pub(crate) derived_recompute: FxHashMap<SignalId, Rc<dyn Fn() -> Box<dyn Any>>>,
    /// Reverse edge index: sources each live derivation currently reads.
    ///
    /// Lets `dispose_derived` unlink exactly the subscriber sets that
    /// reference the id instead of sweeping every set in the runtime, which
    /// keeps teardown of large graphs linear rather than quadratic.
    /// Recorded in `track_read` whenever the reading observer is a derivation.
    pub(crate) derived_sources: FxHashMap<SignalId, FxHashSet<SignalId>>,
    observer_stack: Vec<Observer>,
    dirty_subscribers: FxHashSet<SubscriberId>,
    pending_subscribers: FxHashSet<SubscriberId>,
    batch_depth: usize,
    /// Signals written inside the current batch, collapsed to a set so
    /// repeated writes to the same signal walk the graph once.
    ///
    /// Drained with a single invalidation when the outermost batch scope
    /// exits, which is what makes a burst of writes pay marking only once
    /// instead of once per write.
    pending_roots: FxHashSet<SignalId>,
    next_subscriber_id: usize,
    on_dirty: Option<Rc<dyn Fn()>>,
    tickers: TickerRegistry,
    frame_seconds: f32,
    /// Motion hand-off attached to the next write of each signal, consumed once.
    ///
    /// The reactive graph stores this as an opaque number and never interprets it: only a
    /// physics adapter does. Keeping it here rather than on the signal is what lets a
    /// gesture hand its velocity to a spring without the graph knowing what a spring is.
    motion_handoff: FxHashMap<SignalId, f32>,
    /// Hook state kept per component instance, so a rebuilt component finds its own state.
    ///
    /// Keyed by the instance identity a scope was entered under, then by hook call order.
    /// A component body always calls its hooks in the same order, so the index is a stable
    /// name for "the second signal this component allocates" across rebuilds.
    pub(crate) hook_states: FxHashMap<Rc<str>, Vec<Option<Rc<dyn Any>>>>,
    /// Call-site-keyed hook state, for hooks that must survive conditional execution.
    ///
    /// Same instance keying as [`Self::hook_states`], but the second key is the hook's
    /// call-site identity (`file:line` or an explicit name) rather than its ordinal.
    /// A hook skipped on one build simply leaves its entry untouched, so later hooks
    /// never shift slots when an earlier one is conditional.
    pub(crate) keyed_hook_states: FxHashMap<Rc<str>, FxHashMap<&'static str, KeyedSlot>>,
    /// Where every element holding a geometry handle ended up this frame.
    ///
    /// The one place the reactive layer touches geometry. It is a plain table rather than a
    /// signal graph precisely because reading it must stay a read: a client asking where a row
    /// landed is not a change to anything, and turning it into a notification would let a
    /// measurement schedule the frame that invalidates the measurement.
    snapshot: GeometrySnapshot,
    /// Opaque element identities whose geometry is published each frame, because something
    /// asked to read it.
    ///
    /// Deliberately not a tree `NodeId`: the reactive layer never learns what a node is. The
    /// tree packs its generational id into the `u64` on the way in (`watch_geometry`) and
    /// unpacks it on the way out (`watched_geometry`), so generation staleness is still
    /// caught at publication time without the graph importing the arena.
    ///
    /// A set rather than a list: the publish pass writes each one into a table keyed by the same
    /// index, so the order it visits them in cannot matter, and a handle that is bound and
    /// unbound repeatedly must not grow the list.
    watched: FxHashSet<u64>,
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
            epoch: NEXT_EPOCH.fetch_add(1, Ordering::Relaxed),
            values: Vec::new(),
            free_slots: Vec::new(),
            subscribers: FxHashMap::default(),
            effects: FxHashMap::default(),
            listeners: FxHashMap::default(),
            derived_dirty: FxHashSet::default(),
            derived_recompute: FxHashMap::default(),
            derived_sources: FxHashMap::default(),
            observer_stack: Vec::new(),
            dirty_subscribers: FxHashSet::default(),
            pending_subscribers: FxHashSet::default(),
            batch_depth: 0,
            pending_roots: FxHashSet::default(),
            next_subscriber_id: 1,
            on_dirty: None,
            tickers: TickerRegistry::default(),
            frame_seconds: 0.0,
            motion_handoff: FxHashMap::default(),
            hook_states: FxHashMap::default(),
            keyed_hook_states: FxHashMap::default(),
            snapshot: GeometrySnapshot::new(),
            watched: FxHashSet::default(),
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
    /// binds the same handle again. Takes the tree's packed element identity (`NodeId::packed`);
    /// the value is opaque here and unpacked by the publish pass.
    pub fn watch_geometry(&mut self, packed: u64) {
        self.watched.insert(packed);
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
    pub fn watched_geometry(&self) -> Vec<u64> {
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
        // LIFO reuse keeps hot slots in cache; the generation bump is what keeps
        // a handle from before the free standing in for the new occupant.
        if let Some(index) = self.free_slots.pop() {
            let slot = &mut self.values[index];
            debug_assert!(slot.value.is_none(), "free slot holding a value");
            slot.generation = slot.generation.wrapping_add(1);
            slot.value = Some(initial);
            SignalId::new(index, slot.generation)
        } else {
            let index = self.values.len();
            self.values.push(SignalSlot {
                value: Some(initial),
                generation: 0,
            });
            SignalId::new(index, 0)
        }
    }

    /// Returns the live value for an id, panicking on stale or disposed handles.
    ///
    /// Always on, not `debug_assert`: this check *is* the safety case for handles
    /// that outlive their runtime or their slot era. One predictable branch on a
    /// path already dominated by hashing (`track_read`) and downcasting.
    pub(crate) fn slot(&self, id: SignalId) -> &Box<dyn Any> {        match self.values.get(id.index) {
            Some(slot) if slot.generation == id.generation => slot
                .value
                .as_ref()
                .expect("SignalId names a live slot holding no value"),
            _ => panic!(
                "stale SignalId: slot {} is at generation {}, handle names {}",
                id.index,
                self.values.get(id.index).map(|s| s.generation).unwrap_or(u32::MAX),
                id.generation,
            ),
        }
    }

    /// Mutable twin of [`Self::slot`], same always-on validation.
    pub(crate) fn slot_mut(&mut self, id: SignalId) -> &mut Box<dyn Any> {
        match self.values.get_mut(id.index) {
            Some(slot) if slot.generation == id.generation => slot
                .value
                .as_mut()
                .expect("SignalId names a live slot holding no value"),
            _ => panic!(
                "stale SignalId: slot {} is at generation {}, handle names {}",
                id.index,
                self.values.get(id.index).map(|s| s.generation).unwrap_or(u32::MAX),
                id.generation,
            ),
        }
    }

    /// Disposes a plain signal slot, returning it to the free list.
    ///
    /// Mirrors [`Self::dispose_derived`] minus the derivation registries. Plain
    /// signals have no `Drop` (handles are `Copy`), so this is explicit — call it
    /// when the owner goes away. Un-disposed slots simply never get reused, which
    /// is today's behavior, not a regression.
    pub fn dispose_signal(&mut self, id: SignalId) {
        // Validate liveness first: double-dispose must fail here, not alias later.
        self.slot(id);
        self.subscribers.remove(&id);
        self.derived_dirty.remove(&id);
        self.motion_handoff.remove(&id);
        self.pending_roots.remove(&id);
        if let Some(slot) = self.values.get_mut(id.index) {
            slot.value = None;
            slot.generation = slot.generation.wrapping_add(1);
            self.free_slots.push(id.index);
        }
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
        if self.batch_depth == 1 && !self.pending_roots.is_empty() {
            // Outermost scope exiting: coalesce everything written inside
            // the batch into one walk + sweep. Depth is still above zero,
            // so notifications defer into `pending_subscribers` and the
            // single hook below preserves batch atomicity. Effect bodies
            // gathered here run at the same point they would have run for
            // an unbatched write: after the runtime borrow is released.
            let roots: Vec<SignalId> = self.pending_roots.drain().collect();
            let effects = self.invalidate_roots(&roots);
            // The runtime is unborrowed here (`invalidate_roots` takes
            // `&mut self` and returns), so running bodies is re-entrancy
            // safe on the same terms as an unbatched write.
            for effect_action in effects {
                effect_action();
            }
        }
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
        self.derived_recompute.remove(&id);
        self.motion_handoff.remove(&id);
        self.pending_roots.remove(&id);
        if let Some(sources) = self.derived_sources.remove(&id) {
            for src in sources {
                if let Some(set) = self.subscribers.get_mut(&src) {
                    set.remove(&Observer::Derived(id));
                }
            }
        }
        // Return the slot: derivation churn (long chains rebuilt per frame) is
        // exactly the workload a free list serves. Generation-guarded: a stale
        // id must never free its successor's slot.
        if let Some(slot) = self.values.get_mut(id.index) {
            if slot.generation == id.generation && slot.value.is_some() {
                slot.value = None;
                slot.generation = slot.generation.wrapping_add(1);
                self.free_slots.push(id.index);
            }
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
            if let Observer::Derived(d) = current {
                self.derived_sources.entry(d).or_default().insert(id);
            }
        }
    }

    pub fn read_signal<T: 'static>(&self, id: SignalId) -> &T {
        self.slot(id)
            .downcast_ref::<T>()
            .expect("Signal type mismatch")
    }

    /// Records a write for invalidation: immediately outside a batch,
    /// coalesced into `pending_roots` inside one.
    ///
    /// Returns effect bodies the caller must run after releasing the
    /// runtime borrow. Inside a batch the vector is empty — bodies run
    /// once, when the outermost scope exits.
    pub(crate) fn note_write(&mut self, id: SignalId) -> Vec<Rc<dyn Fn()>> {
        if self.batch_depth > 0 {
            self.pending_roots.insert(id);
            return Vec::new();
        }
        self.invalidate_roots(std::slice::from_ref(&id))
    }

    /// Marks every transitive derivation of `roots` dirty and notifies
    /// their subscribers, over one explicit work stack.
    ///
    /// Walk, flagging, and the observer snapshot fuse into a single pass
    /// with no per-node clones (`Observer` is `Copy`); notification and
    /// action gathering follow under one exclusive borrow. Returned bodies
    /// run after the caller's borrow is released.
    pub(crate) fn invalidate_roots(&mut self, roots: &[SignalId]) -> Vec<Rc<dyn Fn()>> {
        let mut visited = FxHashSet::default();
        let mut stack: Vec<SignalId> = Vec::new();
        let mut pending: smallvec::SmallVec<[Observer; 8]> = smallvec::SmallVec::new();
        for &root in roots {
            if visited.insert(root) {
                stack.push(root);
            }
        }
        while let Some(id) = stack.pop() {
            if let Some(subs) = self.subscribers.get(&id) {
                for ob in subs.iter().copied() {
                    match ob {
                        Observer::Derived(dep) => {
                            if visited.insert(dep) {
                                stack.push(dep);
                            }
                        }
                        _ => pending.push(ob),
                    }
                }
            }
            if self.derived_recompute.contains_key(&id) {
                self.derived_dirty.insert(id);
            }
        }

        let mut effects_to_run = Vec::new();
        for obs in pending {
            match obs {
                Observer::Subscriber(sub_id) => {
                    self.notify_subscriber(sub_id);
                    if let Some(action) = self.listeners.get(&sub_id) {
                        effects_to_run.push(Rc::clone(action));
                    }
                }
                Observer::Effect(eff_id) => {
                    if let Some(action) = self.effects.get(&eff_id) {
                        effects_to_run.push(Rc::clone(action));
                    }
                }
                Observer::Derived(_) => {}
            }
        }
        effects_to_run
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

    pub fn drain_dirty_subscribers(&mut self) -> FxHashSet<SubscriberId> {
        std::mem::take(&mut self.dirty_subscribers)
    }

    /// Returns skipped subscribers to the dirty set so the next drain pass retries them.
    pub fn requeue_dirty_subscribers(&mut self, subs: FxHashSet<SubscriberId>) {
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

impl Drop for ReactiveRuntime {
    fn drop(&mut self) {
        // The recompute registry owns closures that own further handles;
        // dropping them here would nest handle drops that re-enter this
        // half-dropped struct. In normal operation the registry is already
        // empty (every handle disposes on drop), so forgetting a leftover
        // only abandons teardown-time state the process is exiting anyway.
        if !self.derived_recompute.is_empty() {
            let leftover = std::mem::take(&mut self.derived_recompute);
            std::mem::forget(leftover);
        }
    }
}
