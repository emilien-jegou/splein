// Single responsibility: Fluent declarative Group builder with dynamic properties and helpers.

use crate::dsl::component::IntoElement;
use crate::dsl::element::{Element, IntoChildList};
use crate::foundation::*;
use crate::reactive::prop::{DynamicProp, IntoProp};

/// Declarative flex container builder with reactive properties and styling modifiers.
pub struct GroupDef {
    pub key: Option<Key>,
    pub width: Option<DynamicProp<Size>>,
    pub height: Option<DynamicProp<Size>>,
    pub fill: Option<DynamicProp<Option<Fill>>>,
    pub opacity: Option<DynamicProp<f32>>,
    pub transform: Option<DynamicProp<Transform>>,
    pub dynamic_shadow: Option<DynamicProp<Shadow>>,
    pub layout: LayoutConfig,
    pub margin: Margin,
    pub stroke: Option<Stroke>,
    pub shadows: Vec<Shadow>,
    pub radius: Radius,
    pub clip: bool,
    pub is_absolute: bool,
    pub anchor: Option<Anchor>,
    pub z_index: i32,
    pub is_overlay: bool,
    pub has_layer: bool,
    pub children: Vec<Element>,
}

pub fn group() -> GroupDef {
    GroupDef {
        key: None,
        width: None,
        height: None,
        fill: None,
        opacity: None,
        transform: None,
        dynamic_shadow: None,
        layout: LayoutConfig::default(),
        margin: Margin::ZERO,
        stroke: None,
        shadows: Vec::new(),
        radius: Radius::ZERO,
        clip: false,
        is_absolute: false,
        anchor: None,
        z_index: 0,
        is_overlay: false,
        has_layer: false,
        children: Vec::new(),
    }
}

impl GroupDef {
    pub fn key<K: Into<Key>>(mut self, k: K) -> Self { self.key = Some(k.into()); self }
    pub fn width<P: IntoProp<Size>>(mut self, p: P) -> Self { self.width = Some(p.into_prop()); self }
    pub fn height<P: IntoProp<Size>>(mut self, p: P) -> Self { self.height = Some(p.into_prop()); self }
    pub fn size<W: IntoProp<Size>, H: IntoProp<Size>>(self, w: W, h: H) -> Self { self.width(w).height(h) }
    pub fn fill_parent(self) -> Self { self.width(Size::fill()).height(Size::fill()) }
    pub fn fit_content(self) -> Self { self.width(Size::fit()).height(Size::fit()) }
    pub fn fit_width(self) -> Self { self.width(Size::fit()) }
    pub fn fit_height(self) -> Self { self.height(Size::fit()) }
    pub fn fill_width(self) -> Self { self.width(Size::fill()) }
    pub fn fill_height(self) -> Self { self.height(Size::fill()) }

    pub fn fill<P: IntoProp<Fill>>(mut self, p: P) -> Self {
        let prop = p.into_prop();
        self.fill = Some(match prop {
            DynamicProp::Static(v) => DynamicProp::Static(Some(v)),
            DynamicProp::Dynamic(f) => DynamicProp::Dynamic(std::rc::Rc::new(move || Some(f()))),
        });
        self
    }
    pub fn opacity<P: IntoProp<f32>>(mut self, p: P) -> Self { self.opacity = Some(p.into_prop()); self }
    pub fn transform<P: IntoProp<Transform>>(mut self, t: P) -> Self { self.transform = Some(t.into_prop()); self }
    pub fn dynamic_shadow<P: IntoProp<Shadow>>(mut self, s: P) -> Self { self.dynamic_shadow = Some(s.into_prop()); self }

    pub fn margin(mut self, m: Margin) -> Self { self.margin = m; self }
    pub fn margin_x(mut self, x: f32) -> Self { self.margin.left = x; self.margin.right = x; self }
    pub fn margin_y(mut self, y: f32) -> Self { self.margin.top = y; self.margin.bottom = y; self }
    pub fn margin_xy(self, x: f32, y: f32) -> Self { self.margin_x(x).margin_y(y) }
    pub fn margin_left(mut self, l: f32) -> Self { self.margin.left = l; self }
    pub fn margin_top(mut self, t: f32) -> Self { self.margin.top = t; self }
    pub fn margin_right(mut self, r: f32) -> Self { self.margin.right = r; self }
    pub fn margin_bottom(mut self, b: f32) -> Self { self.margin.bottom = b; self }

    pub fn stroke(mut self, s: Stroke) -> Self { self.stroke = Some(s); self }
    pub fn shadow(mut self, s: Shadow) -> Self { self.shadows.push(s); self }
    pub fn shadows<I: IntoIterator<Item = Shadow>>(mut self, s: I) -> Self { self.shadows.extend(s); self }
    pub fn radius<R: Into<Radius>>(mut self, r: R) -> Self { self.radius = r.into(); self }
    pub fn clip(mut self, c: bool) -> Self { self.clip = c; self }
    pub fn rotation(mut self, deg: f32) -> Self { self.transform = Some(DynamicProp::Static(Transform::from_rotation_degrees(deg))); self }
    pub fn direction(mut self, d: Direction) -> Self { self.layout.direction = d; self }
    pub fn gap<G: Into<Gap>>(mut self, g: G) -> Self { self.layout.gap = g.into(); self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.layout.alignment = a; self }
    pub fn align_center(self) -> Self { self.alignment(Alignment::Center) }
    pub fn distribution(mut self, d: Distribution) -> Self { self.layout.distribution = d; self }
    pub fn justify_center(self) -> Self { self.distribution(Distribution::Center) }
    pub fn justify_between(self) -> Self { self.distribution(Distribution::SpaceBetween) }
    pub fn center(self) -> Self { self.align_center().justify_center() }

    pub fn absolute(mut self) -> Self { self.is_absolute = true; self }
    pub fn anchor(mut self, a: Anchor) -> Self { self.anchor = Some(a); self.is_absolute = true; self }
    pub fn z_index(mut self, z: i32) -> Self { self.z_index = z; self }
    pub fn overlay(mut self) -> Self { self.is_overlay = true; self.z_index = 100; self }
    pub fn children<C: IntoChildList>(mut self, c: C) -> Self { self.children = c.into_child_list(); self }
}

impl IntoElement for GroupDef {
    fn into_element(self) -> Element { Element::Group(self) }
}
