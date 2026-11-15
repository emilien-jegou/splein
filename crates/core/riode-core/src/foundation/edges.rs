// Single responsibility: Per-edge values (borders, strokes, and other
// four-sided quantities that are not spacing around a box).

/// A value per physical edge, in top/right/bottom/left order (CSS order).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T> Edges<T> {
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Edges<U> {
        Edges {
            top: f(self.top),
            right: f(self.right),
            bottom: f(self.bottom),
            left: f(self.left),
        }
    }
}

impl<T: Copy> Edges<T> {
    pub fn all(val: T) -> Self {
        Self {
            top: val,
            right: val,
            bottom: val,
            left: val,
        }
    }

    /// CSS shorthand order: `top right_bottom left`.
    pub fn shorthand(top: T, right_bottom: T, left: T) -> Self {
        Self {
            top,
            right: right_bottom,
            bottom: right_bottom,
            left,
        }
    }
}

impl<T: Default> Default for Edges<T> {
    fn default() -> Self {
        Self {
            top: T::default(),
            right: T::default(),
            bottom: T::default(),
            left: T::default(),
        }
    }
}

impl Edges<f32> {
    pub fn max_width(&self) -> f32 {
        self.top.max(self.right).max(self.bottom).max(self.left)
    }

    pub fn is_zero(&self) -> bool {
        self.top <= 0.0 && self.right <= 0.0 && self.bottom <= 0.0 && self.left <= 0.0
    }
}
