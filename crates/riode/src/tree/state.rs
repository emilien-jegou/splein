// Single responsibility: Retained per-node state, grouped by owning domain.
//
// `NodeState` used to be one flat bag of fields from five domains, so every pass touched
// every other pass's state through the same struct. The groups below draw the ownership
// lines the crate split follows: each group moves to its domain crate with the code that
// reads and writes it, while the tree keeps the arena and the hierarchy.

use crate::foundation::{Key, LayoutPresence, LightEnvironment, ResolvedRect, Transform};
use crate::input::handlers::NodeHandlers;
use crate::scene::SceneChunk;
use crate::text::selection::{SelectionGranularity, TextSelection};
use crate::text::TextLayout;
use crate::tree::binding::NodeBindings;
use crate::tree::cache::LayoutCache;
use crate::tree::flags::DirtyFlags;
use std::sync::Arc;

/// Invalidation flags, layout caches, bindings, and identity: owned by the tree itself.
#[derive(Default, Clone)]
pub struct CoreState {
    /// Active invalidation flags for this node.
    pub dirty: DirtyFlags,
    /// Cached layout calculations.
    pub cache: LayoutCache,
    /// Dynamic property bindings.
    pub bindings: NodeBindings,
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
    pub child_keys: Vec<Option<Key>>,
    /// Ambient light resolved for this node during the damage traversal:
    /// the nearest ancestor override wins, else the global default.
    ///
    /// Written outside reconciliation (which never sees it, so overrides
    /// never dirty unaffected nodes) and read by paint, bounds, and hit
    /// testing. Stale at most until the next damage walk, which always
    /// precedes the paint that consumes it.
    pub resolved_light: LightEnvironment,
    /// Context values this node provides to its subtree: `(context id,
    /// erased signal reader)` pairs, nearest entry wins on lookup.
    ///
    /// Written by reconciliation from the node's declaration (so conditional
    /// providers appear and vanish with their nodes — no leaks, no stale
    /// entries), read by context resolution walks. `None` (the common case)
    /// costs one tag and is skipped outright.
    pub provided: Option<
        Vec<(
            crate::reactive::context::CtxId,
            crate::reactive::context::ProviderValue,
        )>,
    >,
    /// Memoized static-subtree proof [S2-2]: `Some(true)` iff this node has
    /// no live bindings, provides no context, and every child is static.
    /// `None` means unproven (recomputed bottom-up by the damage walk).
    /// Cleared on the node + ancestors whenever bindings, providers, or
    /// structure change (see `TreeArena::invalidate_static_up`).
    pub subtree_static: Option<bool>,
}

/// Pointer interaction state: owned by the input domain.
#[derive(Default, Clone)]
pub struct InputState {
    /// Pointer interaction handlers declared by this node.
    ///
    /// Kept beside the bindings because both are per-node closures that survive
    /// reconciliation behind one pointer clone.
    pub handlers: NodeHandlers,
    /// Whether this node owns keyboard focus for selection editing.
    ///
    /// Only a focused collapsed selection paints a caret; blur hides it while
    /// keeping the range for the selection API.
    pub text_focused: bool,
}

/// Shaped text and selection state: owned by the text domain.
#[derive(Default, Clone)]
pub struct TextState {
    /// Pre-shaped text layout retained from layout pass (zero mutex locks during paint).
    pub cached_text_layout: Option<Arc<TextLayout>>,
    /// Ephemeral mouse selection for selectable text (readonly MVP).
    ///
    /// Owned by the engine's pointer router, not by reconciliation: a drag
    /// writes it per sample and only dirties paint, never layout. Cleared when
    /// the content changes or a press lands elsewhere.
    pub text_selection: Option<TextSelection>,
    /// Click-count granularity latched for the active selection gesture.
    pub selection_granularity: SelectionGranularity,
}

/// Retained paint output: owned by the scene domain.
#[derive(Default, Clone)]
pub struct SceneState {
    /// Retained Display List chunk for this node's stacking context (O(1) compile bypass).
    pub retained_chunk: Option<SceneChunk>,
}

/// Committed paint bounds: owned by the pipeline domain.
#[derive(Default, Clone)]
pub struct PaintState {
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
}

/// Internal engine runtime state attached to a retained arena node.
#[derive(Default, Clone)]
pub struct NodeState {
    pub core: CoreState,
    pub input: InputState,
    pub text: TextState,
    pub scene: SceneState,
    pub paint: PaintState,
}
