// Single responsibility: Container group reconciliation and reactive binding attachment.

use crate::dsl::element::Element;
use crate::dsl::group::GroupDef;
use crate::dsl::reconcile_children::reconcile_children;
use crate::dsl::reconciler::{evaluate_prop, reconcile};
use crate::foundation::{Fill, LayoutPresence, Shadow, Size, Transform};
use crate::input::handlers::NodeHandlers;
use crate::reactive::id::SubscriberId;
use crate::reactive::observer::Observer;
use crate::reactive::prop::DynamicProp;
use crate::reactive::ReactiveRuntime;
use crate::tree::binding::{resolve_z_index, DynamicBindings, NodeBindings, PoseBindings};
use crate::tree::SubscriberRouter;
use crate::tree::{DirtyFlags, NodeId, NodeKind};
use crate::tree::{LayoutNode, TreeArena};
use std::cell::RefCell;
use std::rc::Rc;

/// Reconciles flex group container properties, styles, and children into the arena.
pub fn reconcile_group(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    existing: Option<NodeId>,
    def: GroupDef,
) -> NodeId {
    let id = existing.unwrap_or_else(|| arena.insert(LayoutNode::default()));

    // Evaluation node for context reads below: every DynamicProp::Context in
    // this declaration resolves against the retained ancestors of THIS node.
    // Restored on drop (regions nest: scheduler rebuilds reconcile children).
    let _eval_node = crate::reactive::context::enter_node(id, arena);

    // A reactive child segment needs a measure subscriber of its own. Without one a node whose only
    // reactive input is dynamic children would allocate nothing to track against, read its list
    // once at mount, and never hear about a reorder.
    let has_dyn_measure = def.width.as_ref().is_some_and(|p| p.is_dynamic())
        || def.height.as_ref().is_some_and(|p| p.is_dynamic())
        || def.presence.as_ref().is_some_and(|p| p.is_dynamic())
        || def.has_dynamic_segments();
    let has_dyn_paint = def.fill.as_ref().is_some_and(|p| p.is_dynamic())
        || def.opacity.as_ref().is_some_and(|p| p.is_dynamic())
        || def.shadow.as_ref().is_some_and(|p| p.is_dynamic())
        || def.transform.as_ref().is_some_and(|p| p.is_dynamic())
        || def.z_index.as_ref().is_some_and(|p| p.is_dynamic())
        || def.x.as_ref().is_some_and(|p| p.is_dynamic())
        || def.y.as_ref().is_some_and(|p| p.is_dynamic())
        || def.scale.as_ref().is_some_and(|p| p.is_dynamic())
        || def.rotate_degrees.as_ref().is_some_and(|p| p.is_dynamic());
    // Scroll offset rides its own channel: same transform effect as x/y, but
    // tracked as SCROLL so damage applies the scroll rule instead of repaint.
    let has_dyn_scroll = def.scroll_offset.as_ref().is_some_and(|p| p.is_dynamic());

    let m_sub = has_dyn_measure.then(|| router.measure_sub(id, &mut runtime.borrow_mut()));
    let p_sub = has_dyn_paint.then(|| router.paint_sub(id, &mut runtime.borrow_mut()));
    let s_sub = has_dyn_scroll.then(|| router.scroll_sub(id, &mut runtime.borrow_mut()));

    let store = DynamicBindings {
        width: def.width.as_ref().and_then(|p| p.dynamic_closure()),
        height: def.height.as_ref().and_then(|p| p.dynamic_closure()),
        fill: def.fill.as_ref().and_then(|p| p.dynamic_closure()),
        opacity: def.opacity.as_ref().and_then(|p| p.dynamic_closure()),
        transform: def.transform.as_ref().and_then(|p| p.dynamic_closure()),

        shadow: def
            .shadow
            .as_ref()
            .and_then(|p| p.dynamic_closure())
            .map(|s| Rc::new(move || Some(s())) as _),
        text: None,
        presence: def.presence.as_ref().and_then(|p| p.dynamic_closure()),
        z_index: def.z_index.as_ref().and_then(|p| p.dynamic_closure()),
        pose: pose_bindings(&def),
        scroll_offset: def
            .scroll_offset
            .as_ref()
            .and_then(|p| p.dynamic_closure()),
        children: def
            .has_dynamic_segments()
            .then(|| def.child_segments.clone()),
    };
    // Pose components compose into the single transform the tree stores, so the hot path
    // keeps one value per node while the DSL exposes them independently.
    let pose = store.pose.clone();

    // Every reactive property is read here, once, before anything is written. Reading them in
    // two passes with node writes in between meant the second batch could observe a tree the
    // first batch had already half-updated. It also has to precede the handler extraction below,
    // which moves `def.handlers` out and so would leave `def` partially moved.
    let props = ResolvedProps::evaluate(&def, pose.as_ref(), m_sub, p_sub, s_sub, runtime);
    let change = ChangeSet::detect(arena.get(id), &def, &props);
    tracing::debug!(
        "aurora: reconcile group {:?} fill={:?} layout={} paint={} transform={}",
        id,
        props.fill,
        change.layout,
        change.paint,
        change.transform,
    );
    // Hoisted before the writes below move fields out of `def`.
    let has_dynamics = def.has_dynamic_segments();

    let bindings = NodeBindings::from_store(store);
    // Handlers ride alongside the bindings: both are per-node closures that reconciliation
    // replaces wholesale, and both are stored behind one pointer clone.
    let handlers = NodeHandlers::from_store(def.handlers);
    let node = arena.get_mut(id);
    node.state.core.bindings = bindings;
    node.state.input.handlers = handlers;
    // Bindings decide staticness: clear this node's proof and ancestors'.
    arena.invalidate_static_up(id);

    if change.layout {
        arena.mark_dirty(id, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    }
    if change.paint || change.transform {
        arena.mark_dirty(id, DirtyFlags::PAINT);
    }

    // Same reasoning for a geometry handle: this is the first and only moment the node id exists,
    // so this is where a handle becomes readable. Re-binding on a rebuild is what keeps the
    // handle pointing at the retained node rather than the one it replaced.
    if let Some(handle) = &def.element_ref {
        handle.bind(id);
    }

    let node = arena.get_mut(id);
    node.kind = NodeKind::Group;
    if let Some(presence) = props.presence {
        node.state.core.presence = presence;
    }
    if let Some(w) = props.width {
        node.style.width = w;
    }
    if let Some(h) = props.height {
        node.style.height = h;
    }
    // Persist the declared half separately: the reactive path recomposes from it, and the
    // composed value cannot be decomposed back out.
    node.state.core.declared_transform = props.declared_transform;
    node.transform = props.transform;
    node.style.layout = def.layout;
    node.style.aspect_ratio = def.aspect_ratio;
    node.style.margin = def.margin;
    node.style.padding = def.padding;
    node.style.clip = def.clip;
    node.style.scrollable = def.scrollable;
    node.style.overscroll = def.overscroll;
    node.style.isolate_children = def.isolate_children;
    node.style.is_absolute = def.is_absolute;
    node.style.anchor = def.anchor;
    node.style.z_index = props.z_index;
    node.style.is_overlay = def.is_overlay;
    node.style.has_layer = def.has_layer;
    node.style.hit_test = def.hit_test;
    let node = arena.get_mut(id);
    if let Some(f) = props.fill {
        node.style.appearance.fill = Some(f);
    }
    if let Some(op) = props.opacity {
        node.style.appearance.opacity = op;
    }
    node.style.appearance.stroke = def.stroke;
    node.style.appearance.shadow = props.shadow.clone();
    node.style.appearance.light_override = def.light_override;
    node.style.appearance.radius = def.radius;

    // Provided context values live on the retained node (cleared when the
    // declaration carries none, so conditional providers vanish cleanly) and
    // on the descent stack for the children reconcile below: freshly mounted
    // links don't exist in the tree yet, but the lexical chain is live.
    // Reads walk ancestors for the nearest entry; see `reactive::context`.
    if def.provided.is_empty() {
        arena.get_mut(id).state.core.provided = None;
    } else {
        arena.get_mut(id).state.core.provided = Some(def.provided.clone());
    }
    // Providers are reactive (signal readers): same invalidation.
    arena.invalidate_static_up(id);
    let _descent = crate::reactive::context::enter_descent(&def.provided);

    // Children merge in declaration order: static segments splice in place,
    // dynamic segments build now, inside the measure observer, so a builder
    // subscribes to the signals it reads on the very first build rather than
    // only on a later re-mount. Static-only nodes skip all of this: the merged
    // vec moves out with no cloning and no subscription.
    let (children, child_keys) = if has_dynamics {
        // A reactive segment is rebuilt during reconciliation, not during mount, so no
        // ambient hook scope is installed. A child that is a `#[component]` calling
        // `use_signal` would therefore panic with "hook called outside a component body"
        // the first time its parent re-rendered. Entering the scope here makes the two
        // compose: the list can be rebuilt as often as its signals change.
        let cx = crate::reactive::Cx::new(Rc::clone(runtime));
        let spec = if let Some(sub) = m_sub.or(p_sub).or(s_sub) {
            runtime
                .borrow_mut()
                .push_observer(Observer::Subscriber(sub));
            let built = cx.enter(|| crate::dsl::group::flatten_segments(&def.child_segments));
            runtime.borrow_mut().pop_observer();
            built
        } else {
            cx.enter(|| crate::dsl::group::flatten_segments(&def.child_segments))
        };
        let keys = spec.iter().map(|e| e.key()).collect();
        (spec, keys)
    } else {
        let mut merged = Vec::new();
        for seg in def.child_segments {
            let crate::dsl::group::ChildSegment::Static(els) = seg else {
                unreachable!("has_dynamic_segments was false")
            };
            merged.extend(els);
        }
        (merged, Vec::new())
    };

    reconcile_children(runtime, arena, router, id, children);
    if has_dynamics {
        arena.get_mut(id).state.core.child_keys = child_keys;
    }
    id
}

/// Replaces a single child slot of `parent` with a freshly built element.
///
/// This is the surgical half of hot-reload: re-running one component factory yields one new
/// element tree, and only the slot it belongs to is re-reconciled — siblings keep their
/// `NodeId`s, their layout caches, and their state. `slot` past the end appends.
///
/// Returns the (possibly reused) child node.
pub fn replace_child(
    runtime: &Rc<RefCell<ReactiveRuntime>>,
    arena: &mut TreeArena,
    router: &mut SubscriberRouter,
    parent: NodeId,
    slot: usize,
    element: Element,
) -> NodeId {
    let key = element.key();
    let mut children: Vec<NodeId> = arena.children(parent).to_vec();
    let child = if slot < children.len() {
        let old = children[slot];
        let id = reconcile(runtime, arena, router, Some(old), element);
        if id != old {
            children[slot] = id;
            arena.set_children(parent, children);
        }
        id
    } else {
        let id = reconcile(runtime, arena, router, None, element);
        // Pad with nothing: callers pass exact slots; beyond-the-end appends.
        children.push(id);
        arena.set_children(parent, children);
        id
    };
    arena.bind_key(key, child);
    arena.mark_dirty(parent, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    child
}

/// Collects the pose component evaluators a group declared, if any.
fn pose_bindings(def: &GroupDef) -> Option<PoseBindings> {
    let pose = PoseBindings {
        x: def.x.as_ref().and_then(|p| p.dynamic_closure()),
        y: def.y.as_ref().and_then(|p| p.dynamic_closure()),
        scale: def.scale.as_ref().and_then(|p| p.dynamic_closure()),
        rotation: def
            .rotate_degrees
            .as_ref()
            .and_then(|p| p.dynamic_closure()),
    };
    pose.is_bound().then_some(pose)
}

/// Every reactive property of a group, read once before any of them is written.
///
/// Grouping them matters for two reasons. Reconciliation has to decide *all* the change flags
/// before it writes *any* of the node, so the reads cannot be interleaved with the writes; and
/// each property has to be read under its own observer scope, which is easy to get subtly wrong
/// when the reads are scattered through the body.
struct ResolvedProps {
    width: Option<Size>,
    height: Option<Size>,
    presence: Option<LayoutPresence>,
    /// The transform as declared, before pose components are folded in.
    declared_transform: Transform,
    /// The declared transform with the bound pose composed into it.
    transform: Transform,
    fill: Option<Fill>,
    opacity: Option<f32>,
    /// Static or dynamic shadow the node actually paints.
    shadow: Option<Shadow>,
    /// Effective stacking order, absoluteness folded in, read under the paint observer when
    /// dynamic so a signal change repaints.
    z_index: i32,
}

impl ResolvedProps {
    /// Reads every declared property under the observer scope its own bindings need.
    fn evaluate(
        def: &GroupDef,
        pose: Option<&PoseBindings>,
        m_sub: Option<SubscriberId>,
        p_sub: Option<SubscriberId>,
        s_sub: Option<SubscriberId>,
        runtime: &Rc<RefCell<ReactiveRuntime>>,
    ) -> Self {
        let width = evaluate_prop(&def.width, m_sub, runtime);
        let height = evaluate_prop(&def.height, m_sub, runtime);
        let presence = evaluate_prop(&def.presence, m_sub, runtime);
        let declared = evaluate_prop(&def.transform, p_sub, runtime);

        // The pose has to be composed with the paint observer installed, or the signals behind
        // `.x() .y() .scale() .rotate_degrees()` are read once at mount and never again: nothing
        // would ever notify this node, because `evaluate_prop` only opens a scope when the node
        // declares a transform of its own.
        let composed_pose = pose.map(|p| {
            let Some(sub) = p_sub else {
                return p.compose();
            };
            runtime
                .borrow_mut()
                .push_observer(Observer::Subscriber(sub));
            let composed = p.compose();
            runtime.borrow_mut().pop_observer();
            composed
        });
        let declared_transform = declared.unwrap_or(Transform::IDENTITY);

        // Scroll offset reads under its own subscriber so it schedules SCROLL
        // rather than paint: same transform effect, cheaper damage downstream.
        let scroll_offset = evaluate_prop(&def.scroll_offset, s_sub, runtime).unwrap_or(0.0);
        let transform = NodeBindings::compose_with_scroll(
            NodeBindings::compose_with_declared(declared_transform, composed_pose),
            scroll_offset,
        );

        let shadow = match &def.shadow {
            Some(DynamicProp::Static(s)) => Some(s.clone()),
            Some(DynamicProp::Dynamic(_)) | Some(DynamicProp::Context(_)) => {
                evaluate_prop(&def.shadow, p_sub, runtime)
            }
            None => None,
        };

        Self {
            width,
            height,
            presence,
            declared_transform,
            transform,
            fill: evaluate_prop(&def.fill, p_sub, runtime).flatten(),
            opacity: evaluate_prop(&def.opacity, p_sub, runtime),
            shadow,
            z_index: resolve_z_index(evaluate_prop(&def.z_index, p_sub, runtime), def.is_absolute),
        }
    }
}

/// Which invalidation passes a reconciliation has to schedule.
///
/// Detected against the node as it was *before* any write, and combined before marking, so a
/// single pass over the tree sees one merged flag set instead of several.
struct ChangeSet {
    layout: bool,
    paint: bool,
    transform: bool,
}

impl ChangeSet {
    /// Compares freshly resolved properties against the node's retained state.
    fn detect(prev: &LayoutNode, def: &GroupDef, props: &ResolvedProps) -> Self {
        let presence_changed =
            props.presence.is_some() && props.presence != Some(prev.state.core.presence);
        let layout = (props.width.is_some() && props.width != Some(prev.style.width))
            || (props.height.is_some() && props.height != Some(prev.style.height))
            || prev.style.layout != def.layout
            || prev.style.aspect_ratio != def.aspect_ratio
            || prev.style.margin != def.margin
            || prev.style.padding != def.padding
            || prev.style.clip != def.clip
            || prev.style.anchor != def.anchor
            || prev.style.is_absolute != def.is_absolute
            || presence_changed;
        let app = &prev.style.appearance;
        let paint = app.fill != props.fill
            || (props.opacity.is_some() && props.opacity != Some(app.opacity))
            || app.shadow != props.shadow
            || app.light_override != def.light_override
            || app.stroke != def.stroke
            || app.radius != def.radius
            // Stacking is paint-class: a new z reorders paint (and hit testing, which mirrors
            // paint) without touching layout.
            || prev.style.z_index != props.z_index
            // Scroll viewport flags change what gets encoded/culled, never a
            // box: repaint-class. The SUBTREE_DIRTY that mark_dirty adds is
            // what forces the subtree to re-encode under the new clip.
            || prev.style.scrollable != def.scrollable
            || prev.style.overscroll != def.overscroll
            || prev.style.isolate_children != def.isolate_children;
        Self {
            layout,
            paint,
            transform: prev.transform != props.transform,
        }
    }
}
