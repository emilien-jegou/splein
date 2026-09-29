// Single responsibility: Cross-axis alignment and bounded allocation preserving explicit child sizes.

use crate::foundation::Alignment;

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
    if child_is_fill && parent_cross.is_finite() { return (parent_cross, m_start); }

    match alignment {
        Alignment::Stretch if !child_is_fixed && parent_cross.is_finite() => {
            (parent_cross.max(child_outer_cross), m_start)
        }
        Alignment::Center if parent_cross.is_finite() => {
            let offset = ((parent_cross - child_outer_cross) * 0.5 + (m_start - m_end) * 0.5).max(m_start);
            (child_outer_cross, offset)
        }
        Alignment::End if parent_cross.is_finite() => {
            let offset = (parent_cross - child_outer_cross - m_end).max(m_start);
            (child_outer_cross, offset)
        }
        _ => (child_outer_cross, m_start),
    }
}
