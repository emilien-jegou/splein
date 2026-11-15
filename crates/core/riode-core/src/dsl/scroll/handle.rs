// Single responsibility: Scroll signal state, programmatic scroll handles, and configuration enums.

use std::rc::Rc;
use crate::dsl::element::Element;
use crate::reactive::{use_signal, Signal};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Scrollbar {
    #[default]
    Auto,
    Visible,
    Hidden,
}

#[derive(Clone)]
pub struct ScrollHandle {
    pub offset: Signal<f32>,
    pub max: Signal<f32>,
}

impl ScrollHandle {
    pub fn scroll_to(&self, offset: f32) {
        self.offset.set(offset);
    }

    pub fn scroll_by(&self, delta: f32) {
        self.offset.set(self.offset.get() + delta);
    }

    pub fn scroll_to_index(&self, index: usize, row_size: f32) {
        self.offset.set(index as f32 * row_size.max(1.0));
    }

    pub fn scroll_into_view(&self, rect_pos: f32, rect_size: f32, view_size: f32) {
        let cur = self.offset.get();
        let next = if rect_pos < cur {
            rect_pos
        } else if rect_pos + rect_size > cur + view_size {
            rect_pos + rect_size - view_size
        } else {
            return;
        };
        self.offset.set(next.clamp(0.0, self.max.get().max(0.0)));
    }
}

pub fn use_scroll(initial: f32) -> ScrollHandle {
    ScrollHandle {
        offset: use_signal(initial),
        max: use_signal(0.0),
    }
}

pub struct OnScroll(pub Rc<dyn Fn(f32)>);

impl<F: Fn(f32) + 'static> From<F> for OnScroll {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}

#[derive(Clone)]
pub struct RowFn(pub Rc<dyn Fn(usize) -> Element>);

impl<F: Fn(usize) -> Element + 'static> From<F> for RowFn {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}
