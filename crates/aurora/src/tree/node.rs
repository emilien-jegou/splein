// Single responsibility: Retained arena node aggregating style, geometry, and runtime state.

use std::ops::{Deref, DerefMut};
use crate::foundation::{Key, ResolvedRect, Size, Transform};
use crate::tree::kind::NodeKind;
use crate::tree::state::NodeState;
use crate::tree::style::NodeStyle;

/// Retained tree node composed of styling intent, computed geometry, and engine state.
#[derive(Default)]
pub struct LayoutNode {
    pub key: Option<Key>,
    pub kind: NodeKind,
    pub style: NodeStyle,
    pub resolved_rect: ResolvedRect,
    pub transform: Transform,
    pub state: NodeState,
}

impl LayoutNode {
    pub fn new(kind: NodeKind) -> Self {
        Self {
            key: None,
            kind,
            style: NodeStyle::default(),
            resolved_rect: ResolvedRect::ZERO,
            transform: Transform::IDENTITY,
            state: NodeState::default(),
        }
    }

    /// An independent 2D layout boundary isolates layout changes within its subtree.
    /// Requires fixed dimensions on both axes and non-absolute positioning.
    #[inline]
    pub fn is_layout_boundary(&self) -> bool {
        !self.style.is_absolute
            && matches!(self.style.width, Size::Fixed(_))
            && matches!(self.style.height, Size::Fixed(_))
    }

    /// Computes the visual screen-space bounds including outer strokes, shadows, and ancestor clipping.
    pub fn compute_visual_bounds(
        &self,
        abs_pos: crate::foundation::Point,
        active_clip: Option<ResolvedRect>,
    ) -> ResolvedRect {
        use crate::foundation::{ShadowKind, StrokeAlign};

        let mut bounds = ResolvedRect::new(abs_pos.x, abs_pos.y, self.resolved_rect.width, self.resolved_rect.height);

        if let Some(stroke) = &self.style.appearance.stroke {
            let expand_px = match stroke.align {
                StrokeAlign::Outside => stroke.width,
                StrokeAlign::Center => stroke.width * 0.5,
                StrokeAlign::Inside => 0.0,
            };
            bounds = bounds.expand(expand_px);
        }

        for shadow in &self.style.appearance.shadows {
            if shadow.kind == ShadowKind::Outer {
                let pad = shadow.spread + 3.0 * shadow.blur;
                let shadow_rect = ResolvedRect::new(
                    abs_pos.x + shadow.offset_x - pad,
                    abs_pos.y + shadow.offset_y - pad,
                    self.resolved_rect.width + 2.0 * pad,
                    self.resolved_rect.height + 2.0 * pad,
                );
                bounds = bounds.union(&shadow_rect);
            }
        }

        if let Some(clip) = active_clip {
            bounds.intersect(&clip)
        } else {
            bounds
        }
    }
}

impl Deref for LayoutNode {
    type Target = NodeStyle;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.style
    }
}

impl DerefMut for LayoutNode {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.style
    }
}
