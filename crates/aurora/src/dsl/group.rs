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
#[derive(Clone)]
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
    pub shadow: Option<DynamicProp<Shadow>>,
    pub light_override: Option<LightEnvironment>,
    pub layout: LayoutConfig,
    pub margin: Margin,
    pub stroke: Option<Stroke>,
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
    /// Ordered child sources: static elements and reactive builders interleaved.
    ///
    /// Control-flow nodes (`<for>`, `<if>`, `<match>`) contribute `Dynamic`
    /// segments directly into their parent — never carrier nodes — so rows and
    /// branches stack in the parent's layout with nothing between. A node with
    /// no `Dynamic` segment reconciles exactly as before.
    pub child_segments: Vec<ChildSegment>,
}

/// One ordered child source of a group.
///
/// `Static` holds eager elements; `Dynamic` holds a builder re-evaluated when
/// any signal it reads changes (reconciliation then diffs the merged key
/// sequence, so an unchanged rebuild costs one comparison). Builders run under
/// the owning node's observer, however deeply nested, so every read
/// subscribes the owner.
#[derive(Clone)]
pub enum ChildSegment {
    Static(Vec<Element>),
    Dynamic(Rc<dyn Fn() -> Vec<ChildSegment>>),
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
        shadow: None,
        light_override: None,
        layout: LayoutConfig::default(),
        margin: Margin::ZERO,
        stroke: None,
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
        child_segments: Vec::new(),
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
    /// Shadow from anything shadow-like: an elevation (`8.0`), an
    /// `(elevation, color)` pair, a [`Shadow`], a closure `move || ...`, or
    /// a reactive signal. Static values stay static; closures and signals
    /// re-evaluate when their dependencies change.
    pub fn shadow<P: IntoProp<Shadow>>(mut self, s: P) -> Self {
        self.shadow = Some(s.into_prop());
        self
    }

    /// Light override for this subtree: children inherit it unless they
    /// declare their own. Resolved during paint traversal, so overrides
    /// never touch layout or reconciliation.
    pub fn light(mut self, light: LightEnvironment) -> Self {
        self.light_override = Some(light);
        self
    }

    /// Merges a margin: nonzero sides of `m` win, the rest is kept.
    ///
    /// Chained calls compose (`.margin(left(..)).margin(top(..))`), which is the
    /// only reading under which the per-side constructors make sense.
    pub fn margin(mut self, m: impl Into<Margin>) -> Self {
        self.margin = self.margin.merged(m.into());
        self
    }

    pub fn stroke(mut self, s: impl Into<Stroke>) -> Self {
        self.stroke = Some(s.into());
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
    /// Appends static children. Repeated calls accumulate in order (they do
    /// not replace); interleaving with the `children_*` builders keeps
    /// declaration order, statics and dynamics alike.
    pub fn children<C: IntoChildList>(mut self, c: C) -> Self {
        self.child_segments
            .push(ChildSegment::Static(c.into_child_list()));
        self
    }

    /// Appends pre-built child segments (static elements and reactive builders
    /// interleaved). This is what `view!` emits for child lists containing
    /// control-flow nodes; handwritten code should prefer [`Self::children`]
    /// plus the `children_*` builders, which cover the same ground.
    pub fn child_segments(mut self, segs: Vec<ChildSegment>) -> Self {
        self.child_segments.extend(segs);
        self
    }

    /// Whether any segment rebuilds from reactive state.
    pub fn has_dynamic_segments(&self) -> bool {
        self.child_segments
            .iter()
            .any(|s| matches!(s, ChildSegment::Dynamic(_)))
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
        self.child_segments.push(ChildSegment::Dynamic(Rc::new(move || {
            vec![ChildSegment::Static(build())]
        })));
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
        self.child_segments.push(ChildSegment::Dynamic(Rc::new(move || {
            vec![ChildSegment::Static(
                items
                    .read()
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| row(i, s))
                    .collect(),
            )]
        })));
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
        self.child_segments.push(ChildSegment::Dynamic(Rc::new(move || {
            vec![ChildSegment::Static(
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
                    .collect(),
            )]
        })));
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

/// Keyed-list free function for programmatic (non-RSX) use.
///
/// Thin wrapper over [`GroupDef::children_labels_keyed`]: `key_of` names an
/// item *instance* (stable + unique, not position), `row` builds one `Element`
/// from `(index, item)`. Rows run in `enter_keyed` scopes so row-component
/// hooks survive list changes. RSX code should prefer `<for>`, which splices
/// rows directly into the parent with no carrier node.
pub fn keyed_list<S, R, K, F>(items: R, key_of: K, row: F) -> Element
where
    R: Readable<Vec<S>> + 'static,
    S: Clone + 'static,
    K: Fn(&S) -> String + 'static,
    F: Fn(usize, S) -> Element + 'static,
{
    crate::dsl::group()
        .children_labels_keyed(items, key_of, row)
        .into_element()
}

/// Index-keyed variant for programmatic (non-RSX) use.
///
/// Correct for static lists (never reorder/filter/insert); rows reset state on
/// reorder by design. Prefer [`keyed_list`] with a per-item key otherwise.
pub fn index_keyed_list<S, R, F>(items: R, row: F) -> Element
where
    R: Readable<Vec<S>> + 'static,
    S: Clone + 'static,
    F: Fn(usize, S) -> Element + 'static,
{
    crate::dsl::group()
        .children_of(move || {
            items
                .read()
                .into_iter()
                .enumerate()
                .map(|(i, s)| match crate::reactive::Cx::current() {
                    Some(cx) => cx.enter_keyed(format!("index-{i}"), || row(i, s)),
                    None => row(i, s),
                })
                .collect()
        })
        .into_element()
}

impl ChildSegment {
    /// Keyed-list segment backing `view!`'s `<for>` node: rows splice directly
    /// into the parent, with no carrier node between.
    ///
    /// `key_of` names an item *instance* and each row builds a segment vec
    /// (usually one element; nested control flow stays nested and flattens at
    /// build). Row scopes mirror [`GroupDef::children_labels_keyed`].
    pub fn keyed<S, R, K, F>(items: R, key_of: K, row: F) -> Self
    where
        R: Readable<Vec<S>> + 'static,
        S: Clone + 'static,
        K: Fn(&S) -> String + 'static,
        F: Fn(usize, S) -> Vec<ChildSegment> + 'static,
    {
        Self::Dynamic(Rc::new(move || {
            items
                .read()
                .into_iter()
                .enumerate()
                .flat_map(|(i, s)| {
                    let key = key_of(&s);
                    match crate::reactive::Cx::current() {
                        Some(cx) => cx.enter_keyed(key, || row(i, s)),
                        None => row(i, s),
                    }
                })
                .collect()
        }))
    }

    /// Index-keyed segment for `<for>` without `key=` (see [`index_keyed_list`]).
    pub fn keyed_index<S, R, F>(items: R, row: F) -> Self
    where
        R: Readable<Vec<S>> + 'static,
        S: Clone + 'static,
        F: Fn(usize, S) -> Vec<ChildSegment> + 'static,
    {
        Self::Dynamic(Rc::new(move || {
            items
                .read()
                .into_iter()
                .enumerate()
                .flat_map(|(i, s)| match crate::reactive::Cx::current() {
                    Some(cx) => cx.enter_keyed(format!("index-{i}"), || row(i, s)),
                    None => row(i, s),
                })
                .collect()
        }))
    }
}

/// Flattens ordered segments to elements, resolving nested dynamics
/// recursively. Runs under the caller's observer, so every signal any nested
/// builder reads subscribes the owning node.
pub fn flatten_segments(segments: &[ChildSegment]) -> Vec<Element> {
    let mut out = Vec::new();
    for seg in segments {
        match seg {
            ChildSegment::Static(els) => out.extend(els.iter().cloned()),
            ChildSegment::Dynamic(build) => out.extend(flatten_segments(&build())),
        }
    }
    out
}
