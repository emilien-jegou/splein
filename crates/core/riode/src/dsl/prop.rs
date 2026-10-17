// Single responsibility: Declarative UI conversions from foundation types into dynamic properties.

use crate::foundation::{
    Alignment, BlendMode, Color, Direction, Distribution, Fill, Filter, Gap, Glass,
    GlassStyle, LayoutPresence, Margin, Radius, Shadow, Size, Stroke, Transform,
};
use crate::foundation::glass::ToneCurve;
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

impl IntoProp<Fill> for Fill {
    fn into_prop(self) -> Prop<Fill> {
        Prop::Static(self)
    }
}

impl IntoProp<Fill> for Color {
    fn into_prop(self) -> Prop<Fill> {
        Prop::Static(Fill::solid(self))
    }
}

impl IntoProp<Radius> for Radius {
    fn into_prop(self) -> Prop<Radius> {
        Prop::Static(self)
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

impl IntoProp<Shadow> for Shadow {
    fn into_prop(self) -> Prop<Shadow> {
        Prop::Static(self)
    }
}

/// Raw elevation: `group().shadow(8.0)` builds a default-physics shadow.
impl IntoProp<Shadow> for f32 {
    fn into_prop(self) -> Prop<Shadow> {
        Prop::Static(Shadow::elevation(self))
    }
}

/// Elevation plus tint: `group().shadow((8.0, color))`.
impl IntoProp<Shadow> for (f32, Color) {
    fn into_prop(self) -> Prop<Shadow> {
        Prop::Static(Shadow::elevation(self.0).color(self.1))
    }
}

impl IntoProp<Filter> for Filter {
    fn into_prop(self) -> Prop<Filter> {
        Prop::Static(self)
    }
}

impl IntoProp<Glass> for Glass {
    fn into_prop(self) -> Prop<Glass> {
        Prop::Static(self)
    }
}

/// Raw strength: `group().glass(4.0)` is `Glass::lens(4.0)` — lens only.
impl IntoProp<Glass> for f32 {
    fn into_prop(self) -> Prop<Glass> {
        Prop::Static(Glass::lens(self))
    }
}

impl IntoProp<Glass> for i32 {
    fn into_prop(self) -> Prop<Glass> {
        Prop::Static(Glass::lens(self as f32))
    }
}

impl IntoProp<Option<Color>> for Color {
    fn into_prop(self) -> Prop<Option<Color>> {
        Prop::Static(Some(self))
    }
}

impl IntoProp<Option<Color>> for Option<Color> {
    fn into_prop(self) -> Prop<Option<Color>> {
        Prop::Static(self)
    }
}

impl IntoProp<ToneCurve> for ToneCurve {
    fn into_prop(self) -> Prop<ToneCurve> {
        Prop::Static(self)
    }
}

impl IntoProp<GlassStyle> for GlassStyle {
    fn into_prop(self) -> Prop<GlassStyle> {
        Prop::Static(self)
    }
}

/// Raw blur sigma: `group().filter(4.0)` is `blur(4px)`.
impl IntoProp<Filter> for f32 {
    fn into_prop(self) -> Prop<Filter> {
        Prop::Static(Filter::blur(self))
    }
}

impl IntoProp<Size> for Signal<f32> {
    fn into_prop(self) -> Prop<Size> {
        Prop::Dynamic(std::rc::Rc::new(move || Size::Fixed(self.get())))
    }
}

impl IntoProp<Fill> for Signal<Color> {
    fn into_prop(self) -> Prop<Fill> {
        Prop::Dynamic(std::rc::Rc::new(move || Fill::solid(self.get())))
    }
}

impl IntoProp<Size> for Computed<f32> {
    fn into_prop(self) -> Prop<Size> {
        Prop::Dynamic(std::rc::Rc::new(move || Size::Fixed(self.get())))
    }
}

impl IntoProp<Fill> for Computed<Color> {
    fn into_prop(self) -> Prop<Fill> {
        Prop::Dynamic(std::rc::Rc::new(move || Fill::solid(self.get())))
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
impl_str_prop!(Filter);
impl_str_prop!(Glass);
impl_str_prop!(BlendMode);

impl IntoProp<BlendMode> for BlendMode {
    fn into_prop(self) -> Prop<BlendMode> {
        Prop::Static(self)
    }
}

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
impl_num_prop!(Fill);
impl_num_prop!(Size);
impl_num_prop!(Radius);
impl_num_prop!(Gap);
