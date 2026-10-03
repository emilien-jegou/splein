// Single responsibility: Intrinsic sizing, text measurement, and desired dimension derivation.

use crate::foundation::{Constraints, DesiredSize, IntrinsicSize, Size};
use crate::layout::group_measure::measure_group_intrinsic;
use crate::text::TextContext;
use crate::tree::{NodeId, NodeKind, TreeArena};

/// Derives resolved pixel size from sizing intent, intrinsic size, and parent bound.
pub fn resolve_desired_dimension(size: Size, intrinsic: f32, parent_dim: Option<f32>) -> f32 {
    let safe_intrinsic = if intrinsic.is_finite() {
        intrinsic.max(0.0)
    } else {
        0.0
    };
    match size {
        Size::Fixed(px) => {
            if px.is_finite() {
                px.max(0.0)
            } else {
                0.0
            }
        }
        Size::Fit => safe_intrinsic,
        Size::Fill => 0.0,
        Size::Percent(pct) => parent_dim
            .filter(|p| p.is_finite() && *p >= 0.0)
            .map_or(0.0, |p| {
                let safe_pct = if pct.is_finite() { pct.max(0.0) } else { 0.0 };
                (p * safe_pct).max(0.0)
            }),
    }
}

/// Computes intrinsic bounding size, fast-pathing cache hits with zero tracing overhead.
#[inline(always)]
pub fn compute_intrinsic(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
) -> IntrinsicSize {
    let (w_size, h_size, is_text) = {
        let n = arena.get(node_id);
        (
            n.style.width,
            n.style.height,
            matches!(n.kind, NodeKind::Text(_)),
        )
    };

    // 1. O(1) Fixed dimension short-circuit (0 allocations, 0 child traversals)
    if let (Size::Fixed(w), Size::Fixed(h)) = (w_size, h_size) {
        return IntrinsicSize {
            width: w,
            height: h,
        };
    }

    // Text wraps against its inline width; groups measure unconstrained.
    let max_width = if is_text {
        text_inline_width(w_size, constraints.max_width)
    } else {
        f32::INFINITY
    };

    let effective_constraints = Constraints {
        min_width: 0.0,
        max_width,
        min_height: 0.0,
        max_height: f32::INFINITY,
    };

    // 2. O(1) Cache Hit (0 tracing overhead)
    if let Some(cached) = arena
        .get(node_id)
        .state
        .cache
        .get_intrinsic(&effective_constraints)
    {
        return cached;
    }

    // 3. Uncached Intrinsic Calculation (Explicitly Instrumented)
    measure_intrinsic_uncached(arena, node_id, effective_constraints, text_ctx)
}

/// Inline wrap width for a text node from its width intent and available space.
fn text_inline_width(width: Size, available: f32) -> f32 {
    match width {
        Size::Fit => f32::INFINITY,
        Size::Fill => {
            if available.is_finite() {
                available.max(0.0)
            } else {
                f32::INFINITY
            }
        }
        Size::Fixed(value) => {
            if available.is_finite() {
                value.min(available).max(0.0)
            } else {
                value.max(0.0)
            }
        }
        Size::Percent(ratio) => {
            if available.is_finite() {
                (available * ratio).max(0.0)
            } else {
                f32::INFINITY
            }
        }
    }
}

#[tracing::instrument(level = "trace", skip(arena, text_ctx), fields(node = node_id.index))]
fn measure_intrinsic_uncached(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
) -> IntrinsicSize {
    if matches!(arena.get(node_id).kind, NodeKind::Group) {
        let intrinsic = measure_group_intrinsic(arena, node_id, constraints, text_ctx);
        arena
            .get_mut(node_id)
            .state
            .cache
            .store_intrinsic(constraints, intrinsic);
        return intrinsic;
    }

    let node = arena.get(node_id);
    let (intrinsic, shaped_layout) = match &node.kind {
        NodeKind::Text(t) => {
            let layout = text_ctx.shape_config(t, constraints);
            let size = layout.total_size;
            (size, Some(layout))
        }
        NodeKind::Custom(p) => (p.measure(constraints), None),
        NodeKind::Group => unreachable!(),
    };

    let target_node = arena.get_mut(node_id);
    target_node
        .state
        .cache
        .store_intrinsic(constraints, intrinsic);
    if let Some(layout) = shaped_layout {
        target_node.state.cached_text_layout = Some(layout);
    }

    intrinsic
}

/// Derives desired width and height for a child in O(1) without subtree recursion when fixed.
pub fn compute_child_desired(
    arena: &mut TreeArena,
    child_id: NodeId,
    constraints: Constraints,
    parent_width: Option<f32>,
    parent_height: Option<f32>,
    text_ctx: &TextContext,
) -> DesiredSize {
    let node = arena.get(child_id);
    let (w_size, h_size) = (node.style.width, node.style.height);

    let needs_intrinsic_w = w_size.is_fit();
    let needs_intrinsic_h = h_size.is_fit();

    let intrinsic = if needs_intrinsic_w || needs_intrinsic_h {
        compute_intrinsic(arena, child_id, constraints, text_ctx)
    } else {
        IntrinsicSize {
            width: 0.0,
            height: 0.0,
        }
    };

    DesiredSize {
        width: resolve_desired_dimension(w_size, intrinsic.width, parent_width),
        height: resolve_desired_dimension(h_size, intrinsic.height, parent_height),
    }
}
