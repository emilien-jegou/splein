// Single responsibility: Retained arena node aggregating style, geometry, and runtime state.

use crate::foundation::{Geo, Key, Point, ResolvedRect, Size, Transform};
use crate::scene::RectSettle;
use crate::tree::kind::NodeKind;
use crate::tree::state::NodeState;
use crate::tree::style::NodeStyle;
use std::ops::{Deref, DerefMut};

pub const OVERLAY_Z_FLOOR: i32 = 100;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PaintLayer {
    Below(i32),
    Above(i32),
}

impl PaintLayer {
    #[inline(always)]
    pub fn z(self) -> i32 {
        match self {
            Self::Below(z) | Self::Above(z) => z,
        }
    }
}

#[derive(Default)]
pub struct LayoutNode {
    pub key: Option<Key>,
    pub kind: NodeKind,
    pub style: NodeStyle,
    pub resolved_rect: ResolvedRect,
    pub absolute_rect: ResolvedRect,
    pub transform: Transform,
    pub abs_tx: Transform,
    pub abs_tx_frame: u64,
    pub state: NodeState,
    pub rect_settle: RectSettle,
}

impl LayoutNode {
    pub fn new(kind: NodeKind) -> Self {
        Self {
            key: None,
            kind,
            style: NodeStyle::default(),
            resolved_rect: ResolvedRect::ZERO,
            absolute_rect: ResolvedRect::ZERO,
            transform: Transform::IDENTITY,
            abs_tx: Transform::IDENTITY,
            abs_tx_frame: 0,
            state: NodeState::default(),
            rect_settle: RectSettle::default(),
        }
    }

    #[inline(always)]
    pub fn is_group(&self) -> bool {
        self.kind.is_group()
    }

    #[inline(always)]
    pub fn is_selectable_text(&self) -> bool {
        self.kind.is_selectable()
    }

    #[inline(always)]
    pub fn text_content(&self) -> Option<String> {
        self.kind.text_content()
    }

    #[inline(always)]
    pub fn is_text_empty(&self) -> bool {
        self.text_content().is_none_or(|s| s.is_empty())
    }

    #[inline(always)]
    pub fn text_len(&self) -> usize {
        self.text_content().map_or(0, |s| s.len())
    }

    #[inline]
    pub fn is_layout_boundary(&self) -> bool {
        !self.style.is_absolute
            && matches!(self.style.width, Size::Fixed(_))
            && matches!(self.style.height, Size::Fixed(_))
    }

    #[inline(always)]
    pub fn effective_transform(&self) -> Transform {
        let combined = self.transform;
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

    #[inline]
    pub fn creates_stacking_context(&self) -> bool {
        self.style.z_index != 0
            || self.style.is_overlay
            || self.effective_transform() != Transform::IDENTITY
            || self.style.paint.material.is_some()
    }

    #[inline]
    pub fn paint_layer(&self) -> PaintLayer {
        let z = if self.style.is_overlay {
            self.style.z_index.max(OVERLAY_Z_FLOOR)
        } else {
            self.style.z_index
        };
        if z < 0 { PaintLayer::Below(z) } else { PaintLayer::Above(z) }
    }

    pub const AA_PAD: f32 = 1.0;

    fn project(tx: &Transform, x: f32, y: f32, w: f32, h: f32) -> ResolvedRect {
        Self::project_aa(tx, x, y, w, h, Self::AA_PAD)
    }

    fn project_aa(tx: &Transform, x: f32, y: f32, w: f32, h: f32, pad: f32) -> ResolvedRect {
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
        ResolvedRect::new(min_x - pad, min_y - pad, (max_x - min_x) + 2.0 * pad, (max_y - min_y) + 2.0 * pad)
    }

    pub fn compute_flow_bounds(&self, tx: &Transform, active_clip: Option<ResolvedRect>) -> ResolvedRect {
        self.compute_visual_bounds(tx, active_clip)
    }

    pub fn compute_layout_bounds(&self, tx: &Transform) -> ResolvedRect {
        let (w, h) = (self.resolved_rect.width, self.resolved_rect.height);
        Self::project_aa(tx, 0.0, 0.0, w, h, 0.0)
    }

    pub fn geo(&self) -> Geo {
        let (w, h) = (self.resolved_rect.width, self.resolved_rect.height);
        Geo::new(w, h, [0.0; 4], 0.0)
    }

    pub fn compute_visual_bounds(&self, tx: &Transform, active_clip: Option<ResolvedRect>) -> ResolvedRect {
        let (w, h) = (self.resolved_rect.width, self.resolved_rect.height);
        let bounds = Self::project(tx, 0.0, 0.0, w, h);
        if let Some(clip) = active_clip { bounds.intersect(&clip) } else { bounds }
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
