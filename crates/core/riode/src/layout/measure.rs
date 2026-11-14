// Single responsibility: Intrinsic sizing, text measurement, and desired dimension derivation.

use crate::foundation::{Constraints, DesiredSize, IntrinsicSize, LayoutPresence, Size};
use crate::layout::group_measure::measure_group_intrinsic;
use crate::text::TextContext;
use crate::tree::TreeArena;
use crate::tree::NodeId;

pub fn resolve_desired_dimension(size: Size, intrinsic: f32, parent_dim: Option<f32>) -> f32 {
    let safe_intrinsic = if intrinsic.is_finite() { intrinsic.max(0.0) } else { 0.0 };
    match size {
        Size::Fixed(px) => if px.is_finite() { px.max(0.0) } else { 0.0 },
        Size::Fit | Size::Fold => safe_intrinsic,
        Size::Fill => parent_dim.filter(|p| p.is_finite() && *p >= 0.0).unwrap_or(0.0),
        Size::Percent(pct) => parent_dim
            .filter(|p| p.is_finite() && *p >= 0.0)
            .map_or(0.0, |p| {
                let safe_pct = if pct.is_finite() { pct.max(0.0) } else { 0.0 };
                (p * safe_pct).max(0.0)
            }),
    }
}

pub fn compute_intrinsic(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    for_height: bool,
) -> IntrinsicSize {
    let (w_size, h_size, is_text) = {
        let n = arena.get(node_id);
        (n.style.width, n.style.height, n.kind.is_text())
    };

    if let (Size::Fixed(w), Size::Fixed(h)) = (w_size, h_size) {
        return IntrinsicSize { width: w, height: h };
    }

    let effective_constraints = if is_text {
        let wrap_w = arena.get(node_id).kind.text_content().map_or(constraints.max_width, |_| {
            if let crate::tree::NodeKind::Text(t) = &arena.get(node_id).kind {
                crate::text::TextConfig::wrap_width(t, arena.get(node_id).style.width, text_ctx, constraints, for_height)
            } else {
                constraints.max_width
            }
        });
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

    if let Some(cached) = arena.get(node_id).state.core.cache.get_intrinsic(&effective_constraints, for_height) {
        return cached;
    }

    measure_intrinsic_uncached(arena, node_id, effective_constraints, text_ctx, for_height)
}

pub fn compute_intrinsic_with_constraints(
    arena: &mut TreeArena,
    node_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    for_height: bool,
) -> IntrinsicSize {
    if let Some(cached) = arena.get(node_id).state.core.cache.get_intrinsic(&constraints, for_height) {
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
    if arena.get(node_id).kind.is_group() {
        let mut intrinsic = measure_group_intrinsic(arena, node_id, constraints, text_ctx, for_height);
        if let Some(h) = aspect_fit_height(&arena.get(node_id).style, constraints) {
            intrinsic.height = h.min(constraints.max_height);
        }
        arena.get_mut(node_id).state.core.cache.store_intrinsic(constraints, for_height, intrinsic);
        return intrinsic;
    }

    let node = arena.get(node_id);
    let (intrinsic, shaped_layout) = node.kind.measure_leaf(constraints, text_ctx)
        .expect("measure_leaf must handle non-group nodes");

    let pad = arena.get(node_id).style.padding;
    let intrinsic = IntrinsicSize {
        width: intrinsic.width + pad.horizontal(),
        height: intrinsic.height + pad.vertical(),
    };

    let target_node = arena.get_mut(node_id);
    target_node.state.core.cache.store_intrinsic(constraints, for_height, intrinsic);
    if let Some(layout) = shaped_layout {
        target_node.state.text.cached_text_layout = Some(layout);
    }

    intrinsic
}

pub(crate) fn aspect_fit_height(
    style: &crate::tree::NodeStyle,
    constraints: Constraints,
) -> Option<f32> {
    let ratio = style.aspect_ratio.filter(|r| r.is_finite() && *r > 0.0)?;
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

pub fn fit_text_run_to_width(
    arena: &mut TreeArena,
    id: NodeId,
    width: f32,
    text_ctx: &TextContext,
) {
    let config = match &arena.get(id).kind {
        crate::tree::NodeKind::Text(config) => {
            let force = arena.get(id).style.width.is_fold();
            let cached = arena.get(id).state.text.cached_text_layout.as_deref();
            if !force && !crate::text::TextConfig::needs_refit(cached, width) {
                return;
            }
            config.clone()
        }
        _ => return,
    };
    let layout = config.refit(text_ctx, width);
    arena.get_mut(id).state.text.cached_text_layout = Some(layout);
}

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
    if arena.get(child_id).state.core.presence == LayoutPresence::Absent {
        return DesiredSize { width: 0.0, height: 0.0 };
    }

    let node = arena.get(child_id);
    let (w_size, h_size) = (node.style.width, node.style.height);

    let needs_intrinsic_w = w_size.is_fit();
    let needs_intrinsic_h = h_size.is_fit();

    let intrinsic = if needs_intrinsic_w || needs_intrinsic_h {
        compute_intrinsic(arena, child_id, constraints, text_ctx, for_height)
    } else {
        IntrinsicSize { width: 0.0, height: 0.0 }
    };

    DesiredSize {
        width: resolve_desired_dimension(w_size, intrinsic.width, parent_width),
        height: resolve_desired_dimension(h_size, intrinsic.height, parent_height),
    }
}
