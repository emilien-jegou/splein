// Single responsibility: scoped context values (provide/use) for the component tree.
//
// A context is a named value channel from a provider subtree to the consumers
// inside it — the answer to prop drilling, and specifically to values that
// cannot be threaded as arguments (list rows, which builders hand only
// `(index, item)`; deeply nested chrome).
//
// Why markers and not scope lookup: component children are built EAGERLY —
// `<Provider><Card/></Provider>` runs `Card()` before `Provider()` runs — so
// no provider scope is ever live when a consumer builds. Resolution is
// therefore tree-structural (ancestor walk at evaluation) rather than
// scope-structural. Values flow through `Signal`s, so every read subscribes
// through the existing observer paths with no new invalidation machinery.
//
// The one rule: context resolves during node evaluation (props, text, pose),
// where a node is known. Component bodies, effects, and `derived!` have no
// node — `get()` there yields a lazy marker, never a value. Transform markers
// with `.map()`; bind them into props; read resolved signals in handlers.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;

use crate::reactive::signal::Signal;
use crate::tree::{NodeId, TreeArena};

/// Identity of one context object: the `file!():line!():column!()` of its
/// `initContext!()` call site. Unique per call, stable per process. Two
/// handles over the same value type coexist — identity is the object, not
/// the type.
pub type CtxId = &'static str;

/// Declares a context object. Call once per context, at module scope:
///
/// ```ignore
/// static Theme: Context<Theme> = initContext!();
/// ```
///
/// A macro (not a function) so the identity is const-compatible with zero
/// runtime cost and no `LazyLock` boilerplate: the call-site position names
/// the context.
#[macro_export]
macro_rules! initContext {
    () => {
        $crate::reactive::context::Context::with_id(::std::concat!(
            ::std::file!(),
            ":",
            ::std::line!(),
            ":",
            ::std::column!()
        ))
    };
}

/// A declared context channel over values of type `T`.
#[derive(Clone, Copy, Debug)]
pub struct Context<T> {
    id: CtxId,
    _marker: PhantomData<T>,
}

impl<T> Context<T> {
    /// Builds a handle for a static identity. Prefer `initContext!()`.
    pub const fn with_id(id: CtxId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Reads this context where it is provided, nearest enclosing provider
    /// wins. Returns a lazy marker (see module docs): bind it into props or
    /// transform it with `.map()` — never readable eagerly, because a
    /// component body has no node yet.
    ///
    /// Missing provider panics at evaluation with the context identity; the
    /// reconciler catches per flagged node into a placeholder plus a console
    /// error, so one unprovided consumer cannot abort the frame.
    pub fn get(&self) -> Ctx<T>
    where
        T: 'static,
    {
        Ctx {
            id: self.id,
            ops: Vec::new(),
            fallback: None,
            _marker: PhantomData,
        }
    }

    /// Reads with an eager fallback value when unprovided. The fallback
    /// replaces the whole chain (missing behaves exactly as if the value
    /// were the default) and never panics.
    pub fn get_or(&self, default: T) -> Ctx<T>
    where
        T: Clone + 'static,
    {
        Ctx {
            id: self.id,
            ops: Vec::new(),
            fallback: Some(Rc::new(move || {
                let v: T = default.clone();
                let b: Box<dyn Any> = Box::new(v);
                b
            })),
            _marker: PhantomData,
        }
    }

    /// Reads with a lazily computed fallback, evaluated at most... eagerly at
    /// setup: the closure runs once when `get_or_else` is called (component
    /// build is rare; per-eval laziness would re-run it every frame).
    pub fn get_or_else(&self, default: impl Fn() -> T + 'static) -> Ctx<T>
    where
        T: 'static,
    {
        Ctx {
            id: self.id,
            ops: Vec::new(),
            fallback: Some(Rc::new(move || {
                let v: T = default();
                let b: Box<dyn Any> = Box::new(v);
                b
            })),
            _marker: PhantomData,
        }
    }

    /// Bundles this context with a value for a `Provider`: the only way to
    /// establish a value, so handle and signal types can never mismatch —
    /// a `Signal<Theme>` cannot be provided under a `Context<Border>`.
    /// Erased at construction (generics known there), so providers stay
    /// concrete over heterogeneous grants.
    pub fn provide(&self, value: Signal<T>) -> Provided
    where
        T: Clone + 'static,
    {
        let signal = value;
        Provided {
            id: self.id,
            reader: Rc::new(move || {
                let v: T = signal.get();
                let b: Box<dyn Any> = Box::new(v);
                b
            }),
        }
    }
}

/// A context/value bundle: the single argument a `Provider` takes.
/// Constructible only through [`Context::provide`]; erased so one
/// `Provider` component serves every context.
#[derive(Clone)]
pub struct Provided {
    id: CtxId,
    reader: ProviderValue,
}

impl Provided {
    pub(crate) fn id(&self) -> CtxId {
        self.id
    }

    pub(crate) fn reader(&self) -> ProviderValue {
        Rc::clone(&self.reader)
    }
}

/// A provided value, type-erased at the node boundary. Built where generics
/// are known (provide site), applied where only dynamics flow.
pub type ProviderValue = Rc<dyn Fn() -> Box<dyn Any>>;

/// One erased map step in a marker chain: `Box<T_in>` to `Box<T_out>`.
/// Construction enforces the types; application is blind but sound.
pub type CtxOp = Rc<dyn Fn(Box<dyn Any>) -> Box<dyn Any>>;

/// Lazy fallback producer (already target-chain-relative, see below).
pub type CtxFallback = Rc<dyn Fn() -> Box<dyn Any>>;

/// A lazily-resolved context read: identity plus a transform chain.
///
/// Markers are data, never values: they compose (`.map()`), convert into
/// props (`IntoPropValue`), and resolve during node evaluation against the
/// retained ancestor chain. Reading one anywhere else (component bodies,
/// effects, `derived!`) is meaningless — there is no node — so no such API
/// exists, on purpose. Transform markers; bind them; read resolved signals
/// in handlers.
#[derive(Clone)]
pub struct Ctx<T> {
    id: CtxId,
    ops: Vec<CtxOp>,
    /// Replacement for the whole chain when unprovided. `.map()` replays
    /// over it exactly like a resolved value ("missing behaves as if the
    /// value were the default"), so ordering of `map` vs `get_or` never matters.
    fallback: Option<CtxFallback>,
    _marker: PhantomData<T>,
}

impl<T: 'static> Ctx<T> {
    /// Transforms the resolved value. Pure value function: runs at each
    /// evaluation under the ambient observer, so signal reads inside `f`
    /// subscribe like any other binding. Composes arbitrarily; the chain is
    /// checked statically (`Ctx<Theme>` → `Ctx<Color>`), applied blindly.
    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + 'static) -> Ctx<U> {
        let mut ops = self.ops;
        ops.push(Rc::new(move |boxed: Box<dyn Any>| {
            let input: T = *boxed
                .downcast::<T>()
                .expect("riode: context op-chain type mismatch (unreachable by construction)");
            let out: U = f(input);
            let b: Box<dyn Any> = Box::new(out);
            b
        }));
        Ctx {
            id: self.id,
            ops,
            fallback: self.fallback,
            _marker: PhantomData,
        }
    }

    /// Resolves to a value now. Reads the ambient evaluation node (set by
    /// reconcile, scheduler rebuilds, and handler dispatch) and walks its
    /// retained ancestors for the nearest provider of this context.
    ///
    /// Panics outside node evaluation and on missing providers without
    /// fallback; both are loud by design (reconcile catches the latter per
    /// flagged node into a placeholder plus a console error).
    pub(crate) fn resolve(&self) -> T
    where
        T: Clone,
    {
        let (node, arena) = current().expect(
            "riode: context read outside node evaluation — bind markers into props \
             (or transform them with `.map()`); component bodies and effects have no node",
        );
        // SAFETY: reads only, through a pointer derived from a borrow that is
        // live-but-dormant for the whole region (single-threaded sequential
        // dispatch/evaluation; handlers receive no arena access, so nothing
        // reachable can write through the outstanding borrow while we read).
        // Set and cleared by RAII guards at reconcile, scheduler rebuilds,
        // and handler dispatch; nesting saves and restores.
        let arena: &TreeArena = unsafe { &*arena };
        // Descent first: freshly mounted links don't exist in the retained
        // tree yet (children reconcile before set_children links them), but
        // the lexical provider chain is live on the stack. Innermost wins.
        if let Some(reader) = provided_in_descent(self.id) {
            return apply_ops(reader(), &self.ops);
        }
        let mut cursor = Some(node);
        while let Some(n) = cursor {
            if let Some(reader) = provided(arena, n, self.id) {
                return apply_ops(reader(), &self.ops);
            }
            cursor = arena.parent(n);
        }
        if let Some(fallback) = &self.fallback {
            return apply_ops(fallback(), &self.ops);
        }
        panic!(
            "riode: no provider for context `{}` above the reading node; \
             wrap the subtree in a Provider with a grant from provide(), or use get_or",
            self.id
        );
    }

    /// A resolving closure for retained bindings: same resolution on every
    /// call, so scheduler rebuilds track provider updates.
    pub(crate) fn resolving_closure(&self) -> Rc<dyn Fn() -> T>
    where
        T: Clone,
    {
        let marker = self.clone();
        Rc::new(move || marker.resolve())
    }
}

/// Applies an op chain to a produced value, downcasting to the marker type.
/// Construction enforces the chain, so a mismatch here is an engine bug.
fn apply_ops<T: 'static>(mut boxed: Box<dyn Any>, ops: &[CtxOp]) -> T {
    for op in ops {
        boxed = op(boxed);
    }
    *boxed
        .downcast::<T>()
        .expect("riode: context op-chain output mismatch (unreachable by construction)")
}

// Lexical provider chain for the reconcile descent in progress: entries
// pushed as provided-carrying nodes reconcile their children, popped after.
// Stateless across frames (nothing retained); shadowing by innermost-first
// scan. Covers mount, where retained links don't exist yet.
thread_local! {
    static DESCENT: RefCell<Vec<(CtxId, ProviderValue)>> = const { RefCell::new(Vec::new()) };
}

/// Pushes entries for a descent region; pops exactly that many on drop.
/// Panic-safe by construction (Drop restores the length, so a missing-provider
/// unwind can't strand entries for the next subtree).
pub(crate) struct DescentGuard {
    depth: usize,
}

/// Enters a provider descent region for the duration of one node's children
/// reconciliation. Empty providers push nothing and cost one branch.
pub(crate) fn enter_descent(entries: &[(CtxId, ProviderValue)]) -> DescentGuard {
    let depth = DESCENT.with(|d| {
        let mut stack = d.borrow_mut();
        let depth = stack.len();
        stack.extend(entries.iter().map(|(id, v)| (*id, Rc::clone(v))));
        depth
    });
    DescentGuard { depth }
}

impl Drop for DescentGuard {
    fn drop(&mut self) {
        DESCENT.with(|d| d.borrow_mut().truncate(self.depth));
    }
}

/// Innermost descent entry for a context, if the current reconcile descent
/// passes through one of its providers.
fn provided_in_descent(id: CtxId) -> Option<ProviderValue> {
    DESCENT.with(|d| {
        d.borrow()
            .iter()
            .rev()
            .find(|(key, _)| *key == id)
            .map(|(_, v)| Rc::clone(v))
    })
}

/// Provided values stored per retained node (see `LayoutNode::provided`).
fn provided(arena: &TreeArena, node: NodeId, id: CtxId) -> Option<ProviderValue> {
    let stored = arena.get(node).state.core.provided.as_ref()?;
    stored
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, v)| Rc::clone(v))
}

// The node currently under evaluation, plus the arena to walk from.
thread_local! {
    /// Set by RAII guards at reconcile (per node), scheduler rebuilds, and
    /// handler dispatch; always restored, never leaked across regions.
    static CURRENT: Cell<Option<(NodeId, *const TreeArena)>> = const { Cell::new(None) };
}
/// Installs an evaluation node; restores the previous one on drop (regions nest:
/// scheduler rebuilds reconcile children, handlers fire inside dispatch).
pub(crate) struct CurrentGuard {
    prev: Option<(NodeId, *const TreeArena)>,
}

/// Enters node evaluation context. Callers must hold a borrow of `arena`
/// covering the guard's lifetime (all three call sites do: reconcile holds
/// `&mut`, scheduler holds `&mut` via self, dispatch holds a shared reborrow).
pub(crate) fn enter_node(id: NodeId, arena: &TreeArena) -> CurrentGuard {
    let prev = CURRENT.replace(Some((id, arena as *const TreeArena)));
    CurrentGuard { prev }
}

impl Drop for CurrentGuard {
    fn drop(&mut self) {
        CURRENT.set(self.prev.take());
    }
}

fn current() -> Option<(NodeId, *const TreeArena)> {
    CURRENT.with(|c| c.get())
}
