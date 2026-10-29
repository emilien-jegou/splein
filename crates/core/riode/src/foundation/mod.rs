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
//! | Color & paint | [`Color`], [`Fill`], [`Appearance`] |
//! | Geometry | [`Point`], [`ResolvedRect`], [`Transform`], [`Margin`], [`Gap`] |
//! | Layout intent | [`Size`], [`Constraints`], [`Alignment`], [`Direction`], [`Distribution`], [`LayoutPresence`] |
//! | Depth & light | [`LightEnvironment`], [`FalloffCurve`] |
//! | Identity & frames | [`Key`], [`GeometrySnapshot`] |
//!
//! # Showcase
//!
//! The whole module is designed to read like the sentence it describes:
//!
//! ```ignore
//! use riode::prelude::*;
//!
//! group()
//!     .width(Size::fill())                    // intent, not pixels
//!     .height(Size::Fixed(480.0))
//!     .direction(Direction::Horizontal)       // main axis
//!     .alignment(Alignment::Center)           // cross axis
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
//! 2. **No imports** from any other `riode` module — not even siblings in spirit.
//!    `smallvec` is the only allowed third-party dependency.
//! 3. **Constructors over setters**: `Shadow::elevation(8.0)`, `Margin::symmetric(..)`,
//!    `Color::hex(..)` read at the call site; a bag of `set_*` does not.

pub mod alignment;
pub mod appearance;
pub mod color;
pub mod constraints;
pub mod blend;
pub mod content;
pub mod edges;
pub mod effect;
pub mod falloff;
pub mod fill;
pub mod filter;
pub mod geo;
pub mod geometry;
pub mod key;
pub mod light;
pub mod literal;
pub mod positioning;
pub mod presence;
pub mod shading;
pub mod sizing;
pub mod snapshot;
pub mod spacing;
pub mod transform;

pub use alignment::*;
pub use appearance::*;
pub use blend::*;
pub use color::*;
pub use constraints::*;
pub use content::*;
pub use edges::*;
pub use effect::*;
pub use falloff::*;
pub use fill::*;
pub use filter::*;
pub use geo::*;
pub use geometry::*;
pub use key::*;
pub use light::*;
pub use literal::*;
pub use positioning::*;
pub use presence::*;
pub use shading::*;
pub use sizing::*;
pub use snapshot::*;
pub use spacing::*;
pub use transform::*;
