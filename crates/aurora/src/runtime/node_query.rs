// Single responsibility: Key-based reads of retained node geometry.

use aurora_foundation::{Key, Point, ResolvedRect, Transform};
use crate::runtime::FrameScheduler;
use crate::tree::{NodeId};
use crate::tree::{TreeArena};

/// Read-only node queries addressed by key.
///
/// Split out of `Engine` because these are one question -- "where is the view with this key, and
/// what shape is it in" -- asked repeatedly, with the coordinate space being the part that
/// matters. Mixing parent-local, render-box, and absolute answers into the same facade is what
/// made them easy to confuse; keeping them together here is what makes the distinction readable.
///
/// Every query is defined in one coordinate space and says so:
///
/// * [`NodeQuery::flow_rect`] -- the laid-out box, in the parent's space. What layout produces.
/// * [`NodeQuery::render_rect`] -- the box the compositor draws, equal to the flow box until
///   layout motion pulls it away, and the box a hit test follows.
/// * [`NodeQuery::absolute_rect`] -- the box in window coordinates, projected through the
///   accumulated transform. What a cursor or an overlay needs.
pub struct NodeQuery<'a> {
    scheduler: &'a FrameScheduler,
}

impl<'a> NodeQuery<'a> {
    /// Creates a query surface over a scheduler's retained tree.
    pub fn new(scheduler: &'a FrameScheduler) -> Self {
        Self { scheduler }
    }

    /// Resolves a key to its node.
    pub fn id(&self, key: impl Into<Key>) -> Option<NodeId> {
        self.scheduler.arena.node_for_key(&key.into())
    }

    /// The laid-out box, in the parent's coordinate space.
    pub fn flow_rect(&self, key: impl Into<Key>) -> Option<ResolvedRect> {
        let id = self.id(key)?;
        Some(self.scheduler.arena.get(id).resolved_rect)
    }

    /// The box the node occupies, which is what a hit test follows.
    ///
    /// Identical to [`Self::flow_rect`]: there is no longer a separate animated box, so the two
    /// coordinate questions that used to need distinguishing have collapsed into one.
    pub fn render_rect(&self, key: impl Into<Key>) -> Option<ResolvedRect> {
        self.flow_rect(key)
    }

    /// The composed transform the view is painted with: declared style, reactive pose
    /// components, and any compositor override.
    ///
    /// This is what to compare against when a view is driven by an animated transform rather
    /// than by layout.
    pub fn transform(&self, key: impl Into<Key>) -> Option<Transform> {
        let id = self.id(key)?;
        Some(self.scheduler.arena.get(id).effective_transform())
    }

    /// The laid-out box in absolute window coordinates, projected through the live
    /// transform chain but without coverage padding.
    ///
    /// Tight on purpose: this is what geometry handles read from the published snapshot,
    /// so the two agree. Paint coverage (AA pad, stroke, shadow) stays in the damage and
    /// hit paths, which call `compute_visual_bounds` directly.
    pub fn absolute_rect(&self, key: impl Into<Key>) -> Option<ResolvedRect> {
        let id = self.id(key)?;
        Some(self.scheduler.arena.node_absolute_layout_rect(id))
    }

    /// Resolves a window point to the topmost keyed view under it.
    ///
    /// Deliberately the same code path the router uses for real pointer events, so this and an
    /// actual click cannot disagree.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Key> {
        crate::input::hit::top_key(&self.scheduler.arena, self.scheduler.root, Point::new(x, y))
    }

    /// Runs a closure over the tree, for reads no key-based query covers.
    pub fn with_arena<R>(&self, visit: impl FnOnce(&TreeArena, NodeId) -> R) -> R {
        visit(&self.scheduler.arena, self.scheduler.root)
    }
}
