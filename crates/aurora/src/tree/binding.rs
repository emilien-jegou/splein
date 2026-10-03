// Single responsibility: Dynamic property evaluation closures on a layout node.

use std::rc::Rc;
use crate::foundation::{Fill, LayoutPresence, Shadow, Size, Transform};

/// Inner storage for dynamic property evaluators.
#[derive(Default, Clone)]
pub struct DynamicBindings {
    pub width: Option<Rc<dyn Fn() -> Size>>,
    pub height: Option<Rc<dyn Fn() -> Size>>,
    pub fill: Option<Rc<dyn Fn() -> Option<Fill>>>,
    pub opacity: Option<Rc<dyn Fn() -> f32>>,
    pub text: Option<Rc<dyn Fn() -> String>>,
    pub transform: Option<Rc<dyn Fn() -> Transform>>,
    pub shadows: Option<Rc<dyn Fn() -> Vec<Shadow>>>,
    pub presence: Option<Rc<dyn Fn() -> LayoutPresence>>,
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
        Self { inner: Some(Rc::new(store)) }
    }

    /// Whether this node has dynamic dimension evaluators bound.
    #[inline(always)]
    pub fn has_dimension_bindings(&self) -> bool {
        self.inner.as_ref().is_some_and(|b| b.has_dimension_bindings())
    }

    /// Re-evaluates bound width and height dimensions, returning true if either changed.
    pub fn update_layout(&self, width: &mut Size, height: &mut Size) -> bool {
        let b = match &self.inner { Some(b) => b, None => return false };
        let mut changed = false;
        if let Some(w) = &b.width {
            let fresh = w();
            if *width != fresh { *width = fresh; changed = true; }
        }
        if let Some(h) = &b.height {
            let fresh = h();
            if *height != fresh { *height = fresh; changed = true; }
        }
        changed
    }

    /// Re-evaluates bound local transform, returning true if changed.
    pub fn update_transform(&self, transform: &mut Transform) -> bool {
        let b = match &self.inner { Some(b) => b, None => return false };
        if let Some(t) = &b.transform {
            let fresh = t();
            if *transform != fresh { *transform = fresh; return true; }
        }
        false
    }

    /// Re-evaluates bound fill, opacity, and shadows, returning true if any changed.
    pub fn update_paint(&self, fill: &mut Option<Fill>, opacity: &mut f32, shadows: &mut Vec<Shadow>) -> bool {
        let b = match &self.inner { Some(b) => b, None => return false };
        let mut changed = false;
        if let Some(f) = &b.fill {
            let fresh = f();
            if *fill != fresh { *fill = fresh; changed = true; }
        }
        if let Some(o) = &b.opacity {
            let fresh = o();
            if *opacity != fresh { *opacity = fresh; changed = true; }
        }
        if let Some(s) = &b.shadows {
            let fresh = s();
            if *shadows != fresh { *shadows = fresh; changed = true; }
        }
        changed
    }

    /// Re-evaluates bound text content, returning true if changed.
    pub fn update_text(&self, content: &mut String) -> bool {
        let b = match &self.inner { Some(b) => b, None => return false };
        if let Some(t) = &b.text {
            let fresh = t();
            if *content != fresh { *content = fresh; return true; }
        }
        false
    }

    /// Re-evaluates the bound layout footprint, returning true if changed.
    pub fn update_presence(&self, presence: &mut LayoutPresence) -> bool {
        let b = match &self.inner { Some(b) => b, None => return false };
        if let Some(p) = &b.presence {
            let fresh = p();
            if *presence != fresh { *presence = fresh; return true; }
        }
        false
    }
}
