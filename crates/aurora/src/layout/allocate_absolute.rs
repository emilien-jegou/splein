// Single responsibility: Immediate-parent absolute anchoring incorporating external child margins.

use crate::foundation::{Anchor, ResolvedRect};
use crate::tree::LayoutNode;

#[derive(Copy, Clone)]
enum Align1D { Start, Center, End }

/// Resolves 1D coordinate with safe-centering and symmetric margin bias.
#[inline]
fn resolve_axis_1d(parent: f32, child: f32, m_start: f32, m_end: f32, align: Align1D) -> f32 {
    match align {
        Align1D::Start => m_start,
        Align1D::End => parent - child - m_end,
        Align1D::Center => {
            if child >= parent {
                m_start
            } else {
                (parent - child) * 0.5 + (m_start - m_end) * 0.5
            }
        }
    }
}

/// Computes child resolved bounds for absolute positioning anchored relative to parent.
pub fn resolve_absolute_child(
    node: &LayoutNode,
    parent_width: f32,
    parent_height: f32,
    desired_w: f32,
    desired_h: f32,
) -> ResolvedRect {
    let m = node.style.margin;
    let anchor = node.style.anchor.unwrap_or(Anchor::TopLeft);

    let (h_align, v_align) = match anchor {
        Anchor::TopLeft => (Align1D::Start, Align1D::Start),
        Anchor::Top => (Align1D::Center, Align1D::Start),
        Anchor::TopRight => (Align1D::End, Align1D::Start),
        Anchor::Left => (Align1D::Start, Align1D::Center),
        Anchor::Center => (Align1D::Center, Align1D::Center),
        Anchor::Right => (Align1D::End, Align1D::Center),
        Anchor::BottomLeft => (Align1D::Start, Align1D::End),
        Anchor::Bottom => (Align1D::Center, Align1D::End),
        Anchor::BottomRight => (Align1D::End, Align1D::End),
    };

    let x = resolve_axis_1d(parent_width, desired_w, m.left, m.right, h_align);
    let y = resolve_axis_1d(parent_height, desired_h, m.top, m.bottom, v_align);

    ResolvedRect::new(x, y, desired_w, desired_h)
}
