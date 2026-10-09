//! Reactive engine: signals, derivations, effects, tickers — no UI knowledge.
//!
//! This layer is deliberately free of any UI knowledge: it owns the dependency graph and
//! nothing else. Animation adapts to it ([`crate::anim`]); the UI tree consumes it
//! ([`crate::dsl`], [`crate::runtime`]). A `Signal<bool>` here works identically whether
//! it drives a pixel, a layout width, or a test assertion — that indifference is the
//! whole design.
//!
//! # The vocabulary
//!
//! - [`Signal<T>`]: a copyable handle to a versioned value. `.get()` tracks the read
//!   (inside a derivation, effect, or binding); `.set()` writes and invalidates
//!   dependents. `Copy` is load-bearing: handles are meant to be cloned into closures
//!   freely, and cross-engine misuse fails loudly via runtime epochs instead of
//!   aliasing the wrong graph.
//! - [`Derived<T>`]: a memoized computation. Lazy, cached, dependency-tracked —
//!   `signal.map(|v| ...)` is the one-line form.
//! - [`Effect`]: a subscriber that *runs* when its dependencies change (as opposed to a
//!   derivation, which *recomputes* when read).
//! - [`Ticker`]: frame-driven work. `advance(dt)` returns whether to keep being asked;
//!   returning `false` retires it. This is how animations request frames without the
//!   engine knowing what animation is.
//! - [`batch()`]: wraps a burst of writes so the graph walks once, not once per write.
//!
//! # Showcase
//!
//! ```ignore
//! use aurora::prelude::*;
//! use std::{cell::RefCell, rc::Rc};
//!
//! let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
//! let cx = Cx::new(Rc::clone(&rt));
//! let view = cx.enter(|| {
//!     let count = use_signal(0);
//!     let doubled = count.map(|c| c * 2);          // Derived: recomputes on read
//!     Effect::new(&cx, move || {                   // Effect: runs on write
//!         println!("count is now {}", count.get());
//!     });
//!     batch(&rt, || { count.set(1); count.set(2); }); // one invalidation, not two
//!     doubled
//! });
//! assert_eq!(view.get(), 4);
//! ```
//!
//! Inside a `#[component]` body you never write this scaffolding — the ambient scope
//! (`Cx::expect_current`) makes `use_signal(0)` just work, with state filed per
//! component instance and recalled across rebuilds.
//!
//! # Hooks for time
//!
//! [`hooks::use_frame_clock`], [`hooks::use_interval`] / [`hooks::use_timeout`],
//! [`hooks::use_interval_toggle`], and [`hooks::use_playback`] / [`hooks::use_transition`]
//! allocate clocks, timers, and animation adapters in component scope. See
//! [`crate::anim`] for which motion flavor to pick.
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`signal`] / [`derived`] / [`effect`] | The three primitives |
//! | [`runtime`] | [`ReactiveRuntime`]: the store, plus [`runtime::tick`] and [`runtime::run_post_layout`] frame drivers |
//! | [`hooks`] | [`Cx`] + all `use_*` hooks + the `use_signal!` / `derived!` macros |
//! | [`prop`] / [`readable`] | [`DynamicProp`]/[`IntoProp`], [`Readable`]/[`SignalRead`]: binding-facing adapters |
//! | [`ticker`] / [`mod@batch`] / [`ambient`] / [`observer`] / [`id`] | Plumbing: ticker registry, batching, scope guards, observer edges, id types |
//!
//! # Pitfalls
//!
//! - Reading a signal outside any tracking context just returns the value — no
//!   subscription is recorded. If your derivation "never updates", its `.get()` ran
//!   where nobody was listening.
//! - `Effect::new` runs its action once at creation. If that surprises you mid-debug,
//!   it is working as designed (and why FLIP cells store their `Effect` instead of
//!   dropping it).
//! - Deep chains can't overflow the stack: writes refresh transitive derivations in
//!   dependency-first order without re-entering user handles.

pub mod batch;
pub mod ambient;
pub mod derived;
pub mod effect;
pub mod hooks;
pub mod id;
pub mod observer;
pub mod prop;
pub mod readable;
pub mod runtime;
pub mod signal;
pub mod ticker;

pub use ambient::{guard as ambient_guard, AmbientGuard};
pub use batch::batch;
pub use derived::Derived;
pub use effect::Effect;
pub use hooks::{
    use_frame_clock, use_interval, use_interval_toggle, use_playback, use_signal,
    use_signal_keyed, use_timeout, use_transition, Cx,
};
pub use id::{SignalId, SubscriberId};
pub use observer::Observer;
pub use prop::{computed, DynamicProp, IntoProp};
pub use readable::{Readable, SignalRead};
pub use runtime::{run_post_layout, tick, ReactiveRuntime};
pub use signal::Signal;
pub use ticker::Ticker;
