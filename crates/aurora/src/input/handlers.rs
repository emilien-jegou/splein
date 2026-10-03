// Single responsibility: Per-node event handler storage shared across reconciliation.

use std::rc::Rc;

use crate::input::event::{ClickInfo, DragInfo, HoverInfo, PointerEvent};

/// Closures a view declares for pointer interaction.
///
/// Handlers are stored behind one `Rc` per node, so reconciliation swaps them in O(1) and a
/// node with no handlers costs a null check. Nothing here invalidates layout: a handler that
/// wants to change the tree writes to a signal, and ordinary reactive propagation decides
/// what that dirties.
#[derive(Default)]
pub struct HandlerSet {
    pub click: Option<Rc<dyn Fn(&ClickInfo)>>,
    pub pointer_down: Option<Rc<dyn Fn(&PointerEvent)>>,
    pub pointer_up: Option<Rc<dyn Fn(&PointerEvent)>>,
    pub pointer_move: Option<Rc<dyn Fn(&PointerEvent)>>,
    pub pointer_enter: Option<Rc<dyn Fn(&HoverInfo)>>,
    pub pointer_leave: Option<Rc<dyn Fn(&HoverInfo)>>,
    /// Fired once a press has moved far enough to count as a drag rather than a click.
    pub drag_start: Option<Rc<dyn Fn(&DragInfo)>>,
    /// Fired for every pointer sample while a drag is active.
    pub drag_step: Option<Rc<dyn Fn(&DragInfo)>>,
    /// Fired on release, carrying the release velocity a gesture hands to a spring.
    pub drag_end: Option<Rc<dyn Fn(&DragInfo)>>,
}

impl HandlerSet {
    /// Whether this node can respond to anything at all.
    ///
    /// Hit testing stops collecting a chain once it reaches a node with no handlers, which
    /// keeps pointer cost proportional to interactive subtrees rather than tree size.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.click.is_none()
            && self.pointer_down.is_none()
            && self.pointer_up.is_none()
            && self.pointer_move.is_none()
            && self.pointer_enter.is_none()
            && self.pointer_leave.is_none()
            && self.drag_start.is_none()
            && self.drag_step.is_none()
            && self.drag_end.is_none()
    }

    /// Whether this node declared drag handlers, which is what makes it a drag target.
    #[inline]
    pub fn handles_drag(&self) -> bool {
        self.drag_start.is_some() || self.drag_step.is_some() || self.drag_end.is_some()
    }
}

/// A node's handlers, cloneable in constant time.
#[derive(Default, Clone)]
pub struct NodeHandlers {
    inner: Option<Rc<HandlerSet>>,
}

impl NodeHandlers {
    /// Adopts a handler set, or clears them when none were declared.
    pub fn from_store(store: HandlerSet) -> Self {
        if store.is_empty() {
            Self { inner: None }
        } else {
            Self {
                inner: Some(Rc::new(store)),
            }
        }
    }

    /// Reads the handler set, if this node has one.
    #[inline(always)]
    pub fn get(&self) -> Option<&HandlerSet> {
        self.inner.as_deref()
    }

    /// Whether this node can respond to any pointer event.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_none()
    }

    /// Whether this node declared drag handlers.
    #[inline(always)]
    pub fn handles_drag(&self) -> bool {
        self.inner.as_deref().is_some_and(HandlerSet::handles_drag)
    }
}

impl HandlerSet {
    /// Invokes the drag-start handler, if present.
    #[inline]
    pub fn fire_drag_start(&self, info: &DragInfo) {
        if let Some(h) = self.drag_start.as_ref() {
            h(info);
        }
    }

    /// Invokes the drag-step handler, if present.
    #[inline]
    pub fn fire_drag_step(&self, info: &DragInfo) {
        if let Some(h) = self.drag_step.as_ref() {
            h(info);
        }
    }

    /// Invokes the drag-end handler, if present.
    #[inline]
    pub fn fire_drag_end(&self, info: &DragInfo) {
        if let Some(h) = self.drag_end.as_ref() {
            h(info);
        }
    }

    /// Invokes the click handler, if present.
    #[inline]
    pub fn fire_click(&self, info: &ClickInfo) {
        if let Some(h) = self.click.as_ref() {
            h(info);
        }
    }

    /// Invokes the pointer-down handler, if present.
    #[inline]
    pub fn fire_pointer_down(&self, event: &PointerEvent) {
        if let Some(h) = self.pointer_down.as_ref() {
            h(event);
        }
    }

    /// Invokes the pointer-up handler, if present.
    #[inline]
    pub fn fire_pointer_up(&self, event: &PointerEvent) {
        if let Some(h) = self.pointer_up.as_ref() {
            h(event);
        }
    }

    /// Invokes the pointer-move handler, if present.
    #[inline]
    pub fn fire_pointer_move(&self, event: &PointerEvent) {
        if let Some(h) = self.pointer_move.as_ref() {
            h(event);
        }
    }

    /// Invokes the pointer-enter handler, if present.
    #[inline]
    pub fn fire_pointer_enter(&self, info: &HoverInfo) {
        if let Some(h) = self.pointer_enter.as_ref() {
            h(info);
        }
    }

    /// Invokes the pointer-leave handler, if present.
    #[inline]
    pub fn fire_pointer_leave(&self, info: &HoverInfo) {
        if let Some(h) = self.pointer_leave.as_ref() {
            h(info);
        }
    }
}
