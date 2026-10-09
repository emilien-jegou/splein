//! Shared vocabulary: value types with no behavior and no dependencies.
//!
//! Everything in the engine speaks this dialect. These are deliberately *dumb* types —
//! constructors, predicates, and tiny math, nothing that knows about trees, frames, or
//! backends. That ignorance is the point: because nothing here imports anything else,
//! every other module can (and does) depend on all of it without cycles.
//!
//! # What's inside
//!
//! | Concern | Types |
//! |---------|-------|
//! | Color & paint | [`Color`], [`Fill`], [`Stroke`], [`Radius`], [`Appearance`] |
//! | Geometry | [`Point`], [`ResolvedRect`], [`Transform`], [`Margin`], [`Gap`] |
//! | Layout intent | [`Size`], [`Constraints`], [`Alignment`], [`Direction`], [`Distribution`], [`LayoutPresence`] |
//! | Depth & light | [`Shadow`], [`LightEnvironment`], [`FalloffCurve`] |
//! | Identity & frames | [`Key`], [`GeometrySnapshot`] |
//!
//! # Showcase
//!
//! The whole module is designed to read like the sentence it describes:
//!
//! ```ignore
//! use aurora::prelude::*;
//!
//! group()
//!     .width(Size::fill())                    // intent, not pixels
//!     .height(Size::Fixed(480.0))
//!     .direction(Direction::Horizontal)       // main axis
//!     .alignment(Alignment::Center)           // cross axis
//!     .fill(Color::hex(0xF9FAFB))             // paint
//!     .radius(Radius::all(12.0))              // corners
//!     .shadow(Shadow::elevation(8.0))         // physics-based drop shadow…
//!     .shadow(Shadow::web_like(0.0, 4.0, 6.0, 0.0, Color::BLACK)) // …or web-exact
//!     .margin(Margin::symmetric(16.0, 8.0))
//! ```
//!
//! Chained `.margin()` calls *merge* per side (nonzero wins), so
//! `.margin(Margin::left(300.0)).margin(Margin::top(200.0))` composes instead of
//! the second call silently discarding the first.
//!
//! # Rules for adding to this module
//!
//! 1. **Value semantics**: `Copy` (or cheap `Clone`), `PartialEq`, `Debug`. If it needs
//!    an arena, a runtime, or I/O, it lives elsewhere.
//! 2. **No imports** from any other `aurora` module — not even siblings in spirit.
//!    `smallvec` is the only allowed third-party dependency.
//! 3. **Constructors over setters**: `Shadow::elevation(8.0)`, `Margin::symmetric(..)`,
//!    `Color::hex(..)` read at the call site; a bag of `set_*` does not.

pub mod alignment;
pub mod appearance;
pub mod color;
pub mod constraints;
pub mod content;
pub mod fill;
pub mod falloff;
pub mod geometry;
pub mod key;
pub mod light;
pub mod positioning;
pub mod presence;
pub mod radius;
pub mod shadow;
pub mod sizing;
pub mod snapshot;
pub mod spacing;
pub mod stroke;
pub mod transform;

pub use alignment::*;
pub use appearance::*;
pub use color::*;
pub use constraints::*;
pub use content::*;
pub use falloff::*;
pub use fill::*;
pub use geometry::*;
pub use key::*;
pub use light::*;
pub use positioning::*;
pub use presence::*;
pub use radius::*;
pub use shadow::*;
pub use sizing::*;
pub use snapshot::*;
pub use spacing::*;
pub use stroke::*;
pub use transform::*;
