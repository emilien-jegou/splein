// Single responsibility: IntoProp trait implementations for Material, Shape, Overlay, and Effect primitives.

use crate::foundation::*;
use crate::reactive::prop::{IntoProp, Prop};

impl<T: Stage, const N: usize> IntoProp<Material> for [T; N] {
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(Material::from(self))
    }
}
impl<T: Stage> IntoProp<Material> for Vec<T> {
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(Material::from(self))
    }
}
impl IntoProp<Material> for Vec<Box<dyn Stage>> {
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(Material::from_boxes(self))
    }
}
impl<F, S> IntoProp<Material> for GeoFn<F>
where
    F: Fn(&Geo) -> S + 'static,
    S: Stage,
{
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(Material::from(self))
    }
}
impl IntoProp<Shape> for Shape {
    fn into_prop(self) -> Prop<Shape> {
        Prop::Static(self)
    }
}
impl<S: Stage, const N: usize> IntoProp<Shape> for [S; N] {
    fn into_prop(self) -> Prop<Shape> {
        Prop::Static(Shape::from(self))
    }
}
impl IntoProp<Shape> for Vec<Box<dyn Stage>> {
    fn into_prop(self) -> Prop<Shape> {
        Prop::Static(Shape::from_boxes(self))
    }
}
impl IntoProp<Overlay> for Overlay {
    fn into_prop(self) -> Prop<Overlay> {
        Prop::Static(self)
    }
}
impl<T: Stage, const N: usize> IntoProp<Overlay> for [T; N] {
    fn into_prop(self) -> Prop<Overlay> {
        Prop::Static(Overlay::from(self))
    }
}
impl IntoProp<Overlay> for Vec<Box<dyn Stage>> {
    fn into_prop(self) -> Prop<Overlay> {
        Prop::Static(Overlay::from_boxes(self))
    }
}
impl IntoProp<Effect> for Effect {
    fn into_prop(self) -> Prop<Effect> {
        Prop::Static(self)
    }
}
impl<T: EffectStage, const N: usize> IntoProp<Effect> for [T; N] {
    fn into_prop(self) -> Prop<Effect> {
        Prop::Static(Effect::from(self))
    }
}
impl IntoProp<Effect> for Vec<Box<dyn EffectStage>> {
    fn into_prop(self) -> Prop<Effect> {
        Prop::Static(Effect::from_boxes(self))
    }
}
impl IntoProp<Effect> for BlendMode {
    fn into_prop(self) -> Prop<Effect> {
        Prop::Static(Effect::from(self))
    }
}
impl<S: Stage> IntoProp<Material> for BlendedStage<S> {
    fn into_prop(self) -> Prop<Material> {
        Prop::Static(Material::collect([self]))
    }
}
