// Single responsibility: Pure geometric damage analysis comparing visual bounds against history.

use crate::foundation::{DamageRegion, LightEnvironment, Point, ResolvedRect, Transform};
use crate::tree::TreeArena;
use crate::tree::{DirtyFlags, NodeId, NodeKind};

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
                resolve_spatial_and_damage(
                    arena,
                    id,
                    parent_tx,
                    None,
                    LightEnvironment::DEFAULT,
                    &mut plan.region,
                );
            }
            return plan;
        }

        tracing::debug!(
            "aurora: damage plan laid_out={} dirty_nodes={} removed={}",
            laid_out,
            dirty_nodes.len(),
            plan.removed_rects.len(),
        );

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
            LightEnvironment::DEFAULT,
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
    inherited_light: LightEnvironment,
    damage: &mut DamageRegion,
) -> ResolvedRect {
    let (node_tx, is_clip, painted) = {
        let node = arena.get(id);
        // Origin comes from the render box so damage follows what is on screen.
        let painted = node.resolved_rect;
        let mut tx = parent_tx.multiply(&Transform::from_translation(painted.x, painted.y));
        let local_tx = node.effective_transform();
        if local_tx != Transform::IDENTITY {
            tx = tx.multiply(&local_tx);
        }
        (tx, node.style.clip, painted)
    };

    // Ambient light resolves top-down: the nearest override wins, and the
    // resolved value is stashed for paint/bounds/hit testing to read. The
    // stash always precedes its consumers within a frame (damage runs before
    // compile), so it can never feed a stale light to fresh paint.
    let ambient = arena
        .get(id)
        .style
        .appearance
        .light_override
        .unwrap_or(inherited_light);
    arena.get_mut(id).state.core.resolved_light = ambient;

    let node_abs = node_tx.transform_point(Point::new(0.0, 0.0));
    // Unclipped occupancy drives damage: a node leaving its clip must still clear its old pixels.
    let full_bounds = arena.get(id).compute_visual_bounds(&node_tx, None);
    // Visible extent under the active clip: bounds the subtree work a chunk has to rasterize.
    let visual_bounds = arena.get(id).compute_visual_bounds(&node_tx, active_clip);
    let mut subtree_bounds = visual_bounds;

    let next_clip = if is_clip {
        let box_rect = ResolvedRect::new(node_abs.x, node_abs.y, painted.width, painted.height);
        Some(active_clip.map_or(box_rect, |c| c.intersect(&box_rect)))
    } else {
        active_clip
    };

    let child_count = arena.children(id).len();
    for i in 0..child_count {
        let child = arena.children(id)[i];
        let child_subtree =
            resolve_spatial_and_damage(arena, child, node_tx, next_clip, ambient, damage);
        subtree_bounds = subtree_bounds.union(&child_subtree);
    }

    let flow_bounds = arena.get(id).compute_flow_bounds(&node_tx, active_clip);

    let node = arena.get_mut(id);
    let old_bounds = node.state.paint.last_painted_bounds;
    let old_flow = node.state.paint.last_painted_flow;
    let is_text_mutation = matches!(node.kind, NodeKind::Text(_))
        && (node.state.core.dirty.contains(DirtyFlags::MEASURE)
            || node.state.core.dirty.contains(DirtyFlags::LAYOUT));

    if let Some(old) = old_bounds {
        // Either the box moved or the interior re-flowed: both vacate pixels.
        //
        // Clipped to the active clip: paint outside it never happened (compile culls
        // there), so there is nothing to clear beyond it — including for a node that
        // moved, whose old painted pixels were inside the clip it was painted under.
        if old != full_bounds || old_flow != Some(flow_bounds) {
            match active_clip {
                Some(clip) => {
                    damage.push(old.intersect(&clip));
                    damage.push(full_bounds.intersect(&clip));
                }
                None => {
                    damage.push(old);
                    damage.push(full_bounds);
                }
            }
        } else if node.state.core.dirty.contains(DirtyFlags::PAINT) || is_text_mutation {
            let damage_target = if child_count > 0 {
                subtree_bounds
            } else {
                full_bounds
            };
            // New paint never escapes the active clip (compile culls it there), so
            // clip it: a text caret blinking at a clipped edge must not damage
            // outside the clip.
            match active_clip {
                Some(clip) => damage.push(damage_target.intersect(&clip)),
                None => damage.push(damage_target),
            }
        }
    } else {
        damage.push(full_bounds);
    }

    node.state.paint.subtree_bounds = subtree_bounds;
    subtree_bounds
}
