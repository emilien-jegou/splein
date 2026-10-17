// Single responsibility: Retained arena node aggregating style, geometry, and runtime state.

use crate::foundation::{Key, Point, ResolvedRect, Size, StrokeAlign, Transform};
use crate::scene::RectSettle;
use crate::tree::kind::NodeKind;
use crate::tree::state::NodeState;
use crate::tree::style::NodeStyle;
use std::ops::{Deref, DerefMut};

/// Lowest z-index an overlay may declare, so it always paints above ordinary content.
pub const OVERLAY_Z_FLOOR: i32 = 100;

/// Where a node paints relative to its siblings, and in what order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PaintLayer {
    /// Painted before the parent's own content, in ascending z order.
    Below(i32),
    /// Painted after the parent's own content, in ascending z order.
    Above(i32),
}

impl PaintLayer {
    /// The sort key within this layer; siblings compare on it directly.
    #[inline(always)]
    pub fn z(self) -> i32 {
        match self {
            Self::Below(z) | Self::Above(z) => z,
        }
    }
}

/// Retained tree node composed of styling intent, computed geometry, and engine state.
#[derive(Default)]
pub struct LayoutNode {
    pub key: Option<Key>,
    pub kind: NodeKind,
    pub style: NodeStyle,
    /// The node's own box, in its parent's coordinates. What layout produces.
    pub resolved_rect: ResolvedRect,
    /// The same box in window coordinates.
    ///
    /// Layout computes this on the way down anyway, to place children and to report bounds, so
    /// keeping it costs nothing. It exists because a component that wants to know where it ended
    /// up otherwise has to walk its ancestor chain and sum offsets, which it cannot do: it holds
    /// no reference to the tree. A pointer arrives in window coordinates, so anything comparing
    /// layout against a gesture needs this frame rather than the parent-relative one.
    pub absolute_rect: ResolvedRect,
    pub transform: Transform,
    /// Cached screen-space transform [STAGE1 #15]; valid when `abs_tx_frame`
    /// equals the arena's current frame (see `TreeArena::tx_frame`).
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

    /// An independent 2D layout boundary isolates layout changes within its subtree.
    /// Requires fixed dimensions on both axes and non-absolute positioning.
    #[inline]
    pub fn is_layout_boundary(&self) -> bool {
        !self.style.is_absolute
            && matches!(self.style.width, Size::Fixed(_))
            && matches!(self.style.height, Size::Fixed(_))
    }

    /// The transform this node paints with, pivoted about its centre so scale and rotation do
    /// not drift toward the top-left origin.
    #[inline(always)]
    pub fn effective_transform(&self) -> Transform {
        let combined = self.transform;
        // Pure translation needs no pivot; skipping it also keeps it free of float drift.
        let linear = Transform {
            tx: 0.0,
            ty: 0.0,
            ..combined
        };
        if linear == Transform::IDENTITY {
            return combined;
        }
        let (cx, cy) = (
            self.resolved_rect.width * 0.5,
            self.resolved_rect.height * 0.5,
        );
        Transform {
            tx: combined.tx + cx - (combined.a * cx + combined.c * cy),
            ty: combined.ty + cy - (combined.b * cx + combined.d * cy),
            ..combined
        }
    }

    /// The opacity this node paints with.
    #[inline(always)]
    pub fn effective_opacity(&self) -> f32 {
        self.style.appearance.opacity
    }

    /// Whether the node must be isolated into its own stacking context.
    #[inline]
    pub fn creates_stacking_context(&self) -> bool {
        self.style.z_index != 0
            || self.style.is_overlay
            || self.effective_opacity() < 1.0
            || self.effective_transform() != Transform::IDENTITY
    }

    /// Painter layer this node occupies relative to its siblings.
    ///
    /// Painting and hit testing must agree on this or a click lands on whatever happens to
    /// be painted last rather than whatever is on top, so both read it from here instead of
    /// repeating the partition rule.
    #[inline]
    pub fn paint_layer(&self) -> PaintLayer {
        let z = if self.style.is_overlay {
            self.style.z_index.max(OVERLAY_Z_FLOOR)
        } else {
            self.style.z_index
        };
        if z < 0 {
            PaintLayer::Below(z)
        } else {
            PaintLayer::Above(z)
        }
    }

    /// Axis-aligned box of a local rect projected through `tx`, used for screen-space bounds.
    ///
    /// Inflated by `AA_PAD` on every side: rasterizers (tiny-skia, Vello) emit
    /// antialiased edge pixels up to 1px outside the geometric box (rounded
    /// corners, glyph edges, strokes). Damage culled on the tight box would
    /// leave a 1px ghost fringe; the pad makes culling conservative.
    pub const AA_PAD: f32 = 1.0;
    fn project(tx: &Transform, x: f32, y: f32, w: f32, h: f32) -> ResolvedRect {
        Self::project_aa(tx, x, y, w, h, Self::AA_PAD)
    }

    /// Projected box with explicit extra padding (for callers that already pad).
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
        ResolvedRect::new(
            min_x - pad,
            min_y - pad,
            (max_x - min_x) + 2.0 * pad,
            (max_y - min_y) + 2.0 * pad,
        )
    }

    /// Bounds of this node's content alone, sized by the flow box rather than the render box.
    ///
    /// This is what changes when layout re-flows the interior while the render box is
    /// stationary, so damage must compare it separately. `tx` is the node's own transform,
    /// so the node's motion (scale, rotation) is included exactly once.
    ///
    /// Implemented as [`Self::compute_visual_bounds`]: both size from
    /// `resolved_rect` with identical stroke/shadow pads, so a separate
    /// projection would recompute the same box (N1). Kept as a named alias
    /// so call sites keep reading intent; if flow and render boxes ever
    /// diverge again, this is the single place to re-split them.
    pub fn compute_flow_bounds(
        &self,
        tx: &Transform,
        active_clip: Option<ResolvedRect>,
    ) -> ResolvedRect {
        self.compute_visual_bounds(tx, active_clip)
    }

    /// Box that must be repainted when this node's geometry changes.
    ///
    /// A node paints its content at the *flow* size but is clipped to the *render* size,
    /// so the pixels it can own span both. When only the flow box changes the render box
    /// has not moved yet, and visual bounds alone would look unchanged while the content
    /// has already shrunk. Nodes with no render box take the plain visual path, so the
    /// common case is unaffected.

    /// Computes the laid-out box projected through `tx`, with no coverage pad.
    ///
    /// Same projection as [`compute_visual_bounds`](Self::compute_visual_bounds) but tight:
    /// no AA pad, no stroke, no shadow. This is the measurement answer — it agrees with the
    /// geometry snapshot the publish pass writes (which stores layout boxes), so a handle
    /// and the engine report the same box once nothing is in flight. Mid-flight it still
    /// tracks the live transform, unlike the stored box, which is layout-time.
    pub fn compute_layout_bounds(&self, tx: &Transform) -> ResolvedRect {
        let (w, h) = (self.resolved_rect.width, self.resolved_rect.height);
        Self::project_aa(tx, 0.0, 0.0, w, h, 0.0)
    }

    /// Computes the visual screen-space bounds projected through `tx`, with strokes,
    /// shadows and ancestor clipping folded in.
    pub fn compute_visual_bounds(
        &self,
        tx: &Transform,
        active_clip: Option<ResolvedRect>,
    ) -> ResolvedRect {
        // Sized from the render box so damage and clipping track what is on screen,
        // while the origin comes from the transform chain built on the same box.
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

        if let Some(shadow) = &self.style.appearance.shadow {
            for layer in shadow.layers() {
                let (ox, oy) = shadow.layer_offset(&self.state.core.resolved_light, layer);
                let pad = shadow.layer_pad(layer);
                let shadow_rect =
                    Self::project(tx, ox - pad, oy - pad, w + 2.0 * pad, h + 2.0 * pad);
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
