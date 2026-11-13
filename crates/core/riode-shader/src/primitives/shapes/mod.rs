pub mod rounded_rect;
pub mod squircle;

pub use rounded_rect::RoundedRect;
pub use squircle::Squircle;

// Ergonomic builder aliases: Shape::rounded(r), Shape::squircle(r), Shape::rect(), etc.
pub struct Shape;

impl Shape {
    pub fn rect() -> riode::foundation::ShapeContainer {
        riode::foundation::ShapeContainer::new(rounded_rect::RoundedRect::rect())
    }

    pub fn rounded(r: f32) -> riode::foundation::ShapeContainer {
        riode::foundation::ShapeContainer::new(rounded_rect::RoundedRect::rounded(r))
    }

    pub fn corners(tl: f32, tr: f32, br: f32, bl: f32) -> riode::foundation::ShapeContainer {
        riode::foundation::ShapeContainer::new(rounded_rect::RoundedRect::corners(tl, tr, br, bl))
    }

    /// Standard iOS-style squircle (superellipse corner with exponent $n = 4.0$).
    pub fn squircle(r: f32) -> riode::foundation::ShapeContainer {
        riode::foundation::ShapeContainer::new(squircle::Squircle::new(r))
    }

    /// Squircle with parametric exponent $n$ (2.0 = circle, 4.0 = classic squircle, 5.0+ = flatter sides).
    pub fn superellipse(r: f32, exponent: f32) -> riode::foundation::ShapeContainer {
        riode::foundation::ShapeContainer::new(squircle::Squircle::with_exponent(r, exponent))
    }
}
