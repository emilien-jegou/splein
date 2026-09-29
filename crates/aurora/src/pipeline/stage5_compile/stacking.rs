// Single responsibility: Stacking context tree construction and z-index order partitioning.

use crate::foundation::{Point, ResolvedRect, Transform};
use crate::tree::{NodeId, TreeArena};
use smallvec::SmallVec;

/// Hierarchical stacking context partitioning nodes by z-index and overlay status.
pub struct StackingContext {
    pub node_id: NodeId,
    pub z_index: i32,
    pub abs_origin: Point,
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
            negative_z: Vec::new(),
            normal_flow: SmallVec::new(),
            positive_z: Vec::new(),
        }
    }
}

/// Recursively builds the stacking context tree and sorts negative/positive contexts.
pub fn build_stacking_tree(
    arena: &TreeArena,
    parent_id: NodeId,
    current_ctx: &mut StackingContext,
    parent_abs: Point,
    count: &mut usize,
    context_bounds: &mut Vec<ResolvedRect>,
) {
    for &child_id in arena.children(parent_id) {
        let child = arena.get(child_id);
        let child_abs = Point::new(
            parent_abs.x + child.resolved_rect.x,
            parent_abs.y + child.resolved_rect.y,
        );

        let creates_context = child.style.z_index != 0
            || child.style.is_overlay
            || child.style.appearance.opacity < 1.0
            || child.transform != Transform::IDENTITY;

        if creates_context {
            *count += 1;
            // O(1) Zero-cost absolute bounds calculation using child_abs
            let abs_rect = ResolvedRect::new(
                child_abs.x,
                child_abs.y,
                child.resolved_rect.width,
                child.resolved_rect.height,
            );
            context_bounds.push(abs_rect);

            let effective_z = if child.style.is_overlay {
                child.style.z_index.max(100)
            } else {
                child.style.z_index
            };
            let mut child_ctx = StackingContext::new(child_id, effective_z, child_abs);
            build_stacking_tree(
                arena,
                child_id,
                &mut child_ctx,
                child_abs,
                count,
                context_bounds,
            );

            if effective_z < 0 {
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
                child_abs,
                count,
                context_bounds,
            );
        }
    }

    current_ctx
        .negative_z
        .sort_unstable_by_key(|ctx| ctx.z_index);
    current_ctx
        .positive_z
        .sort_unstable_by_key(|ctx| ctx.z_index);
}
