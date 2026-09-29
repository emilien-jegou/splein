// Single responsibility: Node tree visual bounds evaluation, framing, and command emission.

use crate::foundation::{Point, ResolvedRect, Transform};
use crate::pipeline::stage5_compile::emit::{emit_node_content, enter_node_frame, exit_node_frame};
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
    parent_abs: Point,
    is_stacking_root: bool,
) {
    let node = arena.get(id);
    let visual_screen = node.compute_visual_bounds(parent_abs, None);
    if !visual_screen.intersects(&viewport) && !node.style.has_layer && !is_stacking_root {
        return;
    }

    let (off_x, off_y) = if is_stacking_root { (parent_abs.x, parent_abs.y) } else { (node.resolved_rect.x, node.resolved_rect.y) };
    enter_node_frame(node, chunk, off_x, off_y, LayerId::from(id));
    emit_node_content(node, chunk, text_ctx);

    let my_abs = Point::new(parent_abs.x + node.resolved_rect.x, parent_abs.y + node.resolved_rect.y);
    let child_count = arena.children(id).len();
    for i in 0..child_count {
        let child_id = arena.children(id)[i];
        let child = arena.get(child_id);
        let creates_context = child.style.z_index != 0 || child.style.is_overlay || child.style.appearance.opacity < 1.0 || child.transform != Transform::IDENTITY;
        if !creates_context {
            compile_node_hierarchy(arena, child_id, chunk, text_ctx, viewport, my_abs, false);
        }
    }

    let node_mut = arena.get_mut(id);
    exit_node_frame(node_mut, chunk, LayerId::from(id));
    node_mut.state.dirty.remove(DirtyFlags::PAINT | DirtyFlags::SUBTREE_DIRTY);
}
