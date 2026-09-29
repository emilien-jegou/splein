// Single responsibility: Outer and inset shadow specification.

use crate::foundation::color::Color;

/// Shadow styling kind designating outer projection or inset debossing.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ShadowKind {
    #[default]
    Outer,
    Inset,
}

/// Shadow styling parameters including blur radius, spread, and color.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Shadow {
    pub kind: ShadowKind,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

impl Shadow {
    /// Constructs an outer drop shadow with offset and blur radius.
    pub const fn outer(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self { kind: ShadowKind::Outer, offset_x, offset_y, blur, spread: 0.0, color }
    }

    /// Constructs an inset inner shadow with offset and blur radius.
    pub const fn inset(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self { kind: ShadowKind::Inset, offset_x, offset_y, blur, spread: 0.0, color }
    }
}
