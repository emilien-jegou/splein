// Single responsibility: Declarative UI conversions from foundation types into dynamic properties.

use crate::foundation::{
    Alignment, Color, Direction, Distribution, Gap, LayoutPresence, Margin, Material, Size, Transform,
};
use crate::reactive::computed::Computed;
pub use crate::reactive::prop::{IntoProp, Prop};
use crate::reactive::signal::Signal;

impl IntoProp<Size> for Size {
    fn into_prop(self) -> Prop<Size> {
        Prop::Static(self)
    }
}

impl IntoProp<Size> for f32 {
    fn into_prop(self) -> Prop<Size> {
        Prop::Static(Size::Fixed(self))
    }
}

impl IntoProp<Transform> for Transform {
    fn into_prop(self) -> Prop<Transform> {
        Prop::Static(self)
    }
}

impl IntoProp<LayoutPresence> for LayoutPresence {
    fn into_prop(self) -> Prop<LayoutPresence> {
        Prop::Static(self)
    }
}

impl IntoProp<Size> for Signal<f32> {
    fn into_prop(self) -> Prop<Size> {
        Prop::Dynamic(std::rc::Rc::new(move || Size::Fixed(self.get())))
    }
}

impl IntoProp<Size> for Computed<f32> {
    fn into_prop(self) -> Prop<Size> {
        Prop::Dynamic(std::rc::Rc::new(move || Size::Fixed(self.get())))
    }
}

impl IntoProp<Material> for Material {
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(self)
    }
}

/// Literal inference: every paint/layout value accepts strings and numbers.
///
/// `width="fill"`, `margin="8 16"`, `direction="vertical"`. Invalid literals
/// panic naming the value and the accepted grammar — a typo should fail loud,
/// not render transparent.
macro_rules! impl_str_prop {
    ($ty:ty) => {
        impl IntoProp<$ty> for &'static str {
            fn into_prop(self) -> Prop<$ty> {
                Prop::Static(<$ty>::from(self))
            }
        }
        impl IntoProp<$ty> for String {
            fn into_prop(self) -> Prop<$ty> {
                Prop::Static(<$ty>::from(self))
            }
        }
    };
}

impl_str_prop!(Color);
impl_str_prop!(Size);
impl_str_prop!(Gap);
impl_str_prop!(Margin);
impl_str_prop!(Direction);
impl_str_prop!(Alignment);
impl_str_prop!(Distribution);

macro_rules! impl_num_prop {
    ($ty:ty) => {
        impl IntoProp<$ty> for u32 {
            fn into_prop(self) -> Prop<$ty> {
                Prop::Static(<$ty>::from(self))
            }
        }
        impl IntoProp<$ty> for i32 {
            fn into_prop(self) -> Prop<$ty> {
                Prop::Static(<$ty>::from(self))
            }
        }
    };
}

impl_num_prop!(Color);
impl_num_prop!(Size);
impl_num_prop!(Gap);

/// Numeric literal acceptance for pixel-valued shorthand setters
/// (`<g p=12>`, `<text s=14>`): bare integer literals auto-cast to f32 px.
pub trait Px {
    fn px(self) -> f32;
}

macro_rules! impl_px {
    ($($t:ty),*) => {
        $(impl Px for $t {
            fn px(self) -> f32 {
                self as f32
            }
        })*
    };
}

impl_px!(f32, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// Numeric literal acceptance for count-valued shorthand setters
/// (`<text max_lines=1 w=600>`): bare integer literals cast to the count type.
pub trait Count {
    fn count(self) -> u32;
}

macro_rules! impl_count {
    ($($t:ty),*) => {
        $(impl Count for $t {
            fn count(self) -> u32 {
                self as u32
            }
        })*
    };
}

impl_count!(f32, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
