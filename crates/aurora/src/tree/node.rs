// Single responsibility: Retained arena node aggregating style, geometry, and runtime state.

use std::ops::{Deref, DerefMut};
use crate::foundation::{Key, Point, ResolvedRect, Size, Transform};
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

    /// Effective transform composing declarative style with motion overrides, pivoted about the
    /// node's centre so scale and rotation do not drift toward the top-left origin.
    #[inline(always)]
    pub fn effective_transform(&self) -> Transform {
        let combined = self.state.motion.compose_transform(self.transform);
        // Pure translation needs no pivot; skipping it also keeps it free of float drift.
        let linear = Transform { tx: 0.0, ty: 0.0, ..combined };
        if linear == Transform::IDENTITY {
            return combined;
        }
        let (cx, cy) = (self.resolved_rect.width * 0.5, self.resolved_rect.height * 0.5);
        Transform {
            tx: combined.tx + cx - (combined.a * cx + combined.c * cy),
            ty: combined.ty + cy - (combined.b * cx + combined.d * cy),
            ..combined
        }
    }

    /// Effective opacity composing declarative appearance with motion overrides.
    #[inline(always)]
    pub fn effective_opacity(&self) -> f32 {
        self.state.motion.compose_opacity(self.style.appearance.opacity)
    }

    /// Whether the node must be isolated into its own stacking context.
    #[inline]
    pub fn creates_stacking_context(&self) -> bool {
        self.style.z_index != 0
            || self.style.is_overlay
            || self.effective_opacity() < 1.0
            || self.effective_transform() != Transform::IDENTITY
    }

    /// Axis-aligned box of a local rect projected through `tx`, used for screen-space bounds.
    fn project(tx: &Transform, x: f32, y: f32, w: f32, h: f32) -> ResolvedRect {
        let corners = [
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x, y + h),
            Point::new(x + w, y + h),
        ];
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for corner in corners {
            let p = tx.transform_point(corner);
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        ResolvedRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Computes the visual screen-space bounds projected through `tx`, with strokes,
    /// shadows and ancestor clipping folded in.
    pub fn compute_visual_bounds(
        &self,
        tx: &Transform,
        active_clip: Option<ResolvedRect>,
    ) -> ResolvedRect {
        use crate::foundation::{ShadowKind, StrokeAlign};

        let (w, h) = (self.resolved_rect.width, self.resolved_rect.height);
        let stroke_pad = match &self.style.appearance.stroke {
            Some(stroke) => match stroke.align {
                StrokeAlign::Outside => stroke.width,
                StrokeAlign::Center => stroke.width * 0.5,
                StrokeAlign::Inside => 0.0,
            },
            None => 0.0,
        };

        let mut bounds = Self::project(
            tx,
            -stroke_pad,
            -stroke_pad,
            w + 2.0 * stroke_pad,
            h + 2.0 * stroke_pad,
        );

        for shadow in &self.style.appearance.shadows {
            if shadow.kind == ShadowKind::Outer {
                let pad = shadow.spread + 3.0 * shadow.blur;
                let shadow_rect = Self::project(
                    tx,
                    shadow.offset_x - pad,
                    shadow.offset_y - pad,
                    w + 2.0 * pad,
                    h + 2.0 * pad,
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
