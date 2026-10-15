// Single responsibility: Component-scoped allocation of signals, clocks, timers, and playbacks.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustc_hash::FxHashSet;

use crate::anim::transition::{ControllerHandle, Transition, TransitionCell};
use crate::reactive::computed::Computed;
use crate::reactive::effect::Effect;
use crate::reactive::readable::Readable;
use crate::reactive::runtime::ReactiveRuntime;
use crate::reactive::signal::Signal;
use crate::reactive::ticker::Ticker;

thread_local! {
    /// Context stack every component body runs inside, so hooks need no plumbing.
    static CURRENT: RefCell<Vec<HookScope>> = const { RefCell::new(Vec::new()) };

    /// Source of identities for scopes nobody named.
    ///
    /// A scope with no declared identity still needs a distinct one, or two sibling
    /// components sharing a name would share their hook slots and hand each other their
    /// state. Nothing can look state up by such an identity later, which is the point:
    /// preserving state across a rebuild requires a declared identity.
    static ANON_SEQ: Cell<usize> = const { Cell::new(0) };
}

/// One entry of the ambient hook scope stack.
///
/// `#[doc(hidden)]` cross-crate plumbing for `riode-dsl` (FLIP hooks allocate hook
/// state through these); not part of the app-facing API.
#[doc(hidden)]
pub struct HookScope {
    pub(crate) inner: Rc<RefCell<Inner>>,
    /// Identity this scope's hook state is filed under.
    pub(crate) instance: Rc<str>,
    /// Next hook slot to hand out. Reset per scope, so a component's hooks are numbered
    /// from zero every time its body runs.
    next_slot: Cell<usize>,
    /// Anonymous scopes opened directly inside this one, used to name nested components.
    children: Cell<usize>,
    /// Call-site hook keys already claimed during this build.
    ///
    /// Reset per scope entry, so the set answers "was this key used twice in one
    /// body run" rather than accumulating across rebuilds. Ordinal hooks do not
    /// consult it; keyed hooks (`use_signal!`) panic on a second claim with the
    /// file location, turning hook-in-a-loop aliasing into a located error.
    keyed_used: RefCell<FxHashSet<&'static str>>,
    /// Whether this identity is reproducible, and so whether state filed under it can be
    /// recalled by a later build.
    ///
    ///
    /// Only a scope entered with [`Cx::enter_keyed`] is stable. An anonymous scope derives a
    /// stable identity *only* when it sits inside one, so a component nested in a keyed row
    /// is also reproducible. An anonymous scope at the top is not, which is deliberate: its
    /// identity cannot be reproduced, and pretending otherwise would make state carry over
    /// by position — exactly the bug that makes a reordered list hand each row the wrong
    /// history.
    pub(crate) stable: bool,
}

impl HookScope {
    /// Whether state filed under this scope's identity survives rebuilds.
    ///
    /// `#[doc(hidden)]` cross-crate read for `riode-dsl` hook authors; not app API.
    #[doc(hidden)]
    pub fn is_stable(&self) -> bool {
        self.stable
    }

    /// The runtime this scope allocates hook state on.
    ///
    /// `#[doc(hidden)]` cross-crate read for `riode-dsl` hook authors; not app API.
    #[doc(hidden)]
    pub fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        Rc::clone(&self.inner.borrow().runtime)
    }
}

/// Handle to the ambient component context.
///
/// Hooks allocate owned state here rather than reaching into the engine, so a component
/// declares its reactive state with this one argument and never touches the scheduler.
///
/// A context names a *runtime*, not an instance: every signal a body allocates lands on the
/// same graph the engine drains, which is what lets those signals drive nodes without the
/// component holding a reference to either.
#[derive(Clone)]
pub struct Cx(Rc<RefCell<Inner>>);

/// State shared by every clone of a [`Cx`].
pub(crate) struct Inner {
    pub(crate) runtime: Rc<RefCell<ReactiveRuntime>>,
    frame_signal: RefCell<Option<Signal<f64>>>,
}

impl Cx {
    /// Builds a context over a runtime.
    pub fn new(runtime: Rc<RefCell<ReactiveRuntime>>) -> Self {
        Self(Rc::new(RefCell::new(Inner {
            runtime,
            frame_signal: RefCell::new(None),
        })))
    }

    /// Runs `body` with this context installed as the ambient one.
    ///
    /// The scope is anonymous, so hook state it allocates is reachable only for as long as
    /// the body runs. Use [`Cx::enter_keyed`] where the body may be re-run and its state has
    /// to survive.
    pub fn enter<R>(&self, body: impl FnOnce() -> R) -> R {
        // Nested inside a keyed component, this component's identity is reproducible from
        // its parent's, so its hooks are too. Otherwise it gets a fresh identity and its
        // state is not carried across builds.
        let (instance, stable) = CURRENT.with(|stack| {
            let mut stack = stack.borrow_mut();
            match stack.last_mut() {
                Some(parent) if parent.stable => {
                    let n = parent.children.get();
                    parent.children.set(n + 1);
                    let mut name = String::with_capacity(parent.instance.len() + 8);
                    name.push_str(parent.instance.as_ref());
                    name.push('#');
                    name.push_str(&n.to_string());
                    (Rc::from(name), true)
                }
                _ => {
                    let seq = ANON_SEQ.with(|c| {
                        let n = c.get();
                        c.set(n + 1);
                        n
                    });
                    (Rc::from(format!("anon#{seq}")), false)
                }
            }
        });
        self.enter_scope(instance, stable, body)
    }

    /// Runs `body` as a component instance named `path`, so its hook state survives a rebuild.
    ///
    /// This is what makes a component inside a reactive child list keep its state: the list is
    /// rebuilt by re-running its builder, and without a name the rebuild allocates fresh
    /// signals and the component silently starts over. `key` must identify the *instance*,
    /// not its position — a list that reorders must key by item identity, or a component
    /// would inherit the state of whichever item took its index.
    pub fn enter_keyed<R>(&self, key: impl AsRef<str>, body: impl FnOnce() -> R) -> R {
        self.enter_scope(Rc::from(key.as_ref()), true, body)
    }

    /// Runs `body` as a named component, combining [`Cx::enter_keyed`] stability
    /// with [`Cx::enter`] sibling disambiguation. This is what `#[component]`
    /// expands to (with `path` = `module_path!()::Name`).
    ///
    /// Identity rules, in order:
    /// - Under a stable parent: `{parent}#{child_index}={path}` — positional
    ///   like `enter` (so same-name siblings never alias), stable like
    ///   `enter_keyed`. The path suffix is debuggability, not identity.
    /// - At the root (no parent): `path` verbatim — the component declares its
    ///   own stability, which cascades positionally to its whole subtree.
    /// - Under an *unstable* parent: `{path}#{child_index}`, unique within this
    ///   build but not recalled later (an unstable parent promises nothing).
    pub fn enter_named<R>(&self, path: &'static str, body: impl FnOnce() -> R) -> R {
        let (instance, stable) = CURRENT.with(|stack| {
            let mut stack = stack.borrow_mut();
            match stack.last_mut() {
                Some(parent) if parent.stable => {
                    let n = parent.children.get();
                    parent.children.set(n + 1);
                    (Rc::from(format!("{}#{n}={path}", parent.instance)), true)
                }
                Some(parent) => {
                    let n = parent.children.get();
                    parent.children.set(n + 1);
                    (Rc::from(format!("{path}#{n}")), false)
                }
                None => (Rc::from(path), true),
            }
        });
        self.enter_scope(instance, stable, body)
    }

    fn enter_scope<R>(&self, instance: Rc<str>, stable: bool, body: impl FnOnce() -> R) -> R {
        CURRENT.with(|stack| {
            stack.borrow_mut().push(HookScope {
                inner: Rc::clone(&self.0),
                instance,
                next_slot: Cell::new(0),
                children: Cell::new(0),
                keyed_used: RefCell::new(FxHashSet::default()),
                stable,
            })
        });
        // Restoring on unwind matters: a component body that panics must not leave a stale
        // context installed, or every later hook would allocate onto the wrong runtime.
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        CURRENT.with(|stack| {
            stack.borrow_mut().pop();
        });
        match out {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// The ambient context, if a component body is running.
    pub fn current() -> Option<Cx> {
        CURRENT.with(|stack| {
            stack
                .borrow()
                .last()
                .map(|scope| Cx(Rc::clone(&scope.inner)))
        })
    }

    /// The ambient context, panicking outside a component body.
    ///
    /// This is what the `use_*` hooks call, so they read like the documented API while a
    /// component that prefers explicit plumbing can take a `Cx` parameter instead.
    pub fn expect_current() -> Cx {
        Cx::current().expect("hook called outside a component body; take a `Cx` argument instead")
    }

    /// Allocates a signal in this context.
    pub fn signal<T: Clone + 'static>(&self, initial: T) -> Signal<T> {
        Signal::new(self.0.borrow().runtime.clone(), initial)
    }

    /// Derives a memoized signal computed from other signals.
    pub fn computed<T: Clone + 'static>(&self, compute: impl Fn() -> T + 'static) -> Computed<T> {
        Computed::new(self.0.borrow().runtime.clone(), compute)
    }

    /// Signal holding monotonic seconds, advanced once per frame and shared per context.
    pub fn frame_clock(&self) -> Signal<f64> {
        if let Some(existing) = self.0.borrow().frame_signal.borrow().clone() {
            return existing;
        }
        let runtime = self.0.borrow().runtime.clone();
        let out = Signal::new(runtime.clone(), 0.0f64);
        runtime.borrow_mut().register_ticker(Box::new(FrameClock {
            out: out.clone(),
            elapsed: 0.0,
        }));
        *self.0.borrow().frame_signal.borrow_mut() = Some(out.clone());
        out
    }

    /// Runs `action` every `seconds`, starting after one full interval.
    pub fn interval(&self, seconds: f32, action: impl Fn() + 'static) {
        self.register_timer(seconds, action, true);
    }

    /// Runs `action` once after `seconds`.
    pub fn timeout(&self, seconds: f32, action: impl Fn() + 'static) {
        self.register_timer(seconds, action, false);
    }

    fn register_timer(&self, seconds: f32, action: impl Fn() + 'static, repeating: bool) {
        let ticker = Timer {
            period: seconds.max(f32::EPSILON),
            elapsed: 0.0,
            action: Rc::new(action),
            repeating,
        };
        self.0
            .borrow()
            .runtime
            .borrow_mut()
            .register_ticker(Box::new(ticker));
    }

    /// Signal alternating between `false` and `true` every `seconds`.
    pub fn interval_toggle(&self, seconds: f32) -> Signal<bool> {
        let out = self.signal(false);
        let handle = out.clone();
        self.interval(seconds, move || {
            let next = !handle.get();
            handle.set(next);
        });
        out
    }

    /// Starts a self-driving playback, returning its value and a control handle.
    pub fn playback(
        &self,
        transition: Transition,
        from: f32,
        to: f32,
    ) -> (Signal<f32>, ControllerHandle) {
        let runtime = self.0.borrow().runtime.clone();
        transition.playback(&runtime, from, to)
    }
}

/// The innermost ambient scope, or `None` outside a component body.
#[doc(hidden)]
pub fn current_scope() -> Option<HookScope> {
    CURRENT.with(|stack| {
        stack.borrow().last().map(|scope| HookScope {
            inner: Rc::clone(&scope.inner),
            instance: Rc::clone(&scope.instance),
            next_slot: Cell::new(scope.next_slot.get()),
            children: Cell::new(scope.children.get()),
            keyed_used: RefCell::new(FxHashSet::default()),
            stable: scope.stable,
        })
    })
}

#[doc(hidden)]
pub fn no_scope() -> ! {
    panic!("hook called outside a component body; take a `Cx` argument instead")
}

/// Allocates the next hook slot for the ambient scope.
///
/// The returned slot index is how a rebuild finds the state it allocated last time.
///
/// This advances the counter on the live scope sitting on the stack — not on the snapshot
/// `current_scope` hands out. Bumping the snapshot would increment a throwaway copy, handing
/// every hook in a body slot 0: values filed by different hooks would collide, recall would
/// miss by type on every rebuild, and each rebuild would allocate fresh state (a FLIP that
/// teleports, a counter that resets, a ticker registry that grows without bound).
#[doc(hidden)]
pub fn next_slot(scope: &HookScope) -> usize {
    CURRENT.with(|stack| {
        let stack = stack.borrow();
        match stack.last() {
            Some(live) => {
                debug_assert_eq!(
                    live.instance, scope.instance,
                    "the ambient scope changed between current_scope and next_slot"
                );
                let slot = live.next_slot.get();
                live.next_slot.set(slot + 1);
                slot
            }
            None => {
                // No live scope (body already returned); fall back to the snapshot so the
                // call still yields a deterministic slot rather than panicking.
                let slot = scope.next_slot.get();
                scope.next_slot.set(slot + 1);
                slot
            }
        }
    })
}

/// Returns the signal this instance already allocated at `slot`, if the type still matches.
///
/// The type check is what makes a changed hook signature fail loudly at the first rebuild
/// instead of handing back a signal of the wrong shape and panicking somewhere unrelated.
fn recall<T: Clone + 'static>(scope: &HookScope, slot: usize) -> Option<Signal<T>> {
    recall_value(scope, slot)
}

fn remember<T: Clone + 'static>(scope: &HookScope, slot: usize, signal: &Signal<T>) {
    remember_value(scope, slot, signal);
}

/// Returns whatever this instance filed at `slot`, if the type still matches.
///
/// Not restricted to signals: a hook's state does not have to be one. A transition hook files
/// the cell that owns its spring and its ticker, and a FLIP hook files a handle plus the
/// position it last saw, because those are the things that have to survive a rebuild.
///
/// The type check is what makes a changed hook signature fail loudly at the first rebuild
/// instead of handing back a signal of the wrong shape and panicking somewhere unrelated.
#[doc(hidden)]
pub fn recall_value<T: Clone + 'static>(scope: &HookScope, slot: usize) -> Option<T> {
    let runtime = Rc::clone(&scope.inner.borrow().runtime);
    let borrow = runtime.borrow();
    let states = borrow.hook_states.get(&scope.instance)?;
    let stored = states.get(slot)?.as_ref()?;
    stored.downcast_ref::<T>().map(|value| (*value).clone())
}

/// Files a value under this instance's `slot`.
#[doc(hidden)]
pub fn remember_value<T: Clone + 'static>(scope: &HookScope, slot: usize, value: &T) {
    let runtime = Rc::clone(&scope.inner.borrow().runtime);
    let mut borrow = runtime.borrow_mut();
    let states = borrow
        .hook_states
        .entry(Rc::clone(&scope.instance))
        .or_default();
    if states.len() <= slot {
        states.resize_with(slot + 1, || None);
    }
    states[slot] = Some(Rc::new(value.clone()) as Rc<dyn Any>);
}

/// Drops the hook state of a keyed row that left the tree, and everything nested inside it.
///
/// React semantics: a keyed item that unmounts takes its state with it; re-adding it mounts
/// a fresh instance rather than resurrecting the old one. State that should survive a round
/// trip out of the list belongs in props, not in the component's own hook memory.
///
/// Dropping the slot is also what disarms the row's effects: an effect cell owns its
/// `Effect`, so losing the last `Rc` runs the cell's teardown and disposes the subscriber.
/// The cells are therefore collected first and dropped *after* the runtime borrow is
/// released — a teardown may read or write signals, and `Effect::drop` disposes through this
/// same runtime.
///
/// `root` is the `enter_keyed` scope name (the `key_of` value). Descendants are filed under
/// `{root}#{…}`, so the match is exact-or-`{root}#`-prefixed, never a bare string prefix:
/// a row keyed `"a"` must not sweep a sibling keyed `"ab"`.
pub(crate) fn drop_instance_subtree_state(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    root: &str,
) {
    let prefix = format!("{root}#");
    let removed = {
        let mut rt = runtime.borrow_mut();
        let doomed: Vec<Rc<str>> = rt
            .hook_states
            .keys()
            .filter(|k| k.as_ref() == root || k.as_ref().starts_with(&prefix))
            .cloned()
            .collect();
        let mut taken = Vec::with_capacity(doomed.len());
        for key in doomed {
            if let Some(states) = rt.hook_states.remove(&key) {
                taken.push(states);
            }
            rt.keyed_hook_states.remove(&key);
        }
        taken
    };
    // Outside the borrow: each dropped cell runs its teardown, and its `Effect` disposes
    // itself through the runtime.
    drop(removed);
}

/// Publishes elapsed seconds into a signal once per frame.
struct FrameClock {
    out: Signal<f64>,
    elapsed: f64,
}

impl Ticker for FrameClock {
    fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt.max(0.0) as f64;
        self.out.set_if_changed(self.elapsed);
        true
    }
}

/// Fires an action on a fixed period.
struct Timer {
    period: f32,
    elapsed: f32,
    action: Rc<dyn Fn()>,
    repeating: bool,
}

impl Ticker for Timer {
    fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        if self.elapsed < self.period {
            return true;
        }
        // Subtract rather than clear, so a long frame does not lose accumulated periods.
        self.elapsed -= self.period;
        (self.action)();
        self.repeating
    }

    fn is_active(&self) -> bool {
        self.repeating
    }
}

/// Signal of monotonic seconds, driven by the frame clock.
pub fn use_frame_clock() -> Signal<f64> {
    Cx::expect_current().frame_clock()
}

/// Signal alternating every `seconds`.
pub fn use_interval_toggle(seconds: f32) -> Signal<bool> {
    Cx::expect_current().interval_toggle(seconds)
}

/// Runs `action` every `seconds`.
pub fn use_interval(seconds: f32, action: impl Fn() + 'static) {
    Cx::expect_current().interval(seconds, action);
}

/// Runs `action` once after `seconds`.
pub fn use_timeout(seconds: f32, action: impl Fn() + 'static) {
    Cx::expect_current().timeout(seconds, action);
}

/// Allocates a signal in the ambient component context, recalled by call order (ordinal).
///
/// This is the *internal fast path*: the hook is identified as "the Nth hook that ran", so a
/// build that skips or reorders hooks shifts every later slot and hands each one the wrong
/// state. **Prefer [`use_signal!`](crate::use_signal)** for anything that can reorder, sit
/// behind a conditional, or follow an early return — that is the default recommendation.
/// Reach for ordinal `use_signal` only when the hook's position is fixed for the
/// component's whole life (top of the body, unconditional).
///
/// Inside a scope entered with [`Cx::enter_keyed`], the second call in the same component
/// returns the same signal as the first call did on the previous build, so the component's
/// state survives being rebuilt. `initial` is only used the first time.
pub fn use_signal<T: Clone + 'static>(initial: T) -> Signal<T> {
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let slot = next_slot(&scope);
    // Recalling from an unreproducible identity could only ever hit by coincidence, and when
    // it did it would hand a component someone else's state.
    if scope.stable {
        if let Some(existing) = recall::<T>(&scope, slot) {
            return existing;
        }
    }
    let out = Signal::new(Rc::clone(&scope.inner.borrow().runtime), initial);
    if scope.stable {
        remember(&scope, slot, &out);
    }
    out
}

/// Allocates a signal keyed by call-site identity rather than call order — the **default**
/// recommendation for any hook that can reorder, sit behind a conditional, or follow an
/// early return.
///
/// Unlike ordinal [`use_signal`], which recalls by "the Nth hook that ran", this recalls by
/// `key` — so a hook inside a conditional or after an early return keeps its state even on
/// builds where an earlier hook is skipped. Prefer the [`use_signal!`](crate::use_signal)
/// macro, which namespaces the key with `file!()` so you write only the name.
///
/// Stability still requires a reproducible scope ([`Cx::enter_keyed`]), exactly like
/// ordinal hooks. A changed type on an existing key panics naming both types,
/// instead of silently allocating fresh state and resetting the component.
pub fn use_signal_keyed<T: Clone + 'static>(key: &'static str, initial: T) -> Signal<T> {
    let Some(scope) = current_scope() else {
        no_scope()
    };
    claim_key(&scope, key);
    // Recalling from an unreproducible identity could only ever hit by coincidence,
    // and when it did it would hand a component someone else's state.
    if scope.stable {
        if let Some(existing) = recall_keyed::<Signal<T>>(&scope, key) {
            return existing;
        }
    }
    let out = Signal::new(Rc::clone(&scope.inner.borrow().runtime), initial);
    if scope.stable {
        remember_keyed(&scope, key, &out);
    }
    out
}

/// Claims a call-site key for this build, panicking on a second claim.
///
/// A keyed hook used twice in one body run — a copy-paste collision, or a hook in
/// a loop where every iteration shares the call site — would otherwise alias one
/// slot across two owners. Failing here, naming the key, is the whole point of
/// keyed identity: aliasing that ordinal hooks suffer silently.
fn claim_key(scope: &HookScope, key: &'static str) {
    CURRENT.with(|stack| {
        let stack = stack.borrow();
        // The snapshot `current_scope` hands out carries a throwaway set; the claim
        // has to land on the live scope or the second claim would never see the first.
        match stack.last() {
            Some(live) => {
                debug_assert_eq!(
                    live.instance, scope.instance,
                    "the ambient scope changed between current_scope and claim_key"
                );
                if !live.keyed_used.borrow_mut().insert(key) {
                    panic!(
                        "use_signal!: duplicate hook key `{key}` in one component build; \
                         keys must be unique per component (rename one, or index hooks \
                         called in a loop)"
                    );
                }
            }
            None => {
                // No live scope (body already returned); record on the snapshot so the
                // call still has deterministic behavior rather than panicking.
                if !scope.keyed_used.borrow_mut().insert(key) {
                    panic!(
                        "use_signal!: duplicate hook key `{key}` in one component build; \
                         keys must be unique per component (rename one, or index hooks \
                         called in a loop)"
                    );
                }
            }
        }
    })
}

/// Returns the signal this instance filed under `key`, if the type still matches.
///
/// A type mismatch panics naming both types: under keyed identity a changed hook
/// signature is an edit to be told about, not a miss to silently reallocate on
/// (which is what would reset the component without warning).
fn recall_keyed<T: Clone + 'static>(scope: &HookScope, key: &'static str) -> Option<T> {
    use crate::reactive::runtime::KeyedSlot;
    let runtime = Rc::clone(&scope.inner.borrow().runtime);
    let borrow = runtime.borrow();
    let slots = borrow.keyed_hook_states.get(&scope.instance)?;
    let slot: &KeyedSlot = slots.get(key)?;
    match slot.value.downcast_ref::<T>() {
        Some(value) => Some(value.clone()),
        None => panic!(
            "use_signal!: hook `{key}` changed type (was {}, now {}); \
             update the hook or rename the key",
            slot.type_name,
            std::any::type_name::<T>(),
        ),
    }
}

/// Files a value under this instance's call-site `key`.
fn remember_keyed<T: Clone + 'static>(scope: &HookScope, key: &'static str, value: &T) {
    use crate::reactive::runtime::KeyedSlot;
    let runtime = Rc::clone(&scope.inner.borrow().runtime);
    let mut borrow = runtime.borrow_mut();
    let slots = borrow
        .keyed_hook_states
        .entry(Rc::clone(&scope.instance))
        .or_default();
    slots.insert(
        key,
        KeyedSlot {
            value: Rc::new(value.clone()) as Rc<dyn Any>,
            type_name: std::any::type_name::<T>(),
        },
    );
}

/// Allocates a signal with call-site identity — the **default** way to hold component state.
///
/// The key must be an explicit literal namespaced by file — stable across
/// moves, unlike the removed `file!():line!()` derivation, which silently
/// reset the hook's state every time the line shifted. Reach for this over
/// ordinal [`use_signal`] whenever the hook can sit inside a conditional, follow
/// an early return, or live in a list that reorders — builds that skip an earlier
/// hook still recall correctly. Ordinal [`use_signal`] is the fast path for a
/// hook whose position is fixed for the component's whole life.
///
/// The key must be unique per component build: a hook in a loop shares its
/// call site across iterations and panics naming the key, as does a pasted
/// duplicate. Use keyed lists for repeated hooks.
#[macro_export]
macro_rules! use_signal {
    ($key:literal, $init:expr) => {
        $crate::reactive::use_signal_keyed(concat!(file!(), "#", $key), $init)
    };
}

/// Creates a named [`Computed`] handle from a plain closure.
///
/// The closure reads signals with `.get()` and the runtime auto-tracks every read,
/// so the derivation recomputes whenever any signal it touched changes. This is
/// the named-handle counterpart to inline `DynamicProp` closures: use this when
/// you need to store the derivation in a struct, pass it as a prop, or share it
/// across multiple binding sites.
///
/// ```ignore
/// let is_held = computed!(move || drag.held.get().as_deref() == Some(label.as_str()));
/// ```
///
/// The closure must be `'static` — clone any captured `String`/`Vec` by hand.
/// Prefer this over nested `.map().combine()` chains when the derivation reads
/// more than two signals.
#[macro_export]
macro_rules! computed {
    ($body:expr) => {
        $crate::reactive::Cx::expect_current().computed($body)
    };
}

/// Starts a self-driving playback in the ambient component context.
pub fn use_playback(transition: Transition, from: f32, to: f32) -> (Signal<f32>, ControllerHandle) {
    Cx::expect_current().playback(transition, from, to)
}

/// A transition whose animated value survives being rebuilt.
///
/// [`Transition::on`] seeds its output at the target's current value. That is right for a
/// one-shot and wrong for a row in a reactive list: the list re-runs its builder whenever its
/// order changes, so a plain `on` hands the rebuilt row a brand-new spring already sitting on its
/// new slot. `animated - current` is then zero on the frame the move lands, and an animation that
/// looks like it should be a FLIP is a teleport instead.
///
/// This recalls the cell the component used last time, so the row keeps its position *and* its
/// velocity across the rebuild and simply retargets. The source is swapped every build, because
/// the derived handed in is itself a new object each time.
///
/// Outside a keyed scope there is nothing to recall, so this behaves exactly like `on` rather
/// than panicking — an anonymous component's state was never going to survive anyway.
pub fn use_transition(transition: Transition, target: impl Readable<f32> + 'static) -> Signal<f32> {
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let slot = next_slot(&scope);

    if scope.stable {
        if let Some(cell) = recall_value::<Rc<TransitionCell>>(&scope, slot) {
            let reader: Rc<dyn Readable<f32>> =
                Rc::from(Box::new(target) as Box<dyn Readable<f32>>);
            cell.retarget_source(reader);
            return cell.out();
        }
    }

    let cell = transition.cell(target);
    // Attached once per hook slot, for the life of the component. A ticker registered per
    // rebuild would never be retired — `advance` returns true unconditionally, since the target
    // may move again at any time — so the registry would grow on every list change.
    cell.out()
        .attach(Box::new(TransitionTicker(Rc::clone(&cell))));
    if scope.stable {
        remember_value(&scope, slot, &cell);
    }
    cell.out()
}

/// Drives one recalled transition once per frame.
struct TransitionTicker(Rc<TransitionCell>);

impl Ticker for TransitionTicker {
    fn advance(&mut self, dt: f32) -> bool {
        self.0.advance(dt);
        true
    }

    fn is_active(&self) -> bool {
        !self.0.settled()
    }
}

/// What an effect body returns: a one-shot that undoes its run.
pub type Teardown = Box<dyn FnOnce()>;

/// Hook state for [`use_effect`]: the body, the teardown of the run in flight, and the
/// [`Effect`] that drives it.
///
/// The cell — not a local — owns the `Effect`, for the same reason `use_flip` stores its
/// effect in a cell: an effect held only for the duration of the hook call disposes itself
/// on drop, subscribing for exactly one build and never firing again. A `Rc` so
/// `recall_value`/`remember_value` can share it across rebuilds.
struct EffectCell {
    body: Box<dyn FnMut() -> Teardown>,
    teardown: Option<Teardown>,
    effect: Option<Effect>,
}

impl Drop for EffectCell {
    fn drop(&mut self) {
        // Runs before the fields below, so teardown fires before `Effect::drop` disposes the
        // subscriber. This *is* the unmount path: a keyed row leaving the tree has its hook
        // state dropped (`drop_instance_subtree_state`), the last `Rc` to this cell goes, and
        // the pending teardown runs here.
        if let Some(t) = self.teardown.take() {
            t();
        }
    }
}

/// Subscribes `cell`'s body to the reactive drain, replacing whatever effect it already held.
///
/// The stored action holds only a `Weak` back to the cell — a strong one would be
/// `cell → effect → action → cell`, an uncollectable cycle that leaks every effect on every
/// rebuild.
///
/// The action runs the *previous* teardown before the next body run, so a rebuild that
/// re-arms the effect does explicit cleanup + setup rather than piling a second registration
/// on top of the first. `Effect::new` runs this action once at creation, which is what arms
/// the mount/rebuild run.
fn arm_effect(cell: &Rc<RefCell<EffectCell>>, scope: &HookScope) {
    let weak = Rc::downgrade(cell);
    let fresh = Effect::new(scope.runtime(), move || {
        let Some(cell) = weak.upgrade() else {
            return;
        };
        let mut c = cell.borrow_mut();
        if let Some(t) = c.teardown.take() {
            t();
        }
        let td = (c.body)();
        c.teardown = Some(td);
    });
    // Replaced rather than asserted absent, exactly as `use_flip::set_effect` does it: a
    // rebuild recalls the same cell and hands it a fresh effect, and the old one must be
    // disposed rather than left subscribed.
    if let Some(mut previous) = cell.borrow_mut().effect.take() {
        previous.dispose();
    }
    cell.borrow_mut().effect = Some(fresh);
}

/// Runs `body` on mount and whenever its tracked reads change, and owns the teardown of each
/// run.
///
/// `body` returns a one-shot that undoes it. That teardown runs before the *next* run of
/// `body` (so a rebuild does explicit cleanup + setup instead of overwriting), and once more
/// when the component unmounts: a keyed row leaving a list has its hook state dropped, the
/// cell falls out, and the pending teardown fires. React semantics throughout — unmount
/// destroys state, re-adding the same key mounts a fresh instance.
///
/// What it absorbs is the "insert into the registry on rebuild" half of the `motion_reorder`
/// mirror-signal hack: registration and its unregister ride the same teardown, so a rebuilt
/// row re-registers exactly once instead of stacking a new callback per rebuild (24 drags
/// took that list from 10 entries to 50), and a removed row takes its callback with it.
///
/// ```ignore
/// use_effect(move || {
///     let id = register_post_layout(mirror);
///     move || unregister(id)
/// });
/// ```
///
/// A body that has nothing to undo returns a no-op: `move || {}`.
///
/// The effect re-runs on every rebuild of a *stable* scope — that is how a body that began
/// reading a different signal re-subscribes to it, since read tracking happens when the
/// effect runs. (In an anonymous scope there is nothing to recall, so state is not filed and
/// the effect lives only for the duration of the call; put effects in a `#[component]` or a
/// keyed scope.)
pub fn use_effect<T, F>(mut body: F)
where
    F: FnMut() -> T + 'static,
    T: FnOnce() + 'static,
{
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let slot = next_slot(&scope);

    let fresh: Box<dyn FnMut() -> Teardown> = Box::new(move || Box::new(body()) as Teardown);

    match scope
        .is_stable()
        .then(|| recall_value::<Rc<RefCell<EffectCell>>>(&scope, slot))
        .flatten()
    {
        Some(cell) => {
            // Swap the body only; `arm_effect` below runs it (after the old teardown) by
            // way of `Effect::new`'s one-time action.
            cell.borrow_mut().body = fresh;
            arm_effect(&cell, &scope);
        }
        None => {
            let cell = Rc::new(RefCell::new(EffectCell {
                body: fresh,
                teardown: None,
                effect: None,
            }));
            arm_effect(&cell, &scope);
            if scope.is_stable() {
                remember_value(&scope, slot, &cell);
            }
        }
    }
}

/// [`use_effect`]'s post-layout slot: `frame` runs once per frame, after layout has produced
/// the boxes and before the frame is compiled.
///
/// The phase is in the name rather than smuggled in as a magic registration, because the two
/// slots have different lifetimes: the effect body runs once per mount/rebuild, `frame` runs
/// every frame until it is torn down. Registration and its unregister ride the same teardown,
/// so a rebuilt row re-registers exactly once instead of stacking a new callback per rebuild,
/// and a removed row takes its callback with it — the explicit version of the "insert into
/// the registry on rebuild" pattern this hook exists to absorb.
///
/// ```ignore
/// use_effect_post_layout(move |dt| {
///     displayed_sig.set_if_changed(displayed.get());
/// });
/// ```
pub fn use_effect_post_layout(mut frame: impl FnMut(f32) + 'static) {
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let runtime = scope.runtime();
    // `frame` outlives any one registration (a rebuild makes a fresh one), so it is shared
    // rather than moved into the first callback the body hands to the registry.
    let frame = Rc::new(RefCell::new(frame));

    use_effect(move || {
        let body = Rc::clone(&frame);
        let id = runtime
            .borrow()
            .register_post_layout(move |dt| body.borrow_mut()(dt));
        let rt = Rc::clone(&runtime);
        move || {
            rt.borrow().unregister_post_layout(id);
        }
    });
}
