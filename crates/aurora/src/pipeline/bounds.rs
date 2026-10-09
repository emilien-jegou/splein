// Single responsibility: Commits each node's compiled screen-space visual bounds back into the arena.

use crate::foundation::{Point, ResolvedRect, Transform};
use crate::tree::{NodeId};
use crate::tree::{TreeArena};

/// Commits the visual screen-space bounds of every node to the arena.
pub fn commit_painted_bounds(arena: &mut TreeArena, root: NodeId) {
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

    commit_node_painted_bounds(arena, root, Transform::IDENTITY, root_clip);
}

fn commit_node_painted_bounds(
    arena: &mut TreeArena,
    id: NodeId,
    parent_tx: Transform,
    active_clip: Option<ResolvedRect>,
) {
    if !arena.is_valid(id) {
        return;
    }

    let node = arena.get(id);
    let painted = node.resolved_rect;
    let mut tx = parent_tx.multiply(&Transform::from_translation(painted.x, painted.y));
    let local_tx = node.effective_transform();
    if local_tx != Transform::IDENTITY {
        tx = tx.multiply(&local_tx);
    }

    let node_abs = tx.transform_point(Point::new(0.0, 0.0));
    // Stored unclipped: a node that leaves its clip must still remember where it last
    // painted. Same union the damage pass used, so the two compare symmetrically.
    let visual_bounds = node.compute_visual_bounds(&tx, None);

    let next_clip = if node.style.clip {
        let box_rect = ResolvedRect::new(node_abs.x, node_abs.y, painted.width, painted.height);
        Some(active_clip.map_or(box_rect, |c| c.intersect(&box_rect)))
    } else {
        active_clip
    };

    // Both records are needed: the union drives vacated-pixel damage, and the flow-only
    // record detects an interior re-flow that happens without the box moving.
    let flow_bounds = node.compute_flow_bounds(&tx, active_clip);
    let node = arena.get_mut(id);
    node.state.paint.last_painted_bounds = Some(visual_bounds);
    node.state.paint.last_painted_flow = Some(flow_bounds);

    let child_count = arena.children(id).len();
    for i in 0..child_count {
        let child = arena.children(id)[i];
        commit_node_painted_bounds(arena, child, tx, next_clip);
    }
}
