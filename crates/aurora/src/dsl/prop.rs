// Single responsibility: Declarative UI conversions from foundation types into dynamic properties.

use crate::foundation::{Color, Fill, LayoutPresence, Radius, Shadow, Size, Transform};
use crate::reactive::derived::Derived;
use crate::reactive::prop::{DynamicProp, IntoProp};
use crate::reactive::signal::Signal;

impl IntoProp<Size> for Size {
    fn into_prop(self) -> DynamicProp<Size> { DynamicProp::Static(self) }
}

impl IntoProp<Size> for f32 {
    fn into_prop(self) -> DynamicProp<Size> { DynamicProp::Static(Size::Fixed(self)) }
}

impl IntoProp<Fill> for Fill {
    fn into_prop(self) -> DynamicProp<Fill> { DynamicProp::Static(self) }
}

impl IntoProp<Fill> for Color {
    fn into_prop(self) -> DynamicProp<Fill> { DynamicProp::Static(Fill::solid(self)) }
}

impl IntoProp<Radius> for Radius {
    fn into_prop(self) -> DynamicProp<Radius> { DynamicProp::Static(self) }
}

impl IntoProp<Transform> for Transform {
    fn into_prop(self) -> DynamicProp<Transform> { DynamicProp::Static(self) }
}

impl IntoProp<LayoutPresence> for LayoutPresence {
    fn into_prop(self) -> DynamicProp<LayoutPresence> { DynamicProp::Static(self) }
}

impl IntoProp<Shadow> for Shadow {
    fn into_prop(self) -> DynamicProp<Shadow> { DynamicProp::Static(self) }
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
