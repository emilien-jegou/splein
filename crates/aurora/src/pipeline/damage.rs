// Single responsibility: Pure geometric damage analysis comparing visual bounds against history.

use crate::foundation::{DamageRegion, Point, ResolvedRect, Transform};
use crate::tree::{DirtyFlags, NodeId, NodeKind, TreeArena};

/// Damage calculation plan containing aggregated damaged regions and removed node bounds.
#[derive(Clone, Debug, Default)]
pub struct DamagePlan {
    /// Minimal non-overlapping damage rectangles.
    pub region: DamageRegion,
    /// Screen-space bounds of nodes unmounted this frame.
    pub removed_rects: Vec<ResolvedRect>,
}

impl DamagePlan {
    /// Computes damage regions, skipping un-mutated subtrees in O(M) when layout is clean.
    #[tracing::instrument(skip_all)]
    pub fn compute(
        arena: &mut TreeArena,
        root: NodeId,
        removed_bounds: Vec<ResolvedRect>,
        dirty_nodes: &[NodeId],
        laid_out: bool,
    ) -> Self {
        let mut plan = Self {
            region: DamageRegion::new(),
            removed_rects: removed_bounds,
        };

        for removed in &plan.removed_rects {
            plan.region.push(*removed);
        }

        // Fast Path 1: Idle frame (no nodes dirtied, no layout, no unmounted rects)
        if !laid_out && dirty_nodes.is_empty() && plan.removed_rects.is_empty() {
            return plan;
        }

        // Fast Path 2: Paint-only mutations walk only the dirty subtrees (O(M*K)), pushing
        // each node's previous and current bounds so a moved node damages both positions.
        if !laid_out {
            for &id in dirty_nodes {
                if !arena.is_valid(id) {
                    continue;
                }
                let parent_tx = arena
                    .parent(id)
                    .map_or(Transform::IDENTITY, |p| arena.node_absolute_transform(p));
                resolve_spatial_and_damage(arena, id, parent_tx, None, &mut plan.region);
            }
            return plan;
        }

        // Layout path: Full / boundary tree resolution with hierarchical pruning
        let root_clip = if arena.get(root).style.clip {
            Some(ResolvedRect::new(
                0.0,
                0.0,
                arena.get(root).resolved_rect.width,
                arena.get(root).resolved_rect.height,
            ))
        } else {
            None
        };

        resolve_spatial_and_damage(
            arena,
            root,
            Transform::IDENTITY,
            root_clip,
            &mut plan.region,
        );
        plan
    }
}

fn resolve_spatial_and_damage(
    arena: &mut TreeArena,
    id: NodeId,
    parent_tx: Transform,
    active_clip: Option<ResolvedRect>,
    damage: &mut DamageRegion,
) -> ResolvedRect {
    let (node_tx, is_clip, local_rect) = {
        let node = arena.get(id);
        let mut tx = parent_tx.multiply(&Transform::from_translation(
            node.resolved_rect.x,
            node.resolved_rect.y,
        ));
        let local_tx = node.effective_transform();
        if local_tx != Transform::IDENTITY {
            tx = tx.multiply(&local_tx);
        }
        (tx, node.style.clip, node.resolved_rect)
    };

    let node_abs = node_tx.transform_point(Point::new(0.0, 0.0));
    // Unclipped occupancy drives damage: a node leaving its clip must still clear its old pixels.
    let full_bounds = arena.get(id).compute_visual_bounds(&node_tx, None);
    // Visible extent under the active clip: bounds the subtree work a chunk has to rasterize.
    let visual_bounds = arena.get(id).compute_visual_bounds(&node_tx, active_clip);
    let mut subtree_bounds = visual_bounds;

    let next_clip = if is_clip {
        let box_rect =
            ResolvedRect::new(node_abs.x, node_abs.y, local_rect.width, local_rect.height);
        Some(active_clip.map_or(box_rect, |c| c.intersect(&box_rect)))
    } else {
        active_clip
    };

    let child_count = arena.children(id).len();
    for i in 0..child_count {
        let child = arena.children(id)[i];
        let child_subtree = resolve_spatial_and_damage(arena, child, node_tx, next_clip, damage);
        subtree_bounds = subtree_bounds.union(&child_subtree);
    }

    let node = arena.get_mut(id);
    let old_bounds = node.state.last_painted_bounds;
    let is_text_mutation = matches!(node.kind, NodeKind::Text(_))
        && (node.state.dirty.contains(DirtyFlags::MEASURE)
            || node.state.dirty.contains(DirtyFlags::LAYOUT));

    if let Some(old) = old_bounds {
        if old != full_bounds {
            damage.push(old);
            damage.push(full_bounds);
        } else if node.state.dirty.contains(DirtyFlags::PAINT) || is_text_mutation {
            let damage_target = if child_count > 0 {
                subtree_bounds
            } else {
                full_bounds
            };
            damage.push(damage_target);
        }
    } else {
        damage.push(full_bounds);
    }

    node.state.subtree_bounds = subtree_bounds;
    subtree_bounds
}
