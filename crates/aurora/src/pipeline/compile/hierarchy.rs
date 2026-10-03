// Single responsibility: Node tree visual bounds evaluation, framing, and command emission.

use crate::foundation::{Point, ResolvedRect, Transform};
use crate::pipeline::compile::emit::{emit_node_content, enter_node_frame, exit_node_frame};
use crate::scene::{LayerId, SceneChunk};
use crate::text::TextContext;
use crate::tree::{DirtyFlags, NodeId, TreeArena};

/// Recursively traverses a node hierarchy and emits commands into the current scene chunk.
pub fn compile_node_hierarchy(
    arena: &mut TreeArena,
    id: NodeId,
    chunk: &mut SceneChunk,
    text_ctx: &TextContext,
    viewport: ResolvedRect,
    parent_tx: Transform,
    is_stacking_root: bool,
) {
    let node = arena.get(id);
    // A stacking context root offsets its chunk from the inherited origin; flow nodes nest locally.
    let own_tx = if is_stacking_root {
        parent_tx.multiply(&node.effective_transform())
    } else {
        parent_tx.multiply(&Transform::from_translation(
            node.resolved_rect.x,
            node.resolved_rect.y,
        ))
    };
    let visual_screen = node.compute_visual_bounds(&own_tx, None);
    if !visual_screen.overlaps(&viewport) && !node.style.has_layer && !is_stacking_root {
        return;
    }

    let (off_x, off_y) = if is_stacking_root {
        let origin = parent_tx.transform_point(Point::ZERO);
        (origin.x, origin.y)
    } else {
        (node.resolved_rect.x, node.resolved_rect.y)
    };
    enter_node_frame(node, chunk, off_x, off_y, LayerId::from_packed(id.packed()));
    emit_node_content(node, chunk, text_ctx);

    let child_count = arena.children(id).len();
    for i in 0..child_count {
        let child_id = arena.children(id)[i];
        let child = arena.get(child_id);
        let creates_context = child.creates_stacking_context();
        if !creates_context {
            compile_node_hierarchy(arena, child_id, chunk, text_ctx, viewport, own_tx, false);
        }
    }

    let node_mut = arena.get_mut(id);
    exit_node_frame(node_mut, chunk, LayerId::from_packed(id.packed()));
    node_mut
        .state
        .dirty
        .remove(DirtyFlags::PAINT | DirtyFlags::SUBTREE_DIRTY);
}
