// Single responsibility: Stacking context tree construction and z-index order partitioning.

use crate::foundation::{Point, ResolvedRect, Transform};
use crate::scene::chunk::InheritedClip;
use crate::tree::{NodeId};
use crate::tree::{TreeArena};
use smallvec::SmallVec;

/// Hierarchical stacking context partitioning nodes by z-index and overlay status.
pub struct StackingContext {
    pub node_id: NodeId,
    pub z_index: i32,
    pub abs_origin: Point,
    /// Ancestor clips to re-assert at the top of this context's chunk.
    pub inherited_clips: crate::scene::chunk::InheritedClips,
    pub negative_z: Vec<StackingContext>,
    pub normal_flow: SmallVec<[NodeId; 8]>,
    pub positive_z: Vec<StackingContext>,
}

impl StackingContext {
    #[inline(always)]
    pub fn new(node_id: NodeId, z_index: i32, abs_origin: Point) -> Self {
        Self {
            node_id,
            z_index,
            abs_origin,
            inherited_clips: crate::scene::chunk::InheritedClips::new(),
            negative_z: Vec::new(),
            normal_flow: SmallVec::new(),
            positive_z: Vec::new(),
        }
    }
}

/// The clips a node imposes on its descendants, in absolute screen space.
///
/// Mirrors `enter_node_frame` exactly, including its single-clip rule: while a render box is
/// animating the reveal is square, and the declared radius applies only once it settles. A
/// descendant chunk therefore has to push whatever its parent pushed, or it paints outside a
/// shrinking parent.
fn clips_imposed_by(
    node: &crate::tree::LayoutNode,
    abs_origin: Point,
) -> crate::scene::chunk::InheritedClips {
    let mut clips = crate::scene::chunk::InheritedClips::new();
    if let Some(radius) = crate::pipeline::compile::emit::node_clip_radius(node) {
        let painted = node.resolved_rect;
        clips.push(InheritedClip {
            rect: ResolvedRect::new(abs_origin.x, abs_origin.y, painted.width, painted.height),
            radius,
        });
    }
    clips
}

/// Recursively builds the stacking context tree and sorts negative/positive contexts.
pub fn build_stacking_tree(
    arena: &TreeArena,
    parent_id: NodeId,
    current_ctx: &mut StackingContext,
    parent_tx: Transform,
    inherited: &crate::scene::chunk::InheritedClips,
    count: &mut usize,
    context_bounds: &mut Vec<ResolvedRect>,
) {
    // This node's own clip applies to every descendant chunk spawned below it.
    let mut child_inherited = inherited.clone();
    {
        let node = arena.get(parent_id);
        let abs_origin = parent_tx.transform_point(Point::ZERO);
        child_inherited.extend(clips_imposed_by(node, abs_origin));
    }

    for &child_id in arena.children(parent_id) {
        let child = arena.get(child_id);
        let painted = child.resolved_rect;
        let child_tx = parent_tx.multiply(&Transform::from_translation(painted.x, painted.y));

        if child.creates_stacking_context() {
            *count += 1;
            // The chunk origin is the child's rendered box origin, before its own transform.
            let child_origin = child_tx.transform_point(Point::ZERO);
            let abs_rect = ResolvedRect::new(
                child_origin.x,
                child_origin.y,
                painted.width,
                painted.height,
            );
            context_bounds.push(abs_rect);

            let effective_z = child.paint_layer().z();
            let mut child_ctx = StackingContext::new(child_id, effective_z, child_origin);
            child_ctx.inherited_clips = child_inherited.clone();
            build_stacking_tree(
                arena,
                child_id,
                &mut child_ctx,
                child_tx.multiply(&child.effective_transform()),
                &child_inherited,
                count,
                context_bounds,
            );

            if child.paint_layer() == crate::tree::node::PaintLayer::Below(effective_z) {
                current_ctx.negative_z.push(child_ctx);
            } else {
                current_ctx.positive_z.push(child_ctx);
            }
        } else {
            current_ctx.normal_flow.push(child_id);
            build_stacking_tree(
                arena,
                child_id,
                current_ctx,
                child_tx,
                &child_inherited,
                count,
                context_bounds,
            );
        }
    }

    // Stable: equal z-index resolves by tree order (later sibling paints above),
    // and only a stable sort preserves the insertion order the walk produced.
    current_ctx
        .negative_z
        .sort_by_key(|ctx| ctx.z_index);
    current_ctx
        .positive_z
        .sort_by_key(|ctx| ctx.z_index);
}
