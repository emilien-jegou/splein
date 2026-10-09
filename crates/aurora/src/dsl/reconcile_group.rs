// Single responsibility: Container group reconciliation and reactive binding attachment.

use crate::dsl::group::GroupDef;
use crate::dsl::reconcile_children::reconcile_children;
use crate::dsl::reconciler::evaluate_prop;
use aurora_foundation::{Fill, LayoutPresence, Shadow, Size, Transform};
use crate::input::handlers::NodeHandlers;
use crate::reactive::id::SubscriberId;
use crate::reactive::observer::Observer;
use crate::reactive::prop::DynamicProp;
use crate::reactive::ReactiveRuntime;
use crate::tree::binding::{resolve_z_index, DynamicBindings, NodeBindings, PoseBindings};
use crate::tree::SubscriberRouter;
use crate::tree::{DirtyFlags, LayoutNode, NodeId, NodeKind, TreeArena};
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

    // A reactive child list needs a measure subscriber of its own. Without one a node whose only
    // reactive input is `children_of` would allocate nothing to track against, read its list
    // once at mount, and never hear about a reorder.
    let has_dyn_measure = def.width.as_ref().is_some_and(|p| p.is_dynamic())
        || def.height.as_ref().is_some_and(|p| p.is_dynamic())
        || def.presence.as_ref().is_some_and(|p| p.is_dynamic())
        || def.children_builder.is_some();
    let has_dyn_paint = def.fill.as_ref().is_some_and(|p| p.is_dynamic())
        || def.opacity.as_ref().is_some_and(|p| p.is_dynamic())
        || def.shadow.as_ref().is_some_and(|p| p.is_dynamic())
        || def.transform.as_ref().is_some_and(|p| p.is_dynamic())
        || def.z_index.as_ref().is_some_and(|p| p.is_dynamic())
        || def.x.as_ref().is_some_and(|p| p.is_dynamic())
        || def.y.as_ref().is_some_and(|p| p.is_dynamic())
        || def.scale.as_ref().is_some_and(|p| p.is_dynamic())
        || def.rotate_degrees.as_ref().is_some_and(|p| p.is_dynamic());

    let m_sub = has_dyn_measure.then(|| router.measure_sub(id, &mut runtime.borrow_mut()));
    let p_sub = has_dyn_paint.then(|| router.paint_sub(id, &mut runtime.borrow_mut()));

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
        children: def.children_builder.clone(),
    };
    // Pose components compose into the single transform the tree stores, so the hot path
    // keeps one value per node while the DSL exposes them independently.
    let pose = store.pose.clone();

    // Every reactive property is read here, once, before anything is written. Reading them in
    // two passes with node writes in between meant the second batch could observe a tree the
    // first batch had already half-updated. It also has to precede the handler extraction below,
    // which moves `def.handlers` out and so would leave `def` partially moved.
    let props = ResolvedProps::evaluate(&def, pose.as_ref(), m_sub, p_sub, runtime);
    let change = ChangeSet::detect(arena.get(id), &def, &props);

    let bindings = NodeBindings::from_store(store);
    // Handlers ride alongside the bindings: both are per-node closures that reconciliation
    // replaces wholesale, and both are stored behind one pointer clone.
    let handlers = NodeHandlers::from_store(def.handlers);
    let node = arena.get_mut(id);
    node.state.bindings = bindings;
    node.state.handlers = handlers;

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
        node.state.presence = presence;
    }
    if let Some(w) = props.width {
        node.style.width = w;
    }
    if let Some(h) = props.height {
        node.style.height = h;
    }
    // Persist the declared half separately: the reactive path recomposes from it, and the
    // composed value cannot be decomposed back out.
    node.state.declared_transform = props.declared_transform;
    node.transform = props.transform;
    node.style.layout = def.layout;
    node.style.margin = def.margin;
    node.style.clip = def.clip;
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

    // A node either declares its children statically or builds them from reactive state.
    // Both paths reconcile now, inside the measure observer, so a dynamic list subscribes to
    // the signals it reads on the very first build rather than only on a later re-mount.
    let (children, child_keys) = match &def.children_builder {
        Some(build) => {
            // A reactive list is rebuilt during reconciliation, not during mount, so no
            // ambient hook scope is installed. A child that is a `#[component]` calling
            // `use_signal` would therefore panic with "hook called outside a component body"
            // the first time its parent re-rendered. Entering the scope here makes the two
            // compose: the list can be rebuilt as often as its signals change.
            let cx = crate::reactive::Cx::new(Rc::clone(runtime));
            let spec = if let Some(sub) = m_sub.or(p_sub) {
                runtime
                    .borrow_mut()
                    .push_observer(Observer::Subscriber(sub));
                let built = cx.enter(|| build());
                runtime.borrow_mut().pop_observer();
                built
            } else {
                cx.enter(|| build())
            };
            let keys = spec.iter().map(|e| e.key()).collect();
            (spec, keys)
        }
        None => (def.children, Vec::new()),
    };

    reconcile_children(runtime, arena, router, id, children);
    if def.children_builder.is_some() {
        arena.get_mut(id).state.child_keys = child_keys;
    }
    id
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

        let shadow = match &def.shadow {
            Some(DynamicProp::Static(s)) => Some(s.clone()),
            Some(DynamicProp::Dynamic(_)) => evaluate_prop(&def.shadow, p_sub, runtime),
            None => None,
        };

        Self {
            width,
            height,
            presence,
            declared_transform,
            transform: NodeBindings::compose_with_declared(declared_transform, composed_pose),
            fill: evaluate_prop(&def.fill, p_sub, runtime).flatten(),
            opacity: evaluate_prop(&def.opacity, p_sub, runtime),
            shadow,
            z_index: resolve_z_index(
                evaluate_prop(&def.z_index, p_sub, runtime),
                def.is_absolute,
            ),
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
            props.presence.is_some() && props.presence != Some(prev.state.presence);
        let layout = (props.width.is_some() && props.width != Some(prev.style.width))
            || (props.height.is_some() && props.height != Some(prev.style.height))
            || prev.style.layout != def.layout
            || prev.style.margin != def.margin
            || prev.style.clip != def.clip
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
            || prev.style.z_index != props.z_index;
        Self {
            layout,
            paint,
            transform: prev.transform != props.transform,
        }
    }
}
