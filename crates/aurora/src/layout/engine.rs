// Single responsibility: Coordination and execution of the layout pipeline passes.

use smallvec::SmallVec;

use crate::foundation::{
    Constraints, DesiredSize, Direction, Distribution, Point, ResolvedRect, Size,
};
use crate::layout::allocate_absolute::resolve_absolute_child;
use crate::layout::allocate_cross::resolve_cross_axis;
use crate::layout::allocate_main::allocate_main_axis;
use crate::layout::measure::{compute_child_desired, compute_intrinsic};
use crate::pipeline::stage2_layout::LayoutResult;
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
    let needs_full_pass = dirty.contains(DirtyFlags::LAYOUT) || dirty.contains(DirtyFlags::MEASURE);

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

    let children = arena.children(node_id);
    let mut normal: SmallVec<[NodeId; 8]> = SmallVec::new();
    let mut absolute: SmallVec<[NodeId; 2]> = SmallVec::new();

    for &child in children {
        if arena.get(child).style.is_absolute {
            absolute.push(child);
        } else {
            normal.push(child);
        }
    }

    let (dir, gap, align, dist) = {
        let l = arena.get(node_id).style.layout;
        (l.direction, l.gap, l.alignment, l.distribution)
    };

    let mut desired_sizes: SmallVec<[DesiredSize; 8]> = SmallVec::with_capacity(normal.len());
    let mut desired_main: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());
    let mut is_fills: SmallVec<[bool; 8]> = SmallVec::with_capacity(normal.len());
    let mut shrinks: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());
    let mut margins: SmallVec<[f32; 8]> = SmallVec::with_capacity(normal.len());

    for &child in &normal {
        let d = compute_child_desired(
            arena,
            child,
            Constraints::loose(w, h),
            Some(w),
            Some(h),
            text_ctx,
        );
        let node = arena.get(child);
        let (main_d, is_fill, m) = match dir {
            Direction::Horizontal => (
                d.width + node.style.margin.horizontal(),
                node.style.width.is_fill(),
                node.style.margin.horizontal(),
            ),
            Direction::Vertical => (
                d.height + node.style.margin.vertical(),
                node.style.height.is_fill(),
                node.style.margin.vertical(),
            ),
        };

        desired_main.push(main_d);
        is_fills.push(is_fill);
        shrinks.push(node.style.shrink);
        margins.push(m);
        desired_sizes.push(d);
    }

    let (avail_main, avail_cross) = match dir {
        Direction::Horizontal => (w, h),
        Direction::Vertical => (h, w),
    };

    let plan = allocate_main_axis(
        avail_main,
        &desired_main,
        &is_fills,
        &shrinks,
        gap,
        &margins,
        dist,
    );

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

    let node_abs = Point::new(
        parent_abs.x + arena.get(node_id).resolved_rect.x,
        parent_abs.y + arena.get(node_id).resolved_rect.y,
    );

    for (i, &child) in normal.iter().enumerate() {
        let main_alloc = plan.sizes[i];
        let child_desired = desired_sizes[i];

        let (child_m, is_cross_fixed, is_cross_fill) = {
            let n = arena.get(child);
            (
                n.style.margin,
                match dir {
                    Direction::Horizontal => {
                        matches!(n.style.height, Size::Fixed(_) | Size::Percent(_))
                    }
                    Direction::Vertical => {
                        matches!(n.style.width, Size::Fixed(_) | Size::Percent(_))
                    }
                },
                match dir {
                    Direction::Horizontal => n.style.height.is_fill(),
                    Direction::Vertical => n.style.width.is_fill(),
                },
            )
        };

        let (cross_m_start, cross_m_end, cross_m_total, main_m_start) = match dir {
            Direction::Horizontal => (
                child_m.top,
                child_m.bottom,
                child_m.vertical(),
                child_m.left,
            ),
            Direction::Vertical => (
                child_m.left,
                child_m.right,
                child_m.horizontal(),
                child_m.top,
            ),
        };

        let base_cross = match dir {
            Direction::Horizontal => child_desired.height + cross_m_total,
            Direction::Vertical => child_desired.width + cross_m_total,
        };

        let (cross_alloc, cross_offset) = resolve_cross_axis(
            align,
            avail_cross,
            base_cross,
            cross_m_start,
            cross_m_end,
            is_cross_fixed,
            is_cross_fill,
        );
        let main_offset = cursor + main_m_start;

        let rect = match dir {
            Direction::Horizontal => ResolvedRect::new(
                main_offset,
                cross_offset,
                (main_alloc - child_m.horizontal()).max(0.0),
                (cross_alloc - cross_m_total).max(0.0),
            ),
            Direction::Vertical => ResolvedRect::new(
                cross_offset,
                main_offset,
                (cross_alloc - cross_m_total).max(0.0),
                (main_alloc - child_m.vertical()).max(0.0),
            ),
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
        match w_size {
            Size::Fixed(v) => v.min(constraints.max_width),
            Size::Percent(p) if constraints.is_tight_width() => {
                (constraints.max_width * p).min(constraints.max_width)
            }
            _ => constraints.max_width,
        },
        match h_size {
            Size::Fixed(v) => v.min(constraints.max_height),
            Size::Percent(p) if constraints.is_tight_height() => {
                (constraints.max_height * p).min(constraints.max_height)
            }
            _ => constraints.max_height,
        },
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
