// Single responsibility: Retained tree primitives: ids, flags, caches, styles, node kinds.
//
// The arena, node layout state, and bindings stay in the facade until the input and
// pipeline domains extract: they compose state from those domains.

pub mod cache;
pub mod flags;
pub mod id;
pub mod kind;
pub mod registry;
pub mod router;
pub mod style;

pub use cache::LayoutCache;
pub use flags::DirtyFlags;
pub use id::NodeId;
pub use kind::{NodeKind, Primitive};
pub use registry::KeyRegistry;
pub use router::SubscriberRouter;
pub use style::NodeStyle;
