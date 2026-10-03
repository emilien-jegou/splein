// Single responsibility: Invalidation propagation algorithms accounting for Stretch dependencies.

use crate::tree::arena::TreeArena;
use crate::tree::flags::DirtyFlags;
use crate::tree::id::NodeId;

/// Marks a node dirty and propagates upward intrinsic measurements to containing layout boundaries.
pub fn mark_node_dirty(arena: &mut TreeArena, node_id: NodeId, flags: DirtyFlags) {
    if !arena.is_valid(node_id) {
        return;
    }
    arena
        .get_mut(node_id)
        .state
        .dirty
        .insert(flags | DirtyFlags::SUBTREE_DIRTY);

    let mut curr = node_id;
    while let Some(parent) = arena.parent(curr) {
        let p = arena.get_mut(parent);
        if p.state.dirty.contains(DirtyFlags::SUBTREE_DIRTY) {
            break;
        }
        p.state.dirty.insert(DirtyFlags::SUBTREE_DIRTY);
        curr = parent;
    }

    if flags.contains(DirtyFlags::MEASURE) {
        arena.get_mut(node_id).state.cache.clear();
        propagate_upward(arena, node_id);
    } else if flags.contains(DirtyFlags::LAYOUT) {
        arena.get_mut(node_id).state.cache.clear_layout();
    }
}

/// Traverses upward from a mutated node, invalidating ancestor flex layouts.
fn propagate_upward(arena: &mut TreeArena, node_id: NodeId) {
    let mut current = node_id;
    while let Some(parent) = arena.parent(current) {
        let is_boundary = arena.get(parent).is_layout_boundary();
        let is_fit = {
            let p = arena.get(parent);
            p.style.width.is_fit() || p.style.height.is_fit()
        };

        let p_mut = arena.get_mut(parent);
        p_mut
            .state
            .dirty
            .insert(DirtyFlags::LAYOUT | DirtyFlags::SUBTREE_DIRTY);
        p_mut.state.cache.clear_layout();
        // Min-content floors depend on descendants even when the ancestor has a definite size.
        p_mut.state.cache.clear_min_sizes();

        if is_boundary {
            break;
        }

        if is_fit {
            p_mut.state.dirty.insert(DirtyFlags::MEASURE);
            p_mut.state.cache.clear_intrinsic();
            current = parent;
        } else {
            break;
        }
    }
}
