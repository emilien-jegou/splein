// Single responsibility: Node tree visual bounds evaluation, framing, and command emission.

use crate::foundation::{Point, ResolvedRect, Transform};
use crate::pipeline::compile::emit::{emit_node_content, enter_node_frame, exit_node_frame};
use crate::scene::{LayerId, SceneChunk};
use crate::text::TextContext;
use crate::tree::TreeArena;
use crate::tree::{DirtyFlags, NodeId};

/// Recursively traverses a node hierarchy and emits commands into the current scene chunk.
pub fn compile_node_hierarchy(
    arena: &mut TreeArena,
    id: NodeId,
    chunk: &mut SceneChunk,
    text_ctx: &TextContext,
    viewport: ResolvedRect,
    parent_tx: Transform,
    is_stacking_root: bool,
    caret_visible: bool,
) {
    let node = arena.get(id);
    // A stacking context root offsets its chunk from the inherited origin; flow nodes nest locally.
    // Every node is placed by its render box, so a node whose box is mid-flight lands
    // where it is being drawn rather than where layout eventually wants it.
    let painted = node.resolved_rect;
    let own_tx = if is_stacking_root {
        parent_tx.multiply(&node.effective_transform())
    } else {
        parent_tx.multiply(&Transform::from_translation(painted.x, painted.y))
    };
    let visual_screen = node.compute_visual_bounds(&own_tx, None);
    // A culled node keeps its flags (the early return skips the clear at the
    // end of this function), so culling a dirty node lets its parent replay
    // content that never included it — and nothing re-emits it on reveal.
    // Only settled subtrees cull: a dirty offscreen node encodes (once,
    // cheap) and clears together with its parent, so reveal always replays
    // complete content.
    let dirty = node.state.core.dirty;
    let subtree_settled = !dirty.contains(DirtyFlags::SUBTREE_DIRTY)
        && !dirty.contains(
            DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT | DirtyFlags::SCROLL,
        );
    if !visual_screen.overlaps(&viewport)
        && !node.style.has_layer
        && !is_stacking_root
        && subtree_settled
    {
        return;
    }

    let (off_x, off_y) = if is_stacking_root {
        let origin = parent_tx.transform_point(Point::ZERO);
        (origin.x, origin.y)
    } else {
        (painted.x, painted.y)
    };
    enter_node_frame(
        node,
        chunk,
        off_x,
        off_y,
        LayerId::from_packed(id.packed()),
        is_stacking_root,
    );
    // Absolute layout-space Y of the node's content origin: accumulated
    // offsets through the ancestor chain, so a transition moving an
    // ancestor reads as motion here. (Pure canvas-scroll shifts applied by
    // engines below compile are invisible to this — accepted: scroll keeps
    // subpixel placement.)
    let abs_origin = own_tx.transform_point(Point::ZERO);
    emit_node_content(
        node,
        chunk,
        text_ctx,
        caret_visible,
        abs_origin.y,
        id.packed(),
    );

    // Hoisted single copy (see bounds.rs): per-index `children()` calls
    // rebuild the whole child list N+1 times per node per frame.
    let kids = arena.children(id);
    let child_count = kids.len();
    // Children that live in their own chunks — real stacking contexts, or
    // isolated children of an `isolate_children` parent — are compiled by the
    // context loops, never inline here. Emitting both would double-paint.
    let isolates = arena.get(id).style.isolate_children;
    for i in 0..child_count {
        let child_id = kids[i];
        let child = arena.get(child_id);
        let creates_context = child.creates_stacking_context();
        if !creates_context && !isolates {
            compile_node_hierarchy(
                arena,
                child_id,
                chunk,
                text_ctx,
                viewport,
                own_tx,
                false,
                caret_visible,
            );
        }
    }

    let node_mut = arena.get_mut(id);
    exit_node_frame(
        node_mut,
        chunk,
        LayerId::from_packed(id.packed()),
        is_stacking_root,
    );
    node_mut
        .state
        .core
        .dirty
        .remove(DirtyFlags::PAINT | DirtyFlags::SCROLL | DirtyFlags::SUBTREE_DIRTY);
}
