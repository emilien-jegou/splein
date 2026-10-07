// Single responsibility: Fluent declarative Group builder with dynamic properties and helpers.

use std::rc::Rc;

use crate::dsl::component::IntoElement;
use crate::dsl::element::{Element, IntoChildList};
use crate::dsl::element_ref::ElementRef;
use crate::foundation::*;
use crate::input::{ClickInfo, DragInfo, HandlerSet, HitTest, HoverInfo, PointerEvent, Propagation};
use crate::reactive::prop::{DynamicProp, IntoProp};
use crate::reactive::readable::Readable;

/// Declarative flex container builder with reactive properties and styling modifiers.
pub struct GroupDef {
    pub key: Option<Key>,
    pub width: Option<DynamicProp<Size>>,
    pub height: Option<DynamicProp<Size>>,
    pub fill: Option<DynamicProp<Option<Fill>>>,
    pub opacity: Option<DynamicProp<f32>>,
    pub transform: Option<DynamicProp<Transform>>,
    pub x: Option<DynamicProp<f32>>,
    pub y: Option<DynamicProp<f32>>,
    pub scale: Option<DynamicProp<f32>>,
    pub rotate_degrees: Option<DynamicProp<f32>>,
    pub dynamic_shadow: Option<DynamicProp<Shadow>>,
    pub layout: LayoutConfig,
    pub margin: Margin,
    pub stroke: Option<Stroke>,
    pub shadows: Vec<Shadow>,
    pub radius: Radius,
    pub clip: bool,
    pub is_absolute: bool,
    pub anchor: Option<Anchor>,
    /// Stacking order, optionally bound to a reactive value.
    ///
    /// Static like every other prop by default; bound when the caller hands over a signal
    /// or derivation — a dragged row elevating above its siblings while held, for instance.
    /// Paint order (and therefore hit order) follows the retained style, so a change here
    /// repaints but never re-lays-out.
    pub z_index: Option<DynamicProp<i32>>,
    pub is_overlay: bool,
    pub has_layer: bool,
    pub presence: Option<DynamicProp<LayoutPresence>>,
    /// Handle bound to this node at reconciliation, when the caller wants to read its geometry.
    pub element_ref: Option<ElementRef>,
    /// Pointer interaction handlers, reconciled onto the node alongside its bindings.
    pub handlers: HandlerSet,
    /// Hit-test participation for this node (and, for passthrough, its subtree).
    pub hit_test: HitTest,
    /// Builds children from reactive state, when the list is not static.
    pub children_builder: Option<Rc<dyn Fn() -> Vec<Element>>>,
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
        x: None,
        y: None,
        scale: None,
        rotate_degrees: None,
        dynamic_shadow: None,
        layout: LayoutConfig::default(),
        margin: Margin::ZERO,
        stroke: None,
        shadows: Vec::new(),
        radius: Radius::ZERO,
        clip: false,
        is_absolute: false,
        anchor: None,
        z_index: None,
        is_overlay: false,
        has_layer: false,
        presence: None,
        element_ref: None,
        handlers: HandlerSet::default(),
        hit_test: HitTest::Auto,
        children_builder: None,
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

    /// Horizontal translation, usually bound to an animated signal.
    pub fn x<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.x = Some(v.into_prop());
        self
    }

    /// Vertical translation, usually bound to an animated signal.
    pub fn y<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.y = Some(v.into_prop());
        self
    }

    /// Uniform scale, usually bound to an animated signal.
    pub fn scale<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.scale = Some(v.into_prop());
        self
    }

    /// Clockwise rotation in degrees, usually bound to an animated signal.
    pub fn rotate_degrees<P: IntoProp<f32>>(mut self, v: P) -> Self {
        self.rotate_degrees = Some(v.into_prop());
        self
    }

    /// Rotates by an animated value, or applies no rotation when none is animated.
    ///
    /// Composed through `Readable` so a caller may hand over either a `Signal` or a
    /// `Derived` without wrapping it first.
    pub fn rotate_degrees_from(mut self, source: impl Readable<f32> + 'static) -> Self {
        self.rotate_degrees = Some(DynamicProp::Dynamic(Rc::new(move || source.read())));
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
    pub fn z_index<P: IntoProp<i32>>(mut self, z: P) -> Self {
        self.z_index = Some(z.into_prop());
        self
    }
    pub fn overlay(mut self, on: bool) -> Self {
        self.is_overlay = on;
        self
    }
    /// Sets the layout footprint this view presents to its parent.
    pub fn layout_presence<P: IntoProp<LayoutPresence>>(mut self, p: P) -> Self {
        self.presence = Some(p.into_prop());
        self
    }

    /// Binds a geometry handle to this element.
    ///
    /// For a caller that wants to *ask* where the element ended up rather than be told. The
    /// alternative is declaring a measurement sink and having layout push into it, which is the
    /// right shape when a value changes and something downstream must follow — but the wrong one
    /// for simply wanting to know where a thing is, because it turns a read into a reaction that
    /// can invalidate whatever derived from it. See [`ElementRef`]; the short version is that a
    /// read here cannot dirty anything, so it cannot schedule the frame that would invalidate it.
    pub fn bind_ref(mut self, handle: ElementRef) -> Self {
        self.element_ref = Some(handle);
        self
    }
    pub fn children<C: IntoChildList>(mut self, c: C) -> Self {
        self.children = c.into_child_list();
        self
    }

    /// Builds this node's children from reactive state.
    ///
    /// This is how a list reorders without the component re-running: the closure is stored
    /// on the node and re-evaluated when any signal it read changes. Reconciliation happens
    /// only when the resulting key sequence differs, so reordering costs one diff and an
    /// unchanged list costs one comparison.
    ///
    /// Rows should be keyed by identity rather than by position, otherwise a reorder reads
    /// as "every row changed" instead of "these two rows swapped".
    pub fn children_of(mut self, build: impl Fn() -> Vec<Element> + 'static) -> Self {
        self.children_builder = Some(Rc::new(build));
        self
    }

    /// Builds children from a reactive list of labels.
    ///
    /// A convenience over [`Self::children_of`] for the common case where each entry is
    /// just content for an otherwise uniform row.
    pub fn children_labels<S, R, F>(mut self, items: R, row: F) -> Self
    where
        R: Readable<Vec<S>> + 'static,
        S: Clone + 'static,
        F: Fn(usize, S) -> Element + 'static,
    {
        self.children_builder = Some(Rc::new(move || {
            items
                .read()
                .into_iter()
                .enumerate()
                .map(|(i, s)| row(i, s))
                .collect()
        }));
        self
    }

    /// Builds children from a reactive list, keeping each row component's hook state.
    ///
    /// `key_of` names an item *instance*, and that name is the whole point: a reactive list is
    /// rebuilt by re-running its builder, so a row component is constructed again on every
    /// list change. Without a name its hooks allocate fresh signals and the component
    /// silently starts over — a click counter resets, a draft clears, a scroll offset drops —
    /// while everything still looks like it is working.
    ///
    /// The name must identify the item rather than its position. A list that reorders would
    /// otherwise hand each component the state of whichever item moved into its index, which
    /// is worse than resetting: the state is wrong rather than absent.
    ///
    /// Keys must also be unique among the rows of one list.
    pub fn children_labels_keyed<S, R, K, F>(mut self, items: R, key_of: K, row: F) -> Self
    where
        R: Readable<Vec<S>> + 'static,
        S: Clone + 'static,
        K: Fn(&S) -> String + 'static,
        F: Fn(usize, S) -> Element + 'static,
    {
        self.children_builder = Some(Rc::new(move || {
            items
                .read()
                .into_iter()
                .enumerate()
                .map(|(i, s)| {
                    // The row runs in a hook scope named after its item, so `use_signal`
                    // inside a row component finds the signal it allocated last time. The
                    // scope is the one the list is already being built in, which is why this
                    // needs no runtime of its own.
                    let key = key_of(&s);
                    match crate::reactive::Cx::current() {
                        Some(cx) => cx.enter_keyed(key, || row(i, s)),
                        // Built outside any scope, e.g. by a caller that only wants the shape.
                        // The rows still work; they just cannot preserve hook state.
                        None => row(i, s),
                    }
                })
                .collect()
        }));
        self
    }

    /// Handles a completed press and release on this node.
    ///
    /// Fires only when the pointer barely moved between them; a gesture that travelled is a
    /// drag, and a drag never also reports a click.
    pub fn on_click(mut self, f: impl Fn(&ClickInfo) + 'static) -> Self {
        self.handlers.click = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }

    /// Handles a click, with control over whether ancestors still observe it.
    pub fn on_click_handled(
        mut self,
        f: impl Fn(&ClickInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.click = Some(Rc::new(f));
        self
    }

    /// Handles the primary button going down anywhere on this node.
    pub fn on_pointer_down(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.handlers.pointer_down = Some(Rc::new(move |e| {
            f(e);
            Propagation::Bubble
        }));
        self
    }

    /// Handles pointer-down, with control over whether ancestors still observe it.
    pub fn on_pointer_down_handled(
        mut self,
        f: impl Fn(&PointerEvent) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_down = Some(Rc::new(f));
        self
    }

    /// Handles the primary button coming up on this node.
    pub fn on_pointer_up(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.handlers.pointer_up = Some(Rc::new(move |e| {
            f(e);
            Propagation::Bubble
        }));
        self
    }

    /// Handles pointer-up, with control over whether ancestors still observe it.
    pub fn on_pointer_up_handled(
        mut self,
        f: impl Fn(&PointerEvent) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_up = Some(Rc::new(f));
        self
    }

    /// Handles every pointer move while it is over this node.
    pub fn on_pointer_move(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.handlers.pointer_move = Some(Rc::new(move |e| {
            f(e);
            Propagation::Bubble
        }));
        self
    }

    /// Handles pointer-move, with control over whether ancestors still observe it.
    pub fn on_pointer_move_handled(
        mut self,
        f: impl Fn(&PointerEvent) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_move = Some(Rc::new(f));
        self
    }

    /// Handles the pointer arriving on this node.
    pub fn on_pointer_enter(mut self, f: impl Fn(&HoverInfo) + 'static) -> Self {
        self.handlers.pointer_enter = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }

    /// Handles pointer-enter, with control over whether ancestors still observe it.
    pub fn on_pointer_enter_handled(
        mut self,
        f: impl Fn(&HoverInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_enter = Some(Rc::new(f));
        self
    }

    /// Handles the pointer leaving this node.
    pub fn on_pointer_leave(mut self, f: impl Fn(&HoverInfo) + 'static) -> Self {
        self.handlers.pointer_leave = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }

    /// Handles pointer-leave, with control over whether ancestors still observe it.
    pub fn on_pointer_leave_handled(
        mut self,
        f: impl Fn(&HoverInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.pointer_leave = Some(Rc::new(f));
        self
    }

    /// Handles a press that has travelled far enough to become a drag.
    ///
    /// The node captures the gesture, so it keeps receiving moves after the pointer leaves
    /// it, and only a release ends it.
    pub fn on_drag_start(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.handlers.drag_start = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }

    /// Handles a drag-start, with control over whether ancestors still observe it.
    pub fn on_drag_start_handled(
        mut self,
        f: impl Fn(&DragInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.drag_start = Some(Rc::new(f));
        self
    }

    /// Handles each pointer sample of an active drag.
    pub fn on_drag_step(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.handlers.drag_step = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }

    /// Handles a drag-step, with control over whether ancestors still observe it.
    pub fn on_drag_step_handled(
        mut self,
        f: impl Fn(&DragInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.drag_step = Some(Rc::new(f));
        self
    }

    /// Handles the end of a drag, carrying the release velocity.
    ///
    /// This is the hand-off point for a gesture into physics: the velocity here is what the
    /// element had when the finger left the screen, and handing it to a spring is what stops
    /// a flick from appearing to stop dead.
    pub fn on_drag_end(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.handlers.drag_end = Some(Rc::new(move |info| {
            f(info);
            Propagation::Bubble
        }));
        self
    }

    /// Handles a drag-end, with control over whether ancestors still observe it.
    pub fn on_drag_end_handled(
        mut self,
        f: impl Fn(&DragInfo) -> Propagation + 'static,
    ) -> Self {
        self.handlers.drag_end = Some(Rc::new(f));
        self
    }

    /// Hit-test participation for this node.
    ///
    /// `HitTest::Opaque` makes even handler-less dead space solid, so clicks on a
    /// menu or modal never ghost through to nodes beneath. `HitTest::Passthrough`
    /// makes the subtree invisible to collision (the default for layout spacers,
    /// now formalized rather than implied by having no handlers).
    pub fn hit_test(mut self, policy: HitTest) -> Self {
        self.hit_test = policy;
        self
    }

    /// Toggles the layout footprint this view presents to its parent.
    ///
    /// Sugar over [`Self::layout_presence`]: `visible(true)` is
    /// `LayoutPresence::Present`, `visible(false)` is `LayoutPresence::Absent`.
    /// Accepts any bool prop (literal, `Signal<bool>`, `Derived<bool>`, closure).
    pub fn visible<P: IntoProp<bool>>(mut self, v: P) -> Self {
        let prop = v.into_prop();
        self.presence = Some(match prop {
            DynamicProp::Static(on) => DynamicProp::Static(if on {
                LayoutPresence::Present
            } else {
                LayoutPresence::Absent
            }),
            DynamicProp::Dynamic(f) => {
                DynamicProp::Dynamic(Rc::new(move || {
                    if f() {
                        LayoutPresence::Present
                    } else {
                        LayoutPresence::Absent
                    }
                }))
            }
        });
        self
    }

    /// Attaches an interaction behavior, decoupling gestures from the DSL.
    ///
    /// A behavior owns its state (one clone, inside the behavior) and fans out to
    /// the typed `on_*` slots, so call sites read as one verb instead of five
    /// closures: `row.attach(ReorderGesture::new(drag))`.
    pub fn attach<B: Behavior>(self, behavior: B) -> Self {
        behavior.attach(self)
    }
}

/// An interaction behavior decoupled from the element DSL.
///
/// Implementors own their signal clones and attach to the typed handler slots,
/// keeping `GroupDef` free of gesture-specific builder noise.
pub trait Behavior {
    /// Attaches this behavior's handlers to a group builder.
    fn attach(self, row: GroupDef) -> GroupDef;
}

impl IntoElement for GroupDef {
    fn into_element(self) -> Element {
        Element::Group(self)
    }
}
