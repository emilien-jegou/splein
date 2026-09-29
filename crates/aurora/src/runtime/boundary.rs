// Single responsibility: Layout boundary resolution and nested boundary deduplication.

use crate::tree::{DirtyFlags, NodeId, TreeArena};
use smallvec::SmallVec;

pub type BoundaryVec = SmallVec<[NodeId; 16]>;

/// Collects distinct containing layout boundaries for all dirty nodes.
pub fn collect_layout_boundaries(
    arena: &TreeArena,
    root: NodeId,
    dirty_nodes: &[NodeId],
) -> BoundaryVec {
    let mut boundaries: BoundaryVec = dirty_nodes
        .iter()
        .filter(|&&id| {
            if !arena.is_valid(id) {
                return false;
            }
            let d = arena.get(id).state.dirty;
            d.contains(DirtyFlags::LAYOUT) || d.contains(DirtyFlags::MEASURE)
        })
        .map(|&id| {
            let is_dim = arena.get(id).state.bindings.has_dimension_bindings();
            let origin = if is_dim {
                arena.parent(id).unwrap_or(id)
            } else {
                id
            };
            find_layout_boundary(arena, root, origin)
        })
        .collect();

    boundaries.sort_unstable_by_key(|n| n.index);
    boundaries.dedup();
    boundaries
}

/// Ascends ancestors to find the nearest independent 2D layout boundary.
pub fn find_layout_boundary(arena: &TreeArena, root: NodeId, mut current: NodeId) -> NodeId {
    while current != root {
        if arena.get(current).is_layout_boundary() {
            return current;
        }
        match arena.parent(current) {
            Some(parent) => current = parent,
            None => break,
        }
    }
    root
}

/// Prunes inner boundaries when an ancestor boundary is already dirty.
pub fn prune_nested_boundaries(arena: &TreeArena, boundaries: &mut BoundaryVec) {
    if boundaries.len() <= 1 {
        return;
    }
    let snapshot: BoundaryVec = boundaries.clone();
    boundaries.retain(|candidate| {
        let mut curr = *candidate;
        while let Some(parent) = arena.parent(curr) {
            if snapshot.contains(&parent) {
                return false;
            }
            curr = parent;
        }
        true
    });
}
