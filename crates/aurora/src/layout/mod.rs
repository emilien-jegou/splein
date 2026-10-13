//! Flexbox layout: from size intents to placed boxes.
//!
//! Every node declares an *intent* ([`Size`](crate::foundation::Size) pixels,
//! `Fit` content, `Fill` leftover, `Percent` ratio); this module
//! resolves intents into `resolved_rect`s top-down, then reports what it did in a
//! [`LayoutResult`]. It never paints and never reads pixels — text is measured through
//! shaping callbacks, not rasterized.
//!
//! # How a frame lays out
//!
//! 1. **Measure** (`measure`, `group_measure`, `min_size`): bottom-up intrinsic sizes —
//!    how big each subtree *wants* to be, including text measured at candidate widths
//!    and CSS `min-*: auto` shrink floors.
//! 2. **Allocate** (`allocate_main`, `allocate_cross`, `allocate_absolute`): top-down
//!    distribution of the parent's box along the main axis (flex grow/shrink with margin
//!    preservation), cross-axis alignment, and anchored absolute children.
//! 3. **Report** (`report`): every recomputed node and every cache hit lands in
//!    [`LayoutResult`], which is what the boundary/isolation tests assert on.
//!
//! The entry point is [`layout_node_with_text`]; the
//! scheduler calls it once for a full pass or once per layout boundary for isolated
//! passes (see [`crate::runtime::scheduler`]).
//!
//! # Showcase
//!
//! Layout is declarative — you state intents, the engine places boxes:
//!
//! ```ignore
//! use aurora::prelude::*;
//!
//! group()
//!     .direction(Direction::Horizontal)   // main axis: x
//!     .width(600.0)
//!     .height(100.0)
//!     .gap(8.0)                           // fixed spacing between children
//!     .alignment(Alignment::Center)       // cross axis: y
//!     .children([
//!         group().width(50.0).height(100.0),          // fixed: exactly 50
//!         group().width(Size::fill()).height(100.0),  // fill: 600 − 50 − 8
//!         group().width(Size::fit()).height(100.0)    // fit: content width
//!             .children([text("hello").size(14.0)]),
//!     ])
//! ```
//!
//! # Caching contract
//!
//! A node whose constraints and inputs are unchanged is served from
//! [`LayoutCache`](crate::tree::LayoutCache) without remeasuring — the `record_cached`
//! entries in [`LayoutResult`]. Fixed-size subtrees are layout *boundaries*: dirt inside
//! them never escapes, so a counter ticking in one card can't relayout the page. Tests in
//! `tests/pipeline_stage2_layout.rs` pin every clause of this contract.
//!
//! # Pitfalls
//!
//! - `Fill` inside an unbounded (`Fit`) parent resolves against infinity and clamps —
//!   usually to zero. A fill child that vanishes almost always has an unbounded ancestor.
//! - Text measurement needs a *final* width before heights are derived; the engine
//!   resolves cross sizes first for exactly this reason. Custom [`Primitive`](crate::tree::Primitive)s must
//!   honor the same order in `measure` or wrapping text above them mismeasures.

pub mod allocate_absolute;
pub mod allocate_cross;
pub mod allocate_main;
pub mod engine;
pub mod frozen;
pub mod group_measure;
pub mod measure;
pub mod min_size;
pub mod report;
pub mod wrap;

pub use engine::{layout_node, layout_node_with_text};
pub use frozen::FrozenLayout;
pub use report::LayoutResult;
