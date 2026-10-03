// Single responsibility: Coordination and execution of the layout pipeline passes.

use smallvec::SmallVec;

use crate::foundation::{Constraints, Direction, Distribution, Point, ResolvedRect, Size};
use crate::layout::allocate_absolute::resolve_absolute_child;
use crate::layout::allocate_cross::resolve_child_cross;
use crate::layout::allocate_main::{allocate_main_axis, MainAxisItems};
use crate::layout::measure::{
    compute_child_desired, compute_intrinsic, compute_intrinsic_with_constraints,
    fit_text_run_to_width,
};
use crate::layout::min_size::compute_min_size;
use crate::layout::report::LayoutResult;
use crate::text::TextContext;
use crate::tree::{DirtyFlags, NodeId, TreeArena};

pub fn layout_node(arena: &mut TreeArena, node_id: NodeId, constraints: Constraints) {
    let text_ctx = TextContext::new();
    let mut result = LayoutResult::default();
    layout_node_with_text(
        arena,
        node_id,
        constraints,
        &text_ctx,
        &mut result,
        Point::ZERO,
    );
}

#[inline(always)]
pub fn layout_node_with_text(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    result: &mut LayoutResult,
    parent_abs: Point,
) {
    let dirty = arena.get(node_id).state.dirty;
    // A dirty descendant must still be visited: the cache only describes this node's own size,
    // and boundary resolution can land here even when this node was never marked dirty.
    let needs_full_pass = dirty.contains(DirtyFlags::LAYOUT)
        || dirty.contains(DirtyFlags::MEASURE)
        || dirty.contains(DirtyFlags::SUBTREE_DIRTY);

    if !needs_full_pass {
        if let Some(cached) = arena.get(node_id).state.cache.get_layout(&constraints) {
            let prev = arena.get(node_id).resolved_rect;
            let final_rect = ResolvedRect::new(prev.x, prev.y, cached.width, cached.height);
            arena.get_mut(node_id).resolved_rect = final_rect;
            if arena.parent(node_id).is_some() {
                let abs_rect = ResolvedRect::new(
                    parent_abs.x + prev.x,
                    parent_abs.y + prev.y,
                    cached.width,
                    cached.height,
                );
                result.record_cached(abs_rect);
            }
            return;
        }
    }

    perform_node_layout(arena, node_id, constraints, text_ctx, result, parent_abs);
}

// Uninstrumented hot inner function (runs 1000+ times per frame)
fn perform_node_layout(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    result: &mut LayoutResult,
    parent_abs: Point,
) {
    let (w, h) = resolve_node_bounds(arena, node_id, constraints, text_ctx);
    // A run must not paint wider than the inline size its parent actually assigned to it.
    fit_text_run_to_width(arena, node_id, w, text_ctx);

    let children = arena.children(node_id);
    let mut normal: SmallVec<[NodeId; 8]> = SmallVec::new();
    let mut absolute: SmallVec<[NodeId; 2]> = SmallVec::new();
    let mut collapsed: SmallVec<[NodeId; 2]> = SmallVec::new();

    for &child in children {
        if arena.get(child).style.is_absolute {
            absolute.push(child);
        } else if arena.get(child).state.presence.occupies_space() {
            normal.push(child);
        } else {
            collapsed.push(child);
        }
    }

    let (dir, gap, align, dist) = {
        let l = arena.get(node_id).style.layout;
        (l.direction, l.gap, l.alignment, l.distribution)
    };

    let avail_main = match dir {
        Direction::Horizontal => w,
        Direction::Vertical => h,
    };

    let node_abs = Point::new(
        parent_abs.x + arena.get(node_id).resolved_rect.x,
        parent_abs.y + arena.get(node_id).resolved_rect.y,
    );

    // Per-child resolved cross content box, index-aligned with `normal`. Both axes are
    // resolved before main allocation so wrapping width is final when text is measured.
    let mut cross_content: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());
    let mut cross_offset: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());
    let mut desired_main: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());
    let mut is_fills: SmallVec<[bool; 8]> = SmallVec::with_capacity(normal.len());
    let mut shrinks: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());
    let mut margins: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());
    let mut min_mains: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());

    match dir {
        // Width (cross) is resolved first: it governs text wrapping and therefore height.
        Direction::Vertical => {
            for &child in &normal {
                let (m, width_intent) = {
                    let n = arena.get(child);
                    (n.style.margin, n.style.width)
                };
                let intrinsic_cross = if width_intent.is_fit() {
                    compute_intrinsic_with_constraints(
                        arena,
                        child,
                        Constraints::UNCONSTRAINED,
                        text_ctx,
                    )
                    .width
                } else {
                    0.0
                };
                let (content, offset) =
                    resolve_child_cross(align, w, width_intent, intrinsic_cross, m.left, m.right);
                cross_content.push(content);
                cross_offset.push(offset);

                // Height is measured against the final width, never the parent width.
                let d = compute_child_desired(
                    arena,
                    child,
                    Constraints::tight_width(content),
                    Some(w),
                    Some(h),
                    text_ctx,
                );
                let block_floor = compute_min_size(arena, child, Direction::Vertical, text_ctx)
                    + arena.get(child).style.margin.vertical();
                let n = arena.get(child);
                desired_main.push(d.height + n.style.margin.vertical());
                is_fills.push(n.style.height.is_fill());
                shrinks.push(n.style.shrink);
                margins.push(n.style.margin.vertical());
                min_mains.push(block_floor);
            }
        }
        // Width (main) is allocated first; height (cross) wraps at the assigned width.
        Direction::Horizontal => {
            for &child in &normal {
                let d = compute_child_desired(
                    arena,
                    child,
                    Constraints::loose(w, h),
                    Some(w),
                    Some(h),
                    text_ctx,
                );
                let inline_floor = compute_min_size(arena, child, Direction::Horizontal, text_ctx)
                    + arena.get(child).style.margin.horizontal();
                let n = arena.get(child);
                desired_main.push(d.width + n.style.margin.horizontal());
                is_fills.push(n.style.width.is_fill());
                shrinks.push(n.style.shrink);
                margins.push(n.style.margin.horizontal());
                min_mains.push(inline_floor);
                cross_content.push(0.0);
                cross_offset.push(0.0);
            }
        }
    }

    let plan = allocate_main_axis(
        avail_main,
        gap,
        dist,
        &MainAxisItems {
            desired: &desired_main,
            is_fill: &is_fills,
            shrink: &shrinks,
            margins: &margins,
            min_sizes: &min_mains,
        },
    );

    if dir == Direction::Horizontal {
        for (i, &child) in normal.iter().enumerate() {
            let assigned_w = (plan.sizes[i] - margins[i]).max(0.0);
            let (m, height_intent) = {
                let n = arena.get(child);
                (n.style.margin, n.style.height)
            };
            let intrinsic_cross = if height_intent.is_fit() {
                compute_child_desired(
                    arena,
                    child,
                    Constraints::tight_width(assigned_w),
                    None,
                    None,
                    text_ctx,
                )
                .height
            } else {
                0.0
            };
            let (content, offset) =
                resolve_child_cross(align, h, height_intent, intrinsic_cross, m.top, m.bottom);
            cross_content[i] = content;
            cross_offset[i] = offset;
        }
    }

    let total_used = if !plan.sizes.is_empty() {
        plan.sizes.iter().sum::<f32>() + (plan.sizes.len() - 1) as f32 * plan.gap_px
    } else {
        0.0
    };

    let mut cursor = match dist {
        Distribution::Center if avail_main.is_finite() => {
            ((avail_main - total_used) * 0.5).max(0.0)
        }
        Distribution::End if avail_main.is_finite() => (avail_main - total_used).max(0.0),
        _ => 0.0,
    };

    for (i, &child) in normal.iter().enumerate() {
        let main_alloc = plan.sizes[i];
        let main_m_start = {
            let n = arena.get(child);
            match dir {
                Direction::Horizontal => n.style.margin.left,
                Direction::Vertical => n.style.margin.top,
            }
        };
        let content_cross = cross_content[i].max(0.0);
        let offset_cross = cross_offset[i];
        let main_len = (main_alloc - margins[i]).max(0.0);

        let rect = match dir {
            Direction::Horizontal => {
                ResolvedRect::new(cursor + main_m_start, offset_cross, main_len, content_cross)
            }
            Direction::Vertical => {
                ResolvedRect::new(offset_cross, cursor + main_m_start, content_cross, main_len)
            }
        };

        cursor += main_alloc + plan.gap_px;
        arena.get_mut(child).resolved_rect = rect;
        layout_node_with_text(
            arena,
            child,
            Constraints::tight(rect.width, rect.height),
            text_ctx,
            result,
            node_abs,
        );
    }

    // Collapsed nodes hold a zero-sized slot at the end of the flow so they can grow back in place.
    for &child in &collapsed {
        let rect = match dir {
            Direction::Vertical => ResolvedRect::new(0.0, cursor, 0.0, 0.0),
            Direction::Horizontal => ResolvedRect::new(cursor, 0.0, 0.0, 0.0),
        };
        arena.get_mut(child).resolved_rect = rect;
        layout_node_with_text(
            arena,
            child,
            Constraints::tight(0.0, 0.0),
            text_ctx,
            result,
            node_abs,
        );
    }

    for &child in &absolute {
        let d = compute_child_desired(
            arena,
            child,
            Constraints::loose(w, h),
            Some(w),
            Some(h),
            text_ctx,
        );
        let rect = resolve_absolute_child(arena.get(child), w, h, d.width, d.height);
        arena.get_mut(child).resolved_rect = rect;
        layout_node_with_text(
            arena,
            child,
            Constraints::tight(rect.width, rect.height),
            text_ctx,
            result,
            node_abs,
        );
    }

    let prev = arena.get(node_id).resolved_rect;
    let final_rect = ResolvedRect::new(prev.x, prev.y, w, h);
    let is_boundary = arena.get(node_id).is_layout_boundary();
    let is_root = arena.parent(node_id).is_none();

    let root = arena.get_mut(node_id);
    root.resolved_rect = final_rect;
    root.state.cache.store_layout(constraints, final_rect);
    root.state.dirty.remove(DirtyFlags::LAYOUT);
    root.state.dirty.remove(DirtyFlags::MEASURE);

    let abs_rect = ResolvedRect::new(node_abs.x, node_abs.y, w, h);
    result.record_recomputed(abs_rect);
    if is_boundary && !is_root {
        result.record_boundary(abs_rect);
    }
}

fn resolve_node_bounds(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
) -> (f32, f32) {
    let node = arena.get(node_id);
    let (w_size, h_size) = (node.style.width, node.style.height);

    let (mut w, mut h) = (
        resolve_axis(w_size, constraints.max_width, constraints.is_tight_width()),
        resolve_axis(
            h_size,
            constraints.max_height,
            constraints.is_tight_height(),
        ),
    );

    if w_size.is_fit() || h_size.is_fit() {
        let intrinsic = compute_intrinsic(arena, node_id, constraints, text_ctx);
        if w_size.is_fit() {
            w = intrinsic.width.min(constraints.max_width);
        }
        if h_size.is_fit() {
            h = intrinsic.height.min(constraints.max_height);
        }
    }

    (w.max(0.0), h.max(0.0))
}

/// Resolves one axis: a tight bound is already final, so intent must not be re-applied.
fn resolve_axis(size: Size, bound: f32, is_tight: bool) -> f32 {
    if is_tight {
        return if bound.is_finite() {
            bound.max(0.0)
        } else {
            0.0
        };
    }
    match size {
        Size::Fixed(value) => {
            if value.is_finite() {
                value.max(0.0).min(bound.max(0.0))
            } else {
                bound
            }
        }
        Size::Percent(ratio) => {
            if bound.is_finite() {
                (bound * ratio).max(0.0)
            } else {
                bound
            }
        }
        Size::Fill | Size::Fit => bound,
    }
}
