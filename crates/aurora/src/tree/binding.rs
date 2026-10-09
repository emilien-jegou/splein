// Single responsibility: Dynamic property evaluation closures on a layout node.

use crate::foundation::{Fill, LayoutPresence, Shadow, Size, Transform};
use std::rc::Rc;

/// Inner storage for dynamic property evaluators.
#[derive(Default, Clone)]
pub struct DynamicBindings {
    pub width: Option<Rc<dyn Fn() -> Size>>,
    pub height: Option<Rc<dyn Fn() -> Size>>,
    pub fill: Option<Rc<dyn Fn() -> Option<Fill>>>,
    pub opacity: Option<Rc<dyn Fn() -> f32>>,
    pub text: Option<Rc<dyn Fn() -> String>>,
    pub transform: Option<Rc<dyn Fn() -> Transform>>,
    pub shadow: Option<Rc<dyn Fn() -> Option<Shadow>>>,
    pub presence: Option<Rc<dyn Fn() -> LayoutPresence>>,
    /// Stacking order evaluator, when the node declares a dynamic one.
    pub z_index: Option<Rc<dyn Fn() -> i32>>,
    pub pose: Option<PoseBindings>,
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
    ///
    /// Narrower than [`Self::has_pose_bindings`] on purpose: scale and rotation compose with a
    /// springing box without conflict, but a bound translation commands the same axis the box
    /// moves on and so does.
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

    /// Folds a pose into a declared transform.
    ///
    /// The one place the two are combined. Both writers of a node's transform go through
    /// here -- reconciliation at mount, and the reactive update below -- because when they
    /// composed independently they drifted, and the reactive path overwrote the declared
    /// transform instead of folding the pose into it. Pose components are applied on top of
    /// the declared transform, matching the documented "translate, rotate, scale" order.
    #[inline]
    pub fn compose_with_declared(declared: Transform, pose: Option<Transform>) -> Transform {
        match pose {
            Some(pose) => declared.multiply(&pose),
            None => declared,
        }
    }

    /// Re-evaluates bound local transform, returning true if changed.
    ///
    /// Recomposes from the node's declared transform rather than overwriting the stored one,
    /// so a node declared with both `.transform(t)` and bound pose components keeps `t`.
    pub fn update_transform(&self, declared: Transform, transform: &mut Transform) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        // A bound `.transform(..)` is itself a declared transform, so it replaces the static
        // one before the pose is folded in.
        let declared = match &b.transform {
            Some(t) => t(),
            None => declared,
        };
        // `compose()` is identity when nothing is bound, and multiplying by identity is
        // exact in this representation, so the unbound case needs no branch here.
        let composed =
            Self::compose_with_declared(declared, b.pose.as_ref().map(PoseBindings::compose));
        if *transform == composed {
            false
        } else {
            *transform = composed;
            true
        }
    }

    /// Re-evaluates bound fill, opacity, and shadow, returning true if any changed.
    pub fn update_paint(
        &self,
        fill: &mut Option<Fill>,
        opacity: &mut f32,
        shadow: &mut Option<Shadow>,
    ) -> bool {
        let b = match &self.inner {
            Some(b) => b,
            None => return false,
        };
        let mut changed = false;
        if let Some(f) = &b.fill {
            let fresh = f();
            if *fill != fresh {
                *fill = fresh;
                changed = true;
            }
        }
        if let Some(o) = &b.opacity {
            let fresh = o();
            if *opacity != fresh {
                *opacity = fresh;
                changed = true;
            }
        }
        if let Some(s) = &b.shadow {
            let fresh = s();
            if *shadow != fresh {
                *shadow = fresh;
                changed = true;
            }
        }
        changed
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
    ///
    /// Paint-class like opacity: a new z reorders paint (and hit testing, which mirrors
    /// paint) without touching layout. The absolute-implicit-one rule from reconciliation
    /// applies here too, so an explicit zero on an absolute node still stacks above in-flow
    /// siblings exactly as it would had it been declared statically.
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
///
/// An absolute node with no declared z still has to stack above in-flow siblings, which is what
/// the implicit `1` is for. One function so reconciliation and the reactive update agree.
pub fn resolve_z_index(declared: Option<i32>, is_absolute: bool) -> i32 {
    match declared {
        Some(z) if z != 0 => z,
        _ if is_absolute => 1,
        _ => 0,
    }
}
