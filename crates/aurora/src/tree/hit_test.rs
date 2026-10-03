// Single responsibility: Reverse painter-order pointer hit-testing over retained bounds.

use crate::foundation::{Key, Point, Transform};
use crate::tree::{NodeId, TreeArena};

/// Resolves a window point to the topmost keyed view whose bounds contain it.
pub fn hit_test(arena: &TreeArena, root: NodeId, point: Point) -> Option<Key> {
    if !arena.is_valid(root) {
        return None;
    }
    hit(arena, root, Transform::IDENTITY, point)
}

/// Visits children before self in reverse order so the topmost keyed view wins.
fn hit(arena: &TreeArena, id: NodeId, tx: Transform, point: Point) -> Option<Key> {
    let node = arena.get(id);
    let child_tx = tx
        .multiply(&Transform::from_translation(
            node.resolved_rect.x,
            node.resolved_rect.y,
        ))
        .multiply(&node.effective_transform());

    let children = arena.children(id);
    for index in (0..children.len()).rev() {
        if let Some(key) = hit(arena, children[index], child_tx, point) {
            return Some(key);
        }
    }

    // Z-index and overlay ordering are not modelled: later siblings win, as in normal flow.
    if node.compute_visual_bounds(&child_tx, None).contains(point) {
        return node.key.clone();
    }
    None
}
