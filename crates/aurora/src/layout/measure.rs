// Single responsibility: Intrinsic sizing, text measurement, and desired dimension derivation.

use crate::foundation::{Constraints, DesiredSize, IntrinsicSize, LayoutPresence, Size};
use crate::layout::group_measure::measure_group_intrinsic;
use crate::text::TextContext;
use crate::tree::TreeArena;
use crate::tree::{NodeId, NodeKind};

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
///
/// `for_height` selects the query role (see `TextConfig::wrap_width`): `false`
/// sizes an item for row allocation (max-content), `true` sizes a fit box's
/// wrapped height (clamped at the offer).
#[inline(always)]
pub fn compute_intrinsic(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    for_height: bool,
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

    // Text wraps against its inline width; a group measures against the offered
    // bounds (never widened past them), unless the parent has already fixed one,
    // in which case that used width must stay binding for descendants. Groups no
    // longer re-widen to infinity here: an ancestor boundary must reach every
    // descendant, or nested `Fit` subtrees expand forever (the fit-vs-wrap bug).
    // `Fit` text clamps `max(min-content, min(max-content, available))`, so bounded
    // parents force wrapping instead of expanding forever (see `TextConfig::wrap_width`).
    let effective_constraints = if is_text {
        let wrap_w = {
            let n = arena.get(node_id);
            match &n.kind {
                NodeKind::Text(t) => crate::text::TextConfig::wrap_width(
                    t,
                    n.style.width,
                    text_ctx,
                    constraints,
                    for_height,
                ),
                _ => constraints.max_width,
            }
        };
        Constraints {
            min_width: 0.0,
            max_width: wrap_w,
            min_height: 0.0,
            max_height: f32::INFINITY,
        }
    } else if constraints.is_tight_width() {
        Constraints::tight_width(constraints.max_width.max(0.0))
    } else {
        Constraints::loose(constraints.max_width, constraints.max_height)
    };

    // 2. O(1) Cache Hit (0 tracing overhead)
    if let Some(cached) = arena
        .get(node_id)
        .state
        .core
        .cache
        .get_intrinsic(&effective_constraints, for_height)
    {
        return cached;
    }

    // 3. Uncached Intrinsic Calculation (Explicitly Instrumented)
    measure_intrinsic_uncached(arena, node_id, effective_constraints, text_ctx, for_height)
}

/// Computes intrinsic size under explicit constraints, reusing the memoized measurement.
pub fn compute_intrinsic_with_constraints(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    for_height: bool,
) -> IntrinsicSize {
    if let Some(cached) = arena
        .get(node_id)
        .state
        .core
        .cache
        .get_intrinsic(&constraints, for_height)
    {
        return cached;
    }
    measure_intrinsic_uncached(arena, node_id, constraints, text_ctx, for_height)
}

#[tracing::instrument(level = "trace", skip(arena, text_ctx), fields(node = node_id.index))]
fn measure_intrinsic_uncached(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    for_height: bool,
) -> IntrinsicSize {
    if matches!(arena.get(node_id).kind, NodeKind::Group) {
        let mut intrinsic = measure_group_intrinsic(arena, node_id, constraints, text_ctx, for_height);
        // Aspect boxes report ratio height as intrinsic so parents slot them
        // final (see `aspect_fit_height`).
        if let Some(h) = aspect_fit_height(&arena.get(node_id).style, constraints) {
            intrinsic.height = h.min(constraints.max_height);
        }
        arena
            .get_mut(node_id)
            .state
            .core
            .cache
            .store_intrinsic(constraints, for_height, intrinsic);
        return intrinsic;
    }

    let node = arena.get(node_id);
    let (intrinsic, shaped_layout) = match &node.kind {
        NodeKind::Text(t) => {
            let (size, layout) = t.measure(text_ctx, constraints);
            (size, Some(layout))
        }
        NodeKind::Custom(p) => (p.measure(constraints), None),
        NodeKind::Group => unreachable!(),
    };
    // Padding grows leaf boxes exactly like group boxes: the content measures
    // first, then the inset is added around it.
    let pad = arena.get(node_id).style.padding;
    let intrinsic = IntrinsicSize {
        width: intrinsic.width + pad.horizontal(),
        height: intrinsic.height + pad.vertical(),
    };

    let target_node = arena.get_mut(node_id);
    target_node
        .state
        .core
        .cache
        .store_intrinsic(constraints, for_height, intrinsic);
    if let Some(layout) = shaped_layout {
        target_node.state.text.cached_text_layout = Some(layout);
    }

    intrinsic
}

/// Aspect-derived height for a `Fit`-height box with a determined width.
///
/// A `320×Fit` card at 16/9 is 180 tall no matter what its (empty) content
/// measures. This feeds child *desired* sizes so parents slot aspect boxes at
/// their final height; otherwise the slot is 0 tall, the tight follow-up
/// clamps to it, and the aspect backstop in `resolve_node_bounds` can never
/// recover. `Fit×Fit` stays content-sized; unbounded `Fill` stays unbounded.
/// Min-content floors still win downstream (tall content overflows the ratio
/// rather than crushing).
pub(crate) fn aspect_fit_height(
    style: &crate::tree::NodeStyle,
    constraints: Constraints,
) -> Option<f32> {
    let ratio = style
        .aspect_ratio
        .filter(|r| r.is_finite() && *r > 0.0)?;
    if !style.height.is_fit() {
        return None;
    }
    let w = match style.width {
        Size::Fixed(v) if v.is_finite() => Some(v.max(0.0)),
        Size::Percent(p) if constraints.max_width.is_finite() => {
            Some((constraints.max_width * p).max(0.0))
        }
        Size::Fill if constraints.max_width.is_finite() => Some(constraints.max_width.max(0.0)),
        _ => None,
    }?;
    Some(w / ratio)
}

/// Re-shapes a text run when its cached layout is wider than the inline size it finally received.
///
/// A `Fit` item keeps max-content width for row allocation, so a sibling can
/// still shrink its box below the cached run; the run then re-shapes at the
/// assigned width and paints wrapped inside its own box. Shaping is memoized
/// by content and width, so the common case costs one cache lookup and no
/// shaping.
pub fn fit_text_run_to_width(
    arena: &mut TreeArena,
    id: NodeId,
    width: f32,
    text_ctx: &TextContext,
) {
    let config = match &arena.get(id).kind {
        NodeKind::Text(config) => {
            let cached = arena.get(id).state.text.cached_text_layout.as_deref();
            if !crate::text::TextConfig::needs_refit(cached, width) {
                return;
            }
            config.clone()
        }
        _ => return,
    };
    let layout = config.refit(text_ctx, width);
    arena.get_mut(id).state.text.cached_text_layout = Some(layout);
}

/// Derives desired width and height for a child in O(1) without subtree recursion when fixed.
///
/// Item sizing (max-content role); container-height probes use
/// [`compute_child_desired_for_height`].
pub fn compute_child_desired(
    arena: &mut TreeArena,
    child_id: NodeId,
    constraints: Constraints,
    parent_width: Option<f32>,
    parent_height: Option<f32>,
    text_ctx: &TextContext,
) -> DesiredSize {
    compute_child_desired_inner(arena, child_id, constraints, parent_width, parent_height, text_ctx, false)
}

/// Desired size for container-height sizing: children wrap at the offer.
///
/// Used by height probes (`resolve_node_bounds`) and wrapping column bands so
/// nested text reports wrapped heights instead of single-line ones.
pub(crate) fn compute_child_desired_for_height(
    arena: &mut TreeArena,
    child_id: NodeId,
    constraints: Constraints,
    parent_width: Option<f32>,
    parent_height: Option<f32>,
    text_ctx: &TextContext,
) -> DesiredSize {
    compute_child_desired_inner(arena, child_id, constraints, parent_width, parent_height, text_ctx, true)
}

fn compute_child_desired_inner(
    arena: &mut TreeArena,
    child_id: NodeId,
    constraints: Constraints,
    parent_width: Option<f32>,
    parent_height: Option<f32>,
    text_ctx: &TextContext,
    for_height: bool,
) -> DesiredSize {
    // A node presenting no footprint measures to nothing on either axis.
    if arena.get(child_id).state.core.presence == LayoutPresence::Absent {
        return DesiredSize {
            width: 0.0,
            height: 0.0,
        };
    }

    let node = arena.get(child_id);
    let (w_size, h_size) = (node.style.width, node.style.height);

    let needs_intrinsic_w = w_size.is_fit();
    let needs_intrinsic_h = h_size.is_fit();

    let intrinsic = if needs_intrinsic_w || needs_intrinsic_h {
        compute_intrinsic(arena, child_id, constraints, text_ctx, for_height)
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
