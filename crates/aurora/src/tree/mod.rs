// Single responsibility: Retained tree arena, node models, styles, and invalidation flags.

pub mod arena;
pub mod binding;
pub mod cache;
pub mod flags;
pub mod id;
pub mod invalidation;
pub mod kind;
pub mod node;
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
pub use node::LayoutNode;
pub use router::SubscriberRouter;
pub use state::NodeState;
pub use style::NodeStyle;
