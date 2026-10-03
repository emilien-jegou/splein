// Single responsibility: Fluent declarative Group builder with dynamic properties and helpers.

use crate::dsl::component::IntoElement;
use crate::dsl::element::{Element, IntoChildList};
use crate::foundation::*;
use crate::motion::{Spring, Transition, Tween};
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
    pub transition: Option<Transition>,
    pub layout_transition: Option<Transition>,
    pub presence: Option<DynamicProp<LayoutPresence>>,
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
        transition: None,
        layout_transition: None,
        presence: None,
        children: Vec::new(),
    }
}

impl GroupDef {
    pub fn key<K: Into<Key>>(mut self, k: K) -> Self {
        self.key = Some(k.into());
        self
    }
    pub fn width<P: IntoProp<Size>>(mut self, p: P) -> Self {
        self.width = Some(p.into_prop());
        self
    }
    pub fn height<P: IntoProp<Size>>(mut self, p: P) -> Self {
        self.height = Some(p.into_prop());
        self
    }

    pub fn fill<P: IntoProp<Fill>>(mut self, p: P) -> Self {
        let prop = p.into_prop();
        self.fill = Some(match prop {
            DynamicProp::Static(v) => DynamicProp::Static(Some(v)),
            DynamicProp::Dynamic(f) => DynamicProp::Dynamic(std::rc::Rc::new(move || Some(f()))),
        });
        self
    }
    pub fn opacity<P: IntoProp<f32>>(mut self, p: P) -> Self {
        self.opacity = Some(p.into_prop());
        self
    }
    pub fn transform<P: IntoProp<Transform>>(mut self, t: P) -> Self {
        self.transform = Some(t.into_prop());
        self
    }
    pub fn dynamic_shadow<P: IntoProp<Shadow>>(mut self, s: P) -> Self {
        self.dynamic_shadow = Some(s.into_prop());
        self
    }

    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = m;
        self
    }

    pub fn stroke(mut self, s: Stroke) -> Self {
        self.stroke = Some(s);
        self
    }
    pub fn shadows<I: IntoIterator<Item = Shadow>>(mut self, s: I) -> Self {
        self.shadows.extend(s);
        self
    }
    pub fn radius<R: Into<Radius>>(mut self, r: R) -> Self {
        self.radius = r.into();
        self
    }
    pub fn clip(mut self, c: bool) -> Self {
        self.clip = c;
        self
    }
    pub fn direction(mut self, d: Direction) -> Self {
        self.layout.direction = d;
        self
    }
    pub fn gap<G: Into<Gap>>(mut self, g: G) -> Self {
        self.layout.gap = g.into();
        self
    }
    pub fn alignment(mut self, a: Alignment) -> Self {
        self.layout.alignment = a;
        self
    }
    pub fn distribution(mut self, d: Distribution) -> Self {
        self.layout.distribution = d;
        self
    }

    pub fn absolute(mut self) -> Self {
        self.is_absolute = true;
        self
    }
    pub fn anchor(mut self, a: Anchor) -> Self {
        self.anchor = Some(a);
        self.is_absolute = true;
        self
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }
    pub fn overlay(mut self, on: bool) -> Self {
        self.is_overlay = on;
        self
    }
    /// Interpolates bound property changes over a fixed duration.
    pub fn animate(mut self, tween: Tween) -> Self {
        self.transition = Some(Transition::Timed(tween));
        self
    }
    /// Settles bound property changes with physics instead of a fixed duration.
    pub fn spring(mut self, spring: Spring) -> Self {
        self.transition = Some(Transition::Spring(spring));
        self
    }
    /// Sets the layout footprint this view presents to its parent.
    pub fn layout_presence<P: IntoProp<LayoutPresence>>(mut self, p: P) -> Self {
        self.presence = Some(p.into_prop());
        self
    }
    /// Springs children toward their new layout rects whenever their boxes change.
    pub fn layout_transition(mut self, transition: Transition) -> Self {
        self.layout_transition = Some(transition);
        self
    }
    pub fn children<C: IntoChildList>(mut self, c: C) -> Self {
        self.children = c.into_child_list();
        self
    }
}

impl IntoElement for GroupDef {
    fn into_element(self) -> Element {
        Element::Group(self)
    }
}
