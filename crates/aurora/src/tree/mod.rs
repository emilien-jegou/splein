// Single responsibility: Retained tree arena and node models; primitives re-exported from aurora-tree.

pub mod arena;
pub mod binding;
pub mod invalidation;
pub mod node;
pub mod state;

pub use arena::TreeArena;
pub use aurora_tree::{
    cache, flags, id, kind, registry, router, style, DirtyFlags, KeyRegistry, LayoutCache,
    NodeId, NodeKind, NodeStyle, Primitive, SubscriberRouter,
};
pub use binding::NodeBindings;
pub use invalidation::mark_node_dirty;
pub use node::{LayoutNode, PaintLayer, OVERLAY_Z_FLOOR};
pub use state::{CoreState, InputState, NodeState, PaintState, SceneState, TextState};
