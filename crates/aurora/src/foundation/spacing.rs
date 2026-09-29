// Single responsibility: External margin and inter-element gap spatial models.

/// External spacing offset applied around element bounding boxes.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Margin {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Margin {
    pub const ZERO: Self = Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };
    pub const fn all(val: f32) -> Self { Self { top: val, right: val, bottom: val, left: val } }
    pub const fn symmetric(y: f32, x: f32) -> Self { Self { top: y, right: x, bottom: y, left: x } }
    pub fn horizontal(&self) -> f32 { self.left + self.right }
    pub fn vertical(&self) -> f32 { self.top + self.bottom }
}

/// Spacing interval applied between adjacent layout siblings.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Gap {
    Fixed(f32),
    Full,
}

impl Default for Gap {
    fn default() -> Self { Self::Fixed(0.0) }
}

impl From<f32> for Gap {
    fn from(val: f32) -> Self { Self::Fixed(val) }
}
