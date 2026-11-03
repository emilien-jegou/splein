// Single responsibility: Container group reconciliation and reactive binding attachment.

use crate::dsl::element::Element;
use crate::dsl::group::GroupDef;
use crate::dsl::reconcile_children::reconcile_children;
use crate::dsl::reconciler::{evaluate_prop, reconcile};
use crate::foundation::{LayerPaint, LayoutPresence, Size, Transform};
use crate::input::handlers::NodeHandlers;
use crate::reactive::id::SubscriberId;
use crate::reactive::observer::Observer;
use crate::reactive::prop::Prop;
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

    // Evaluation node for context reads below: every Prop::Context in
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
    let has_dyn_paint = def.material.as_ref().is_some_and(|p| p.is_dynamic())
        || def.shape.as_ref().is_some_and(|p| p.is_dynamic())
        || def.overlay.as_ref().is_some_and(|p| p.is_dynamic())
        || def.effect.as_ref().is_some_and(|p| p.is_dynamic())
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
        transform: def.transform.as_ref().and_then(|p| p.dynamic_closure()),
        paint: paint_binding(&def),
        text: None,
        presence: def.presence.as_ref().and_then(|p| p.dynamic_closure()),
        z_index: def.z_index.as_ref().and_then(|p| p.dynamic_closure()),
        pose: pose_bindings(&def),
        scroll_offset: def.scroll_offset.as_ref().and_then(|p| p.dynamic_closure()),
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
        "riode: reconcile group {:?} paint={:?} layout={} paint_change={} transform={}",
        id,
        props.paint,
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

    node.style.paint = props.paint;

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
        children.push(id);
        arena.set_children(parent, children);
        id
    };
    arena.bind_key(key, child);
    arena.mark_dirty(parent, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    child
}

/// Collects the paint evaluator a group declared, if any.
///
/// One closure over the whole bundle: whichever of the four paint props is
/// dynamic (or a context read) makes the node reactive through the single
/// paint channel, and the static siblings ride along as cheap clones.
fn paint_binding(def: &GroupDef) -> Option<Rc<dyn Fn() -> LayerPaint>> {
    let any_dynamic = def.shape.as_ref().is_some_and(|p| p.is_dynamic())
        || def.material.as_ref().is_some_and(|p| p.is_dynamic())
        || def.overlay.as_ref().is_some_and(|p| p.is_dynamic())
        || def.effect.as_ref().is_some_and(|p| p.is_dynamic());
    if !any_dynamic {
        return None;
    }
    let shape = def.shape.clone();
    let material = def.material.clone();
    let overlay = def.overlay.clone();
    let effect = def.effect.clone();
    Some(Rc::new(move || LayerPaint {
        shape: shape.as_ref().map(|p| p.evaluate()),
        material: material.as_ref().map(|p| p.evaluate()),
        overlay: overlay.as_ref().map(|p| p.evaluate()),
        effect: effect.as_ref().map(|p| p.evaluate()),
    }))
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

/// Composes the pose components a group declared as plain values, if any.
fn static_pose(def: &GroupDef) -> Option<Transform> {
    let static_val = |p: &Option<Prop<f32>>| match p {
        Some(Prop::Static(v)) => Some(*v),
        _ => None,
    };
    let x = static_val(&def.x);
    let y = static_val(&def.y);
    let rotation = static_val(&def.rotate_degrees);
    let scale = static_val(&def.scale);
    if x.is_none() && y.is_none() && rotation.is_none() && scale.is_none() {
        return None;
    }
    let mut out = Transform::IDENTITY;
    if let Some(x) = x {
        if x != 0.0 {
            out.tx = x;
        }
    }
    if let Some(y) = y {
        if y != 0.0 {
            out.ty = y;
        }
    }
    if let Some(degrees) = rotation {
        if degrees != 0.0 {
            out = out.multiply(&Transform::from_rotation_degrees(degrees));
        }
    }
    if let Some(scale) = scale {
        if scale != 1.0 {
            out = out.multiply(&Transform::from_scale(scale, scale));
        }
    }
    Some(out)
}

/// Every reactive property of a group, read once before any of them is written.
struct ResolvedProps {
    width: Option<Size>,
    height: Option<Size>,
    presence: Option<LayoutPresence>,
    declared_transform: Transform,
    transform: Transform,
    paint: LayerPaint,
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
        let declared_transform = match static_pose(def) {
            Some(statics) => declared_transform.multiply(&statics),
            None => declared_transform,
        };

        let scroll_offset = evaluate_prop(&def.scroll_offset, s_sub, runtime).unwrap_or(0.0);
        let transform = NodeBindings::compose_with_scroll(
            NodeBindings::compose_with_declared(declared_transform, composed_pose),
            scroll_offset,
        );

        let paint = LayerPaint {
            shape: match &def.shape {
                Some(Prop::Static(s)) => Some(s.clone()),
                Some(Prop::Dynamic(_)) | Some(Prop::Context(_)) => {
                    evaluate_prop(&def.shape, p_sub, runtime)
                }
                None => None,
            },
            material: match &def.material {
                Some(Prop::Static(m)) => Some(m.clone()),
                Some(Prop::Dynamic(_)) | Some(Prop::Context(_)) => {
                    evaluate_prop(&def.material, p_sub, runtime)
                }
                None => None,
            },
            overlay: match &def.overlay {
                Some(Prop::Static(o)) => Some(o.clone()),
                Some(Prop::Dynamic(_)) | Some(Prop::Context(_)) => {
                    evaluate_prop(&def.overlay, p_sub, runtime)
                }
                None => None,
            },
            effect: match &def.effect {
                Some(Prop::Static(e)) => Some(e.clone()),
                Some(Prop::Dynamic(_)) | Some(Prop::Context(_)) => {
                    evaluate_prop(&def.effect, p_sub, runtime)
                }
                None => None,
            },
        };

        Self {
            width,
            height,
            presence,
            declared_transform,
            transform,
            paint,
            z_index: resolve_z_index(evaluate_prop(&def.z_index, p_sub, runtime), def.is_absolute),
        }
    }
}

/// Which invalidation passes a reconciliation has to schedule.
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
        let paint = prev.style.paint != props.paint
            || prev.style.z_index != props.z_index
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
