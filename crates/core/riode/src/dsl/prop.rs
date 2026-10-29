// Single responsibility: Declarative UI conversions from foundation types into dynamic properties.

use crate::foundation::{
    Alignment, Color, Direction, Distribution, Gap, LayoutPresence, Margin, Size, Transform,
};
use crate::reactive::computed::Computed;
use crate::reactive::prop::{Prop, IntoProp};
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
