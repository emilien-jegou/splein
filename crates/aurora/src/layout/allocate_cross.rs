// Single responsibility: Cross-axis alignment and bounded allocation preserving explicit child sizes.

use crate::foundation::{Alignment, Size};

/// Resolves a child's final cross-axis content size and offset from its sizing intent.
///
/// The cross axis is orthogonal to flex distribution: for a vertical container it is
/// width (which governs text wrapping), for a horizontal container it is height. The
/// returned content size excludes margins; the offset includes the start margin.
pub fn resolve_child_cross(
    alignment: Alignment,
    parent_cross: f32,
    cross_size: Size,
    intrinsic_cross: f32,
    m_start: f32,
    m_end: f32,
) -> (f32, f32) {
    let m_total = m_start + m_end;
    let is_fill = cross_size.is_fill();
    let is_fixed = matches!(cross_size, Size::Fixed(_) | Size::Percent(_));

    let content = match cross_size {
        Size::Fill => (parent_cross - m_total).max(0.0),
        Size::Percent(ratio) if parent_cross.is_finite() => {
            (parent_cross * ratio - m_total).max(0.0)
        }
        Size::Fixed(value) => value.max(0.0),
        Size::Fit | Size::Percent(_) => intrinsic_cross.max(0.0),
    };

    let (alloc, offset) = resolve_cross_axis(
        alignment,
        parent_cross,
        content + m_total,
        m_start,
        m_end,
        is_fixed,
        is_fill,
    );
    ((alloc - m_total).max(0.0), offset)
}

/// Resolves cross-axis allocated dimension and coordinate offset within container bound.
pub fn resolve_cross_axis(
    alignment: Alignment,
    parent_cross: f32,
    child_outer_cross: f32,
    m_start: f32,
    m_end: f32,
    child_is_fixed: bool,
    child_is_fill: bool,
) -> (f32, f32) {
    if child_is_fill && parent_cross.is_finite() {
        return (parent_cross, m_start);
    }

    match alignment {
        Alignment::Stretch if !child_is_fixed && parent_cross.is_finite() => {
            (parent_cross.max(child_outer_cross), m_start)
        }
        Alignment::Center if parent_cross.is_finite() => {
            let offset =
                ((parent_cross - child_outer_cross) * 0.5 + (m_start - m_end) * 0.5).max(m_start);
            (child_outer_cross, offset)
        }
        Alignment::End if parent_cross.is_finite() => {
            let offset = (parent_cross - child_outer_cross - m_end).max(m_start);
            (child_outer_cross, offset)
        }
        _ => (child_outer_cross, m_start),
    }
}
