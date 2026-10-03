// Single responsibility: Retained invalidation bitflags, layout caches, and dynamic bindings.

use crate::foundation::{LayoutPresence, ResolvedRect, Transform};
use crate::input::handlers::NodeHandlers;
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
    /// Pointer interaction handlers declared by this node.
    ///
    /// Kept beside the bindings because both are per-node closures that survive
    /// reconciliation behind one pointer clone.
    pub handlers: NodeHandlers,
    /// Absolute screen-space bounding box committed during the last paint pass.
    ///
    /// Covers the union of where the box was drawn and where its content was laid out,
    /// so vacated pixels are always recoverable.
    pub last_painted_bounds: Option<ResolvedRect>,
    /// Absolute bounds of the content itself on the last paint pass.
    ///
    /// Tracked separately because content can change inside a stationary box: an
    /// accordion item that is fully open re-lays its text out while its render box
    /// happens to sit still, and those pixels still need repainting.
    pub last_painted_flow: Option<ResolvedRect>,
    /// Bounding box enclosing this node and all of its visual descendants.
    pub subtree_bounds: ResolvedRect,
    /// Pre-shaped text layout retained from layout pass (zero mutex locks during paint).
    pub cached_text_layout: Option<Arc<TextLayout>>,
    /// Retained Display List chunk for this node's stacking context (O(1) compile bypass).
    pub retained_chunk: Option<SceneChunk>,
    /// The transform this node was *declared* with, before pose components are folded in.
    ///
    /// `Node::transform` holds the composed value, so the declared part has to be kept
    /// separately: the reactive update path recomposes on every bound pose change and needs
    /// the declared transform to fold the pose into, not a value that already contains it.
    pub declared_transform: Transform,
    /// Layout footprint this node presents to its parent.
    pub presence: LayoutPresence,
    /// Keys of the children this node last reconciled.
    ///
    /// A node whose children come from reactive state is re-reconciled only when this
    /// sequence actually differs, so an unchanged list costs one comparison per frame.
    pub child_keys: Vec<Option<crate::foundation::Key>>,
}
