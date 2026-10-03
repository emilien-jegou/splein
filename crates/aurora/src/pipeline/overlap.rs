// Single responsibility: Queries intersecting elements touching damage in painter's order.

use crate::foundation::DamageRegion;
use crate::tree::{NodeId, TreeArena};
use smallvec::SmallVec;

/// Verified painter's order repaint queue intersecting the active damage region.
#[derive(Clone, Debug, Default)]
pub struct OverlapPlan {
    /// Ordered sequence of node identifiers that touch damage and must be re-rendered.
    pub repaint_queue: SmallVec<[NodeId; 32]>,
}

impl OverlapPlan {
    /// Computes painter's order repaint sequence with BVH subtree culling.
    #[tracing::instrument(skip_all)]
    pub fn compute(arena: &TreeArena, root: NodeId, damage: &DamageRegion) -> Self {
        let mut plan = Self {
            repaint_queue: SmallVec::new(),
        };
        if damage.is_empty() || !arena.is_valid(root) {
            return plan;
        }

        let damage_bbox = damage.bounding_box();
        if !arena
            .get(root)
            .state
            .subtree_bounds
            .intersects(&damage_bbox)
        {
            return plan;
        }

        collect_overlapping_nodes(arena, root, damage, &mut plan.repaint_queue);
        plan
    }

    /// Whether any nodes require repaint.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.repaint_queue.is_empty()
    }
}

fn collect_overlapping_nodes(
    arena: &TreeArena,
    id: NodeId,
    damage: &DamageRegion,
    queue: &mut SmallVec<[NodeId; 32]>,
) {
    if !arena.is_valid(id) {
        return;
    }
    let node = arena.get(id);
    let subtree_bounds = node.state.subtree_bounds;

    let mut subtree_intersects = false;
    for rect in damage.rects() {
        if subtree_bounds.intersects(rect) {
            subtree_intersects = true;
            break;
        }
    }
    if !subtree_intersects {
        return;
    }

    if let Some(painted) = node.state.last_painted_bounds {
        for rect in damage.rects() {
            if painted.intersects(rect) {
                queue.push(id);
                break;
            }
        }
    }

    for &child in arena.children(id) {
        collect_overlapping_nodes(arena, child, damage, queue);
    }
}
