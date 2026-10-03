// Single responsibility: Retained invalidation bitflags, layout caches, and dynamic bindings.

use crate::foundation::{LayoutPresence, ResolvedRect};
use crate::motion::transition::Transition;
use crate::motion::MotionState;
use crate::scene::SceneChunk;
use crate::text::TextLayout;
use crate::tree::binding::NodeBindings;
use crate::tree::cache::LayoutCache;
use crate::tree::flags::DirtyFlags;
use std::sync::Arc;

/// Internal engine runtime state attached to a retained arena node.
#[derive(Default, Clone)]
pub struct NodeState {
    /// Active invalidation flags for this node.
    pub dirty: DirtyFlags,
    /// Cached layout calculations.
    pub cache: LayoutCache,
    /// Dynamic property bindings.
    pub bindings: NodeBindings,
    /// Absolute screen-space bounding box committed during the last paint pass.
    pub last_painted_bounds: Option<ResolvedRect>,
    /// Bounding box enclosing this node and all of its visual descendants.
    pub subtree_bounds: ResolvedRect,
    /// Pre-shaped text layout retained from layout pass (zero mutex locks during paint).
    pub cached_text_layout: Option<Arc<TextLayout>>,
    /// Retained Display List chunk for this node's stacking context (O(1) compile bypass).
    pub retained_chunk: Option<SceneChunk>,
    /// Compositor overrides layered above declarative style.
    pub motion: MotionState,
    /// Interpolation applied when a bound property of this node changes.
    pub transition: Option<Transition>,
    /// Interpolation applied to this node's children when their layout rects change.
    pub layout_transition: Option<Transition>,
    /// Layout footprint this node presents to its parent.
    pub presence: LayoutPresence,
}
