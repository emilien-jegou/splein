//! Frame passes between layout and rasterization: damage, compile, bounds.
//!
//! After [`crate::layout`] places boxes, each frame funnels through three passes, in this
//! order — the order is load-bearing, each pass consumes the previous one's output:
//!
//! 1. **Damage** ([`DamagePlan::compute`](damage::DamagePlan::compute)): which screen regions
//!    went stale? Compares each dirty node's visual bounds (AA pad, stroke, shadow included)
//!    against last frame's committed bounds. New paint is clipped to the active clip;
//!    old bounds stay unclipped so vacated pixels always clear. On resize, the preserved
//!    canvas rect is carved out so stationary content is never redamaged.
//! 2. **Compile** ([`CompileResult::compile`](compile::CompileResult::compile)): turns the
//!    retained tree into a [`Scene`](crate::scene::Scene) of chunks. Builds the stacking
//!    tree (z-index partitioned, stable-sorted so equal z resolves by tree order), culls
//!    offscreen nodes by *visual* bounds (a shadow reaching onscreen pulls its node back
//!    in), and replays retained chunks for clean subtrees (`O(1)` bypass).
//! 3. **Bounds** ([`commit_painted_bounds`]): writes every
//!    node's compiled screen-space bounds back into the arena, so next frame's damage has
//!    something to compare against.
//!
//! # Showcase
//!
//! You rarely call these directly — [`crate::runtime::Engine::frame`] runs them — but the
//! diagnostics they produce are the primary way to understand (and test) a frame:
//!
//! ```ignore
//! let report = engine.frame();
//! // What recomputed, in outermost-first order (the root itself is never listed):
//! assert_eq!(report.diagnostics.layout.recomputed_nodes.len(), 1);
//! // What went stale on screen this frame:
//! assert_eq!(report.diagnostics.spatial.damaged_rects.len(), 1);
//! // How many *drawing* commands were emitted (chunk framing excluded):
//! assert!(report.diagnostics.compile.commands_emitted > 0);
//! ```
//!
//! The `tests/pipeline_stage{1..7}_*.rs` suites pin each pass clause by clause —
//! invalidation classes, boundary isolation, damage clustering, overlap repaints,
//! compile culling, raster reuse, present buffer-ages.
//!
//! # Pitfalls
//!
//! - Damage is *conservative by design*: an entry that is merely touching still merges,
//!   and AA fringe is always covered. If your damage rects look "too big" by a pixel,
//!   that pixel is the antialiasing guarantee working.
//! - `commands_emitted` counts `Draw*` only. A frame that emits 5 commands for one red
//!   box is not broken — the other 4 are chunk push/pop framing, deliberately excluded.
//! - Offscreen culling uses visual bounds, not layout boxes: hiding a node whose shadow
//!   still reaches the viewport is a bug the culler specifically refuses to have.

pub mod bounds;
pub mod compile;
pub mod damage;

pub use bounds::commit_painted_bounds;
pub use compile::{compile_scene_instrumented, CompileResult};
pub use damage::DamagePlan;
