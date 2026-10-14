// Single responsibility: Declarative UI conversions from foundation types into dynamic properties.

use crate::foundation::{
    Alignment, Color, Direction, Distribution, Fill, Gap, LayoutPresence, Margin, Radius, Shadow,
    Size, Stroke, Transform,
};
use crate::reactive::derived::Derived;
use crate::reactive::prop::{DynamicProp, IntoProp};
use crate::reactive::signal::Signal;

impl IntoProp<Size> for Size {
    fn into_prop(self) -> DynamicProp<Size> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<Size> for f32 {
    fn into_prop(self) -> DynamicProp<Size> {
        DynamicProp::Static(Size::Fixed(self))
    }
}

impl IntoProp<Fill> for Fill {
    fn into_prop(self) -> DynamicProp<Fill> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<Fill> for Color {
    fn into_prop(self) -> DynamicProp<Fill> {
        DynamicProp::Static(Fill::solid(self))
    }
}

impl IntoProp<Radius> for Radius {
    fn into_prop(self) -> DynamicProp<Radius> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<Transform> for Transform {
    fn into_prop(self) -> DynamicProp<Transform> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<LayoutPresence> for LayoutPresence {
    fn into_prop(self) -> DynamicProp<LayoutPresence> {
        DynamicProp::Static(self)
    }
}

impl IntoProp<Shadow> for Shadow {
    fn into_prop(self) -> DynamicProp<Shadow> {
        DynamicProp::Static(self)
    }
}

/// Raw elevation: `group().shadow(8.0)` builds a default-physics shadow.
impl IntoProp<Shadow> for f32 {
    fn into_prop(self) -> DynamicProp<Shadow> {
        DynamicProp::Static(Shadow::elevation(self))
    }
}

/// Elevation plus tint: `group().shadow((8.0, color))`.
impl IntoProp<Shadow> for (f32, Color) {
    fn into_prop(self) -> DynamicProp<Shadow> {
        DynamicProp::Static(Shadow::elevation(self.0).color(self.1))
    }
}

impl IntoProp<Size> for Signal<f32> {
    fn into_prop(self) -> DynamicProp<Size> {
        DynamicProp::Dynamic(std::rc::Rc::new(move || Size::Fixed(self.get())))
    }
}

impl IntoProp<Fill> for Signal<Color> {
    fn into_prop(self) -> DynamicProp<Fill> {
        DynamicProp::Dynamic(std::rc::Rc::new(move || Fill::solid(self.get())))
    }
}

impl IntoProp<Size> for Derived<f32> {
    fn into_prop(self) -> DynamicProp<Size> {
        DynamicProp::Dynamic(std::rc::Rc::new(move || Size::Fixed(self.get())))
    }
}

impl IntoProp<Fill> for Derived<Color> {
    fn into_prop(self) -> DynamicProp<Fill> {
        DynamicProp::Dynamic(std::rc::Rc::new(move || Fill::solid(self.get())))
    }
}

/// Literal inference: every paint/layout value accepts strings and numbers.
///
/// `fill="#121212"`, `fill="rgb(18,18,18)"`, `fill={0x121212}`, `width="fill"`,
/// `radius="max"`, `margin="8 16"`, `direction="vertical"`,
/// `shadow="0 4 16 #0008"`. Invalid literals panic naming the value and the
/// accepted grammar — a typo should fail loud, not render transparent.
macro_rules! impl_str_prop {
    ($ty:ty) => {
        impl IntoProp<$ty> for &'static str {
            fn into_prop(self) -> DynamicProp<$ty> {
                DynamicProp::Static(<$ty>::from(self))
            }
        }
        impl IntoProp<$ty> for String {
            fn into_prop(self) -> DynamicProp<$ty> {
                DynamicProp::Static(<$ty>::from(self))
            }
        }
    };
}

impl_str_prop!(Color);
impl_str_prop!(Fill);
impl_str_prop!(Size);
impl_str_prop!(Radius);
impl_str_prop!(Gap);
impl_str_prop!(Margin);
impl_str_prop!(Direction);
impl_str_prop!(Alignment);
impl_str_prop!(Distribution);
impl_str_prop!(Stroke);
impl_str_prop!(Shadow);

macro_rules! impl_num_prop {
    ($ty:ty) => {
        impl IntoProp<$ty> for u32 {
            fn into_prop(self) -> DynamicProp<$ty> {
                DynamicProp::Static(<$ty>::from(self))
            }
        }
        impl IntoProp<$ty> for i32 {
            fn into_prop(self) -> DynamicProp<$ty> {
                DynamicProp::Static(<$ty>::from(self))
            }
        }
    };
}

impl_num_prop!(Color);
impl_num_prop!(Fill);
impl_num_prop!(Size);
impl_num_prop!(Radius);
impl_num_prop!(Gap);
