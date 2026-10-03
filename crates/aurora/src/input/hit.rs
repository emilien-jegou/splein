// Single responsibility: Painter-ordered pointer hit resolution producing a bubbling hit chain.

use crate::foundation::{Point, ResolvedRect, Transform};
use crate::tree::node::PaintLayer;
use crate::tree::{NodeId, TreeArena};

/// One entry in a hit chain: a node under the pointer, plus why it matched.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HitNode {
    pub id: NodeId,
    /// The node's box in absolute window coordinates, which is what a gesture offset from.
    pub bounds: ResolvedRect,
}

/// Nodes under a point, ordered topmost first, ending at the root.
///
/// Ordering mirrors `compile_stacking_context` exactly, in reverse: the topmost painted
/// stacking context is tested first, then the node's own children in reverse document order,
/// then the negative-z contexts. A chain that disagreed with paint order would deliver clicks
/// to whatever is drawn last rather than what is drawn on top.
pub fn hit_chain(arena: &TreeArena, root: NodeId, point: Point) -> Vec<HitNode> {
    let mut out = Vec::new();
    if arena.is_valid(root) {
        collect(arena, root, Transform::IDENTITY, None, point, &mut out);
    }
    out
}

/// Whether a point falls inside a node once its clip is applied.
///
/// `clip` is the active ancestor clip in absolute coordinates. Honouring it is what keeps a
/// view that has been clipped out of its parent from swallowing clicks meant for whatever is
/// actually visible beneath it.
#[inline]
fn contains_clipped(bounds: &ResolvedRect, clip: Option<ResolvedRect>, point: Point) -> bool {
    if !bounds.contains(point) {
        return false;
    }
    clip.map_or(true, |c| c.contains(point))
}

/// Appends `id`'s subtree to `out` in reverse painter order, stopping at the first hit.
///
/// Returns whether anything under this subtree was hit. An ancestor still joins the chain
/// when one of its descendants was hit, because the point has to lie inside the ancestor for
/// the descendant to be visible at all; that is what makes the result a chain rather than a
/// single node, and it is what lets a press bubble.
fn collect(
    arena: &TreeArena,
    id: NodeId,
    parent_tx: Transform,
    active_clip: Option<ResolvedRect>,
    point: Point,
    out: &mut Vec<HitNode>,
) -> bool {
    let (child_tx, bounds, next_clip, layers) = {
        let node = arena.get(id);
        let painted = node.resolved_rect;
        let child_tx = parent_tx
            .multiply(&Transform::from_translation(painted.x, painted.y))
            .multiply(&node.effective_transform());
        let bounds = node.compute_visual_bounds(&child_tx, active_clip);
        let next_clip = if node.style.clip {
            let box_rect =
                ResolvedRect::new(child_tx.tx, child_tx.ty, painted.width, painted.height);
            Some(active_clip.map_or(box_rect, |c| c.intersect(&box_rect)))
        } else {
            active_clip
        };
        // Only stacking-context children are partitioned by z; the rest paint in document
        // order, so they stay in one list and are simply walked backwards.
        let children = arena.children(id);
        let mut above = Vec::new();
        let mut below = Vec::new();
        let mut flow = Vec::new();
        for &child_id in children {
            if !arena.get(child_id).creates_stacking_context() {
                flow.push(child_id);
                continue;
            }
            match arena.get(child_id).paint_layer() {
                PaintLayer::Above(z) => above.push((z, child_id)),
                PaintLayer::Below(z) => below.push((z, child_id)),
            }
        }
        // Reverse first, then stable-sort by descending z. That yields exactly the reverse
        // of the ascending-z paint order: the topmost layer first, and among equal z the
        // later sibling first, because that is the one painted last. Sorting first and
        // reversing afterwards would keep equal-z ties in the wrong order.
        above.reverse();
        above.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        below.reverse();
        below.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        (child_tx, bounds, next_clip, (above, below, flow))
    };

    let (above, below, flow) = layers;
    // The deepest hit found so far, and whether there was one at all. Only the first
    // sibling subtree hit counts: it is the topmost, so anything after it is beneath.
    let mut deepest: Option<(NodeId, ResolvedRect)> = None;

    for (_, child_id) in above {
        if collect(arena, child_id, child_tx, next_clip, point, out) {
            deepest = out.first().map(|h| (h.id, h.bounds));
            break;
        }
    }
    if deepest.is_none() {
        for child_id in flow.into_iter().rev() {
            if collect(arena, child_id, child_tx, next_clip, point, out) {
                deepest = out.first().map(|h| (h.id, h.bounds));
                break;
            }
        }
    }
    if deepest.is_none() {
        for (_, child_id) in below {
            if collect(arena, child_id, child_tx, next_clip, point, out) {
                deepest = out.first().map(|h| (h.id, h.bounds));
                break;
            }
        }
    }

    if let Some(_) = deepest {
        // A descendant was hit. Join the chain behind it if this node is under the point too,
        // which is what lets the event reach ancestors.
        if contains_clipped(&bounds, active_clip, point) {
            out.push(HitNode { id, bounds });
        }
        return true;
    }

    if contains_clipped(&bounds, active_clip, point) {
        out.push(HitNode { id, bounds });
        return true;
    }
    false
}

/// The keyed node on top at a point, for callers that only need the target.
pub fn top_key(arena: &TreeArena, root: NodeId, point: Point) -> Option<crate::foundation::Key> {
    hit_chain(arena, root, point)
        .into_iter()
        .find_map(|hit| arena.get(hit.id).key.clone())
}
