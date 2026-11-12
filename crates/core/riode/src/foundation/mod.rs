//! Shared vocabulary: value types with no behavior and no dependencies.
//!
//! Everything in the engine speaks this dialect. These are deliberately *dumb* types —
//! constructors, predicates, and tiny math, nothing that knows about trees, frames, or
//! backends. That ignorance is the point: because nothing here imports anything else,
//! every other module can (and does) depend on all of it without cycles.

pub mod alignment;
pub mod blend;
pub mod color;
pub mod constraints;
pub mod content;
pub mod edges;
pub mod effect;
pub mod geo;
pub mod geometry;
pub mod key;
pub mod layer_paint;
pub mod material;
pub mod overlay;
pub mod positioning;
pub mod presence;
pub mod shape;
pub mod sizing;
pub mod snapshot;
pub mod spacing;
pub mod transform;
pub mod surface;

pub use alignment::*;
pub use blend::*;
pub use color::*;
pub use constraints::*;
pub use content::*;
pub use edges::*;
pub use effect::*;
pub use geo::*;
pub use geometry::*;
pub use key::*;
pub use layer_paint::*;
pub use material::*;
pub use overlay::*;
pub use positioning::*;
pub use presence::*;
pub use shape::*;
pub use sizing::*;
pub use snapshot::*;
pub use spacing::*;
pub use transform::*;
pub use surface::*;
