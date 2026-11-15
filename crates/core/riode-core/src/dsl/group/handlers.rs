// Single responsibility: Pointer, drag, key, and scroll gesture event dispatch wiring.

use super::GroupElement;
use crate::input::{
    ClickInfo, DragInfo, HoverInfo, KeyEvent, PointerEvent, Propagation, WheelInfo,
};
use std::rc::Rc;

pub trait Behavior {
    fn attach(self, row: GroupElement) -> GroupElement;
}

impl GroupElement {
    pub fn on_click(mut self, f: impl Fn(&ClickInfo) + 'static) -> Self {
        self.handlers.click = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_click_handled(mut self, f: impl Fn(&ClickInfo) -> Propagation + 'static) -> Self {
        self.handlers.click = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_down(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.handlers.pointer_down = Some(Rc::new(move |e| {
            f(e);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_pointer_down_handled(
        mut self,
        f: impl Fn(&PointerEvent) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_down = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_up(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.handlers.pointer_up = Some(Rc::new(move |e| {
            f(e);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_pointer_up_handled(
        mut self,
        f: impl Fn(&PointerEvent) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_up = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_move(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.handlers.pointer_move = Some(Rc::new(move |e| {
            f(e);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_pointer_move_handled(
        mut self,
        f: impl Fn(&PointerEvent) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_move = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_enter(mut self, f: impl Fn(&HoverInfo) + 'static) -> Self {
        self.handlers.pointer_enter = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_pointer_enter_handled(
        mut self,
        f: impl Fn(&HoverInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_enter = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_leave(mut self, f: impl Fn(&HoverInfo) + 'static) -> Self {
        self.handlers.pointer_leave = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_pointer_leave_handled(
        mut self,
        f: impl Fn(&HoverInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_leave = Some(Rc::new(f));
        self
    }
    pub fn on_drag_start(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.handlers.drag_start = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_drag_start_handled(mut self, f: impl Fn(&DragInfo) -> Propagation + 'static) -> Self {
        self.handlers.drag_start = Some(Rc::new(f));
        self
    }
    pub fn on_drag_step(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.handlers.drag_step = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_drag_step_handled(mut self, f: impl Fn(&DragInfo) -> Propagation + 'static) -> Self {
        self.handlers.drag_step = Some(Rc::new(f));
        self
    }
    pub fn on_drag_end(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.handlers.drag_end = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_drag_end_handled(mut self, f: impl Fn(&DragInfo) -> Propagation + 'static) -> Self {
        self.handlers.drag_end = Some(Rc::new(f));
        self
    }
    pub fn on_wheel(mut self, f: impl Fn(&WheelInfo) + 'static) -> Self {
        self.handlers.wheel = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_wheel_handled(mut self, f: impl Fn(&WheelInfo) -> Propagation + 'static) -> Self {
        self.handlers.wheel = Some(Rc::new(f));
        self
    }
    pub fn on_key(mut self, f: impl Fn(&KeyEvent) + 'static) -> Self {
        self.handlers.key = Some(Rc::new(move |event| {
            f(event);
            Propagation::Bubble
        }));
        self
    }
    pub fn on_key_handled(mut self, f: impl Fn(&KeyEvent) -> Propagation + 'static) -> Self {
        self.handlers.key = Some(Rc::new(f));
        self
    }
    pub fn attach<B: Behavior>(self, behavior: B) -> Self {
        behavior.attach(self)
    }
    pub fn on_drag(self, drag: crate::dsl::gesture::Drag) -> Self {
        self.attach(drag)
    }
}
