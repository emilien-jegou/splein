// Single responsibility: Intrinsic sizing, text measurement, and desired dimension derivation.

use crate::foundation::{Constraints, DesiredSize, IntrinsicSize, LayoutPresence, Size};
use crate::layout::group_measure::measure_group_intrinsic;
use crate::text::TextContext;
use crate::tree::kind::MeasureCtx;
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
    let style = arena.get(node_id).style.clone();
    if let (Size::Fixed(w), Size::Fixed(h)) = (style.width, style.height) {
        return IntrinsicSize { width: w, height: h };
    }

    if let Some(cached) = arena.get(node_id).state.core.cache.get_intrinsic(&constraints, for_height) {
        return cached;
    }

    measure_intrinsic_uncached(arena, node_id, constraints, text_ctx, for_height)
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

    let style = arena.get(node_id).style.clone();
    let node = arena.get_mut(node_id);
    let kind = &node.kind;
    let state = &mut node.state;
    let Some(primitive) = kind.primitive() else {
        return IntrinsicSize { width: 0.0, height: 0.0 };
    };
    let raw = primitive.measure(
        &style,
        state,
        &MeasureCtx { constraints, for_height, text: text_ctx },
    );

    let pad = style.padding;
    let intrinsic = IntrinsicSize {
        width: raw.width + pad.horizontal(),
        height: raw.height + pad.vertical(),
    };
    node.state.core.cache.store_intrinsic(constraints, for_height, intrinsic);
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
    let style = arena.get(id).style.clone();
    if !arena.get(id).kind.is_leaf() {
        return;
    }
    let node = arena.get_mut(id);
    let kind = &node.kind;
    let state = &mut node.state;
    if let Some(primitive) = kind.primitive() {
        primitive.refit_to_width(&style, state, width, text_ctx);
    }
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
