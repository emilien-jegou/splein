// Single responsibility: Dynamic property evaluation closures on a layout node.

use crate::foundation::{LayerPaint, LayoutPresence, Size, Transform};
use std::rc::Rc;

/// Inner storage for dynamic property evaluators.
#[derive(Default, Clone)]
pub struct DynamicBindings {
    pub width: Option<Rc<dyn Fn() -> Size>>,
    pub height: Option<Rc<dyn Fn() -> Size>>,
    pub text: Option<Rc<dyn Fn() -> String>>,
    pub transform: Option<Rc<dyn Fn() -> Transform>>,
    /// Paint evaluator, when the node declares any dynamic paint chain
    /// (shape, material, overlay, or effect). The one visual slot on a
    /// node: it evaluates the whole [`LayerPaint`] bundle, so every paint
    /// prop stays reactive through a single channel.
    pub paint: Option<Rc<dyn Fn() -> LayerPaint>>,
    pub presence: Option<Rc<dyn Fn() -> LayoutPresence>>,
    /// Stacking order evaluator, when the node declares a dynamic one.
    pub z_index: Option<Rc<dyn Fn() -> i32>>,
    pub pose: Option<PoseBindings>,
    /// Horizontal scroll offset in logical px, when the node declares one.
    pub scroll_offset_x: Option<Rc<dyn Fn() -> f32>>,
    /// Vertical scroll offset in logical px, when the node declares one.
    /// The scalar `scroll_offset` DSL alias resolves into this slot.
    pub scroll_offset_y: Option<Rc<dyn Fn() -> f32>>,
    /// Retained child segments, when any rebuild from reactive state.
    ///
    /// `None` means static-only (today's fast path: children reconcile once at
    /// mount and never rebuild). `Some` retains the full ordered segment list —
    /// statics included — so the scheduler can re-merge on any nested change.
    /// Reconciling on the merged key sequence rather than on every read is what
    /// makes a reorder cost one diff: an unchanged rebuild re-evaluates to the
    /// same keys and nothing happens.
    pub children: Option<Vec<crate::dsl::group::ChildSegment>>,
}

/// Independent pose components, each bound to its own animated signal.
///
/// Grouped so the pose is one concept in the DSL (`.x() .y() .scale()
/// .rotate_degrees()`) while the tree still stores a single composed transform.
#[derive(Default, Clone)]
pub struct PoseBindings {
    /// Horizontal translation in logical pixels.
    pub x: Option<Rc<dyn Fn() -> f32>>,
    /// Vertical translation in logical pixels.
    pub y: Option<Rc<dyn Fn() -> f32>>,
    /// Uniform scale factor.
    pub scale: Option<Rc<dyn Fn() -> f32>>,
    /// Rotation in degrees.
    pub rotation: Option<Rc<dyn Fn() -> f32>>,
}

impl PoseBindings {
    /// Whether any component is bound.
    #[inline(always)]
    pub fn is_bound(&self) -> bool {
        self.x.is_some() || self.y.is_some() || self.scale.is_some() || self.rotation.is_some()
    }

    /// Composes the components into one transform: translate, then rotate, then scale.
    ///
    /// Runs per frame for every bound node, so it stays allocation-free and skips
    /// components that are at their neutral value.
    pub fn compose(&self) -> Transform {
        let mut out = Transform::IDENTITY;
        if let Some(x) = &self.x {
            let x = x();
            if x != 0.0 {
                out.tx = x;
            }
        }
        if let Some(y) = &self.y {
            let y = y();
            if y != 0.0 {
                out.ty = y;
            }
        }
        if let Some(degrees) = &self.rotation {
            let degrees = degrees();
            if degrees != 0.0 {
                out = out.multiply(&Transform::from_rotation_degrees(degrees));
            }
        }
        if let Some(scale) = &self.scale {
            let scale = scale();
            if scale != 1.0 {
                out = out.multiply(&Transform::from_scale(scale, scale));
            }
        }
        out
    }
}

impl DynamicBindings {
    /// Whether any dimension evaluator is bound.
    #[inline(always)]
    pub fn has_dimension_bindings(&self) -> bool {
        self.width.is_some() || self.height.is_some()
    }

    /// Whether no dynamic evaluator is bound anywhere in the store [S2-2].
    ///
    /// A node whose store is empty never re-evaluates: its properties are
    /// fully described by retained style. (The store itself is always
    /// present — reconciliation wraps even static declarations.)
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.width.is_none()
            && self.height.is_none()
            && self.text.is_none()
            && self.transform.is_none()
            && self.paint.is_none()
            && self.presence.is_none()
            && self.z_index.is_none()
            && self.pose.as_ref().is_none_or(|p| !p.is_bound())
            && self.scroll_offset_x.is_none()
            && self.scroll_offset_y.is_none()
            && self.children.is_none()
    }
}

/// Dynamic property evaluation closures with O(1) single-pointer cloning.
#[derive(Default, Clone)]
pub struct NodeBindings {
    inner: Option<Rc<DynamicBindings>>,
}

impl NodeBindings {
    /// Constructs bindings from a dynamic binding store.
    pub fn from_store(store: DynamicBindings) -> Self {
        Self {
            inner: Some(Rc::new(store)),
        }
    }

    /// Whether this node carries no live evaluators [S2-2]: either no store
    /// at all, or a store whose every slot is `None` (static declarations
    /// still get wrapped — see `DynamicBindings::is_empty`).
    #[inline(always)]
    pub fn is_static(&self) -> bool {
        self.inner.as_ref().is_none_or(|b| b.is_empty())
    }

    /// Whether this node has dynamic dimension evaluators bound.
    #[inline(always)]
    pub fn has_dimension_bindings(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|b| b.has_dimension_bindings())
    }

    /// Whether this node has any pose component evaluator bound.
    #[inline(always)]
    pub fn has_pose_bindings(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|b| b.pose.as_ref().is_some_and(PoseBindings::is_bound))
    }

    /// Whether this node's position is driven by a bound `x` or `y` translation.
    #[inline(always)]
    pub fn has_translation_bindings(&self) -> bool {
        self.inner.as_ref().is_some_and(|b| {
            b.pose
                .as_ref()
                .is_some_and(|p| p.x.is_some() || p.y.is_some())
        })
    }

    /// Composes the bound pose components into a single transform.
    pub fn compose_pose(&self) -> Transform {
        self.inner
            .as_deref()
            .and_then(|b| b.pose.as_ref())
            .map_or(Transform::IDENTITY, PoseBindings::compose)
    }

    /// Builds this node's dynamic child segments from reactive state, if any were declared.
    #[inline(always)]
    pub fn children_builder(&self) -> Option<&Vec<crate::dsl::group::ChildSegment>> {
        self.inner.as_deref().and_then(|b| b.children.as_ref())
    }

    /// Whether any child segment rebuilds from reactive state.
    #[inline(always)]
    pub fn has_children_builder(&self) -> bool {
        self.children_builder().is_some()
    }

    /// Re-evaluates bound width and height dimensions, returning true if either changed.
    pub fn update_layout(&self, width: &mut Size, height: &mut Size) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        let mut changed = false;
        if let Some(w) = &b.width {
            let fresh = w();
            if *width != fresh {
                *width = fresh;
                changed = true;
            }
        }
        if let Some(h) = &b.height {
            let fresh = h();
            if *height != fresh {
                *height = fresh;
                changed = true;
            }
        }
        changed
    }

    /// Whether this node has a bound scroll offset evaluator.
    #[inline(always)]
    pub fn has_scroll_binding(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|b| b.scroll_offset_x.is_some() || b.scroll_offset_y.is_some())
    }

    /// Folds 2D scroll offsets into a composed transform.
    ///
    /// Positive offset scrolls content up/left (translation is negated), matching
    /// the "offset = distance from the top/left edge" convention.
    #[inline]
    pub fn compose_with_scroll(base: Transform, offset_x: f32, offset_y: f32) -> Transform {
        if offset_x == 0.0 && offset_y == 0.0 {
            base
        } else {
            base.multiply(&Transform::from_translation(-offset_x, -offset_y))
        }
    }

    /// Re-evaluates the bound 2D scroll offset, returning (0.0, 0.0) when unbound.
    pub fn eval_scroll_offset(&self) -> (f32, f32) {
        self.inner
            .as_deref()
            .map_or((0.0, 0.0), |b| {
                let ox = b.scroll_offset_x.as_ref().map_or(0.0, |f| f());
                let oy = b.scroll_offset_y.as_ref().map_or(0.0, |f| f());
                (ox, oy)
            })
    }

    /// Folds a pose into a declared transform.
    #[inline]
    pub fn compose_with_declared(declared: Transform, pose: Option<Transform>) -> Transform {
        match pose {
            Some(pose) => declared.multiply(&pose),
            None => declared,
        }
    }

    /// Re-evaluates bound local transform, returning true if changed.
    pub fn update_transform(&self, declared: Transform, transform: &mut Transform) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        let declared = match &b.transform {
            Some(t) => t(),
            None => declared,
        };
        let composed =
            Self::compose_with_declared(declared, b.pose.as_ref().map(PoseBindings::compose));
        let ox = b.scroll_offset_x.as_ref().map_or(0.0, |f| f());
        let oy = b.scroll_offset_y.as_ref().map_or(0.0, |f| f());
        let composed = Self::compose_with_scroll(composed, ox, oy);
        if *transform == composed {
            false
        } else {
            *transform = composed;
            true
        }
    }

    /// Re-evaluates the bound paint bundle, returning true if it changed.
    pub fn update_paint(&self, paint: &mut LayerPaint) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        if let Some(e) = &b.paint {
            let fresh = e();
            if *paint != fresh {
                *paint = fresh;
                return true;
            }
        }
        false
    }

    /// Re-evaluates bound text content, returning true if changed.
    pub fn update_text(&self, content: &mut String) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        if let Some(t) = &b.text {
            let fresh = t();
            if *content != fresh {
                *content = fresh;
                return true;
            }
        }
        false
    }

    /// Re-evaluates the bound layout footprint, returning true if changed.
    pub fn update_presence(&self, presence: &mut LayoutPresence) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        if let Some(p) = &b.presence {
            let fresh = p();
            if *presence != fresh {
                *presence = fresh;
                return true;
            }
        }
        false
    }

    /// Re-evaluates a bound stacking order, returning true if changed.
    pub fn update_z(&self, z: &mut i32, is_absolute: bool) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        if let Some(f) = &b.z_index {
            let fresh = resolve_z_index(Some(f()), is_absolute);
            if *z != fresh {
                *z = fresh;
                return true;
            }
        }
        false
    }
}

/// The z-index a node ends up with once absoluteness is taken into account.
pub fn resolve_z_index(declared: Option<i32>, is_absolute: bool) -> i32 {
    match declared {
        Some(z) if z != 0 => z,
        _ if is_absolute => 1,
        _ => 0,
    }
}
