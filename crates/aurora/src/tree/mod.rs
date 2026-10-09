//! Retained tree: the arena of nodes every pass reads and writes.
//!
//! The tree is the engine's shared memory. The reconciler builds it, layout places it,
//! damage/compile read it, input mutates slices of it — so its shape *is* the coupling
//! map of the whole crate. Read this module when any pass misbehaves; the bug is usually
//! a field here being written by the wrong pass or read at the wrong time.
//!
//! # The pieces
//!
//! - [`TreeArena`]: generational arena (`insert`/`remove`, hierarchy, key lookup,
//!   absolute transforms). `NodeId` is index + generation — stale handles fail loudly
//!   instead of aliasing recycled slots.
//! - [`LayoutNode`]: style + geometry + [`NodeState`]. `resolved_rect` is the layout
//!   box (parent space); `absolute_rect` the stored screen box; `effective_transform`
//!   folds declared + pose + compositor overrides.
//! - [`NodeState`]: per-node runtime state, **grouped by owning domain** —
//!   [`CoreState`] (dirty/cache/bindings/presence/keys/light: the tree's own),
//!   [`InputState`] (handlers/focus), [`TextState`] (shaped layout/selection),
//!   [`SceneState`] (retained chunk), [`PaintState`] (committed bounds).
//!   Each group moves with the code that owns it if this ever splits again.
//! - [`DirtyFlags`]: `MEASURE` (reshape), `LAYOUT` (replace), `PAINT` (repaint),
//!   `INTERACTION` (handlers), `SUBTREE_DIRTY` (descendant changed). Fill/opacity/shadow
//!   are paint-only; text content is measure; structure goes through reconciliation.
//! - [`mark_node_dirty`]: upward invalidation to layout boundaries. Prefer it over
//!   touching flags by hand — it maintains the boundary protocol.
//!
//! # Showcase
//!
//! ```ignore
//! use aurora::prelude::*;
//! use aurora::runtime::Engine;
//! use aurora::tree::{DirtyFlags, NodeId};
//!
//! let mut engine = Engine::headless(400, 300);
//! engine.mount(group().width(400.0).height(300.0).key("root"));
//! engine.frame();
//!
//! // Read geometry through the query API (preferred over reaching into the arena):
//! let r = engine.rect("root").expect("mounted");
//! assert_eq!((r.width, r.height), (400.0, 300.0));
//!
//! // Dirty by hand only when no signal owns the change (tests, tooling):
//! engine.with_arena(|arena, root| {
//!     arena.mark_dirty(root, DirtyFlags::PAINT);
//! });
//! ```
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`arena`] | [`TreeArena`], hierarchy, key registry lookups, absolute math |
//! | [`node`] | [`LayoutNode`], [`PaintLayer`], stacking/opacity/transform queries |
//! | [`state`] | [`NodeState`] + the five domain groups |
//! | [`style`] | [`NodeStyle`]: width/height/layout/margin/appearance/z/hit-test |
//! | [`binding`] | [`NodeBindings`]: reactive property evaluators (width/pose/fill/…) |
//! | [`kind`] | [`NodeKind`] (Group/Text/Custom) + [`Primitive`] trait |
//! | [`flags`] / [`invalidation`] / [`cache`] / [`registry`] / [`router`] / [`id`] | [`DirtyFlags`], [`mark_node_dirty`], [`LayoutCache`], [`KeyRegistry`], [`SubscriberRouter`], [`NodeId`] |
//!
//! # Pitfalls
//!
//! - `with_arena` hands out the *whole* arena including `get_mut`. It exists for
//!   diagnostics and tests; passes must grow narrow methods instead.
//! - `SUBTREE_DIRTY` propagates upward; `PAINT` does not need layout. Marking `LAYOUT`
//!   when only paint changed forces a full pass for nothing — check
//!   `tests/pipeline_stage1_invalidation.rs` for the class table.
//! - Cached text layouts (`TextState::cached_text_layout`) are shared `Arc`s: clearing
//!   one node's cache must not mutate the layout another node still reads.

pub mod arena;
pub mod binding;
pub mod cache;
pub mod flags;
pub mod id;
pub mod invalidation;
pub mod kind;
pub mod node;
pub mod registry;
pub mod router;
pub mod state;
pub mod style;

pub use arena::TreeArena;
pub use binding::NodeBindings;
pub use cache::LayoutCache;
pub use flags::DirtyFlags;
pub use id::NodeId;
pub use invalidation::mark_node_dirty;
pub use kind::{NodeKind, Primitive};
pub use node::{LayoutNode, PaintLayer, OVERLAY_Z_FLOOR};
pub use registry::KeyRegistry;
pub use router::SubscriberRouter;
pub use state::{CoreState, InputState, NodeState, PaintState, SceneState, TextState};
pub use style::NodeStyle;
