// Architectural goal: Declarative container primitive, reactive layout configuration, and event binding.

// In crates/core/riode/src/dsl/group/mod.rs

pub mod children;
pub mod handlers;
pub mod into_prop_impls;
pub mod shorthands;

pub use children::{flatten_segments, index_keyed_list, keyed_list, ChildSegment};
pub use handlers::Behavior;

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::dsl::element_ref::ElementRef;
use crate::foundation::*;
use crate::input::{HandlerSet, HitTest};
use crate::reactive::prop::{IntoProp, Prop};

/// Declarative flex container builder with reactive properties and styling modifiers.
#[derive(Clone)]
pub struct GroupElement {
    pub key: Option<Key>,
    pub width: Option<Prop<Size>>,
    pub height: Option<Prop<Size>>,
    pub transform: Option<Prop<Transform>>,
    pub x: Option<Prop<f32>>,
    pub y: Option<Prop<f32>>,
    pub scale: Option<Prop<f32>>,
    pub rotate_degrees: Option<Prop<f32>>,
    pub shape: Option<Prop<ShapeContainer>>,
    pub material: Option<Prop<Material>>,
    pub overlay: Option<Prop<Overlay>>,
    pub effect: Option<Prop<Effect>>,
    pub layout: LayoutConfig,
    pub margin: Margin,
    pub padding: Padding,
    pub align_h: Option<PhysAlign>,
    pub align_v: Option<PhysAlign>,
    pub gap_x: Option<f32>,
    pub gap_y: Option<f32>,
    pub scroll: Option<crate::dsl::scroll::ScrollAxis>,
    pub scroll_bounce: Option<bool>,
    pub clip: bool,
    pub is_absolute: bool,
    pub scrollable: bool,
    pub overscroll: f32,
    pub isolate_children: bool,
    pub scroll_offset: Option<Prop<f32>>,
    pub scroll_offset_x: Option<Prop<f32>>,
    pub scroll_offset_y: Option<Prop<f32>>,
    pub anchor: Option<Anchor>,
    pub aspect_ratio: Option<f32>,
    pub opacity: Option<Prop<f32>>,
    pub z_index: Option<Prop<i32>>,
    pub is_overlay: bool,
    pub has_layer: bool,
    pub presence: Option<Prop<LayoutPresence>>,
    pub element_ref: Option<ElementRef>,
    pub handlers: HandlerSet,
    pub hit_test: HitTest,
    pub provided: Vec<(
        crate::reactive::context::CtxId,
        crate::reactive::context::ProviderValue,
    )>,
    pub child_segments: Vec<ChildSegment>,
}

pub fn group() -> GroupElement {
    GroupElement {
        key: None,
        width: None,
        height: None,
        transform: None,
        x: None,
        y: None,
        scale: None,
        rotate_degrees: None,
        material: None,
        overlay: None,
        effect: None,
        shape: None,
        layout: LayoutConfig::default(),
        margin: Margin::ZERO,
        padding: Padding::ZERO,
        align_h: None,
        align_v: None,
        gap_x: None,
        gap_y: None,
        scroll: None,
        scroll_bounce: None,
        clip: false,
        is_absolute: false,
        scrollable: false,
        overscroll: 0.0,
        isolate_children: false,
        scroll_offset: None,
        scroll_offset_x: None,
        scroll_offset_y: None,
        anchor: None,
        aspect_ratio: None,
        z_index: None,
        opacity: None,
        is_overlay: false,
        has_layer: false,
        presence: None,
        element_ref: None,
        handlers: HandlerSet::default(),
        hit_test: HitTest::Auto,
        provided: Vec::new(),
        child_segments: Vec::new(),
    }
}

impl GroupElement {
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
    pub fn transform<P: IntoProp<Transform>>(mut self, t: P) -> Self {
        self.transform = Some(t.into_prop());
        self
    }
    pub fn x<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.x = Some(v.into_prop());
        self
    }
    pub fn y<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.y = Some(v.into_prop());
        self
    }
    pub fn scale<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.scale = Some(v.into_prop());
        self
    }
    pub fn opacity<P: IntoProp<f32>>(mut self, o: P) -> Self {
        self.opacity = Some(o.into_prop());
        self.has_layer = true;
        self
    }
    pub fn rotate_degrees<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.rotate_degrees = Some(v.into_prop());
        self
    }
    pub fn material<P: IntoProp<Material>>(mut self, e: P) -> Self {
        self.material = Some(e.into_prop());
        self
    }
    pub fn material_opt(mut self, e: Option<Material>) -> Self {
        if let Some(m) = e {
            self.material = Some(m.into_prop());
        }
        self
    }
    pub fn shape<P: IntoProp<ShapeContainer>>(mut self, s: P) -> Self {
        self.shape = Some(s.into_prop());
        self
    }
    pub fn overlay<P: IntoProp<Overlay>>(mut self, o: P) -> Self {
        self.overlay = Some(o.into_prop());
        self
    }
    pub fn effect<P: IntoProp<Effect>>(mut self, e: P) -> Self {
        self.effect = Some(e.into_prop());
        self
    }
    pub fn margin(mut self, m: impl Into<Margin>) -> Self {
        self.margin = self.margin.merged(m.into());
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = self.padding.merged(p.into());
        self
    }
    pub fn clip(mut self, c: bool) -> Self {
        self.clip = c;
        self
    }
    pub fn is_overlay(mut self, on: bool) -> Self {
        self.is_overlay = on;
        self
    }
    pub fn scrollable(mut self, s: bool) -> Self {
        self.scrollable = s;
        self
    }
    pub fn overscroll(mut self, px: f32) -> Self {
        self.overscroll = px.max(0.0);
        self
    }
    pub fn isolate_children(mut self, on: bool) -> Self {
        self.isolate_children = on;
        self
    }
    pub fn scroll_offset<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.scroll_offset = Some(v.into_prop());
        self
    }
    pub fn scroll_offset_x<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.scroll_offset_x = Some(v.into_prop());
        self
    }
    pub fn scroll_offset_y<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.scroll_offset_y = Some(v.into_prop());
        self
    }
    pub fn direction(mut self, d: impl Into<Direction>) -> Self {
        self.layout.direction = d.into();
        self
    }
    pub fn gap<G: Into<Gap>>(mut self, g: G) -> Self {
        self.layout.gap = g.into();
        self
    }
    pub fn alignment(mut self, a: impl Into<Alignment>) -> Self {
        self.layout.alignment = a.into();
        self
    }
    pub fn distribution(mut self, d: impl Into<Distribution>) -> Self {
        self.layout.distribution = d.into();
        self
    }
    pub fn flow_wrap(mut self, w: impl Into<FlowWrap>) -> Self {
        self.layout.flow_wrap = w.into();
        self
    }
    pub fn row_gap(mut self, gap: f32) -> Self {
        self.layout.gap = self.layout.gap.with_cross(gap);
        self
    }
    pub fn col_gap(mut self, gap: f32) -> Self {
        self.layout.gap = self.layout.gap.with_main(gap);
        self
    }
    pub fn gaps(mut self, main: f32, cross: f32) -> Self {
        self.layout.gap = Gap::Split { main, cross };
        self
    }
    pub fn aspect_ratio(mut self, ratio: impl Into<AspectRatio>) -> Self {
        self.aspect_ratio = Some(ratio.into().ratio());
        self
    }
    pub fn absolute(mut self) -> Self {
        self.is_absolute = true;
        self
    }
    pub fn anchor(mut self, a: impl Into<Anchor>) -> Self {
        self.anchor = Some(a.into());
        self.is_absolute = true;
        self
    }
    pub fn z_index<P: IntoProp<i32>>(mut self, z: P) -> Self {
        self.z_index = Some(z.into_prop());
        self
    }
    pub fn layout_presence<P: IntoProp<LayoutPresence>>(mut self, p: P) -> Self {
        self.presence = Some(p.into_prop());
        self
    }
    pub fn bind_ref(mut self, handle: ElementRef) -> Self {
        self.element_ref = Some(handle);
        self
    }
    pub fn hit_test(mut self, policy: HitTest) -> Self {
        self.hit_test = policy;
        self
    }
    pub fn provided(mut self, grant: crate::reactive::context::Provided) -> Self {
        self.provided.push((grant.id(), grant.reader()));
        self
    }
}

impl IntoElement for GroupElement {
    fn into_element(self) -> Element {
        let def = crate::dsl::scroll::finalize_scroll(self);
        Element::Group(def)
    }
}
