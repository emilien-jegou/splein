// Single responsibility: P3 — `key=` on a component tag is reconciler identity.
//
// Two contracts, no pixels:
// 1. The RSX `key=` attribute reaches `Element::key()` (the macro threads it
//    through the generated builder into `Element::with_key`).
// 2. Reconciliation reuses the *same* `NodeId` for the same key across a
//    reorder — identity follows the key, not the position.
//
// Hook-state identity (a component's `use_signal` surviving a list rebuild) is
// P1's `children_labels_keyed` and lives in `reactive_component_state.rs`;
// this file pins the reconciler half only.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{reconcile, Element};
use riode::foundation::Key;
use riode::prelude::*;
use riode::reactive::{Cx, ReactiveRuntime};
use riode::tree::{NodeId, SubscriberRouter, TreeArena};

/// Hook-free chip: state is not the observable here, node identity is.
#[component]
fn Chip(id: &'static str) -> impl IntoElement {
    group()
        .width(Size::fill())
        .height(40.0)
        .children([text(id).size(12.0)])
}

/// `key=` on the tag must surface on the built element; its absence must not.
#[test]
#[ntest::timeout(5_000)]
fn component_tag_key_reaches_element() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&rt));

    let keyed: Element = cx.enter(|| {
        view! {
            <Chip key="alpha" id="alpha" />
        }
    });
    assert_eq!(
        keyed.key(),
        Some(Key("alpha".to_string())),
        "key= on a component tag must set the element's reconciliation key"
    );

    let unkeyed: Element = cx.enter(|| {
        view! {
            <Chip id="beta" />
        }
    });
    assert_eq!(
        unkeyed.key(),
        None,
        "a component tag without key= must stay unkeyed"
    );
}

/// Reordering a keyed list must reuse the existing nodes, not recreate them.
#[test]
#[ntest::timeout(5_000)]
fn reconciler_reuses_keyed_nodes_across_reorder() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&rt));
    let mut arena = TreeArena::new();
    let mut router = SubscriberRouter::default();

    let initial: Element = cx.enter(|| {
        view! {
            <Group key="root" width="fill" height="fill">
                <Chip key="alpha" id="alpha" />
                <Chip key="beta" id="beta" />
            </Group>
        }
    });
    let root = reconcile(&rt, &mut arena, &mut router, None, initial);
    let before: Vec<(NodeId, Option<Key>)> = arena
        .children(root)
        .iter()
        .map(|&c| (c, arena.get(c).key.clone()))
        .collect();
    assert_eq!(
        before.iter().map(|(_, k)| k.clone()).collect::<Vec<_>>(),
        vec![Some(Key("alpha".into())), Some(Key("beta".into()))],
        "the two keyed chips must mount in declaration order"
    );
    let (alpha_before, beta_before) = (before[0].0, before[1].0);

    // Flip the order: identity must follow the key, so the same two NodeIds
    // reappear reversed. Positional reuse would instead hand alpha's node to
    // beta (or recreate both).
    let flipped: Element = cx.enter(|| {
        view! {
            <Group key="root" width="fill" height="fill">
                <Chip key="beta" id="beta" />
                <Chip key="alpha" id="alpha" />
            </Group>
        }
    });
    let root2 = reconcile(&rt, &mut arena, &mut router, Some(root), flipped);
    assert_eq!(root2, root, "the root must be reused, not recreated");

    let after: Vec<(NodeId, Option<Key>)> = arena
        .children(root2)
        .iter()
        .map(|&c| (c, arena.get(c).key.clone()))
        .collect();
    assert_eq!(
        after.iter().map(|(_, k)| k.clone()).collect::<Vec<_>>(),
        vec![Some(Key("beta".into())), Some(Key("alpha".into()))],
        "the flipped list must present beta then alpha"
    );
    assert_eq!(
        after[0].0, beta_before,
        "the beta node must be reused (same NodeId), not recreated"
    );
    assert_eq!(
        after[1].0, alpha_before,
        "the alpha node must be reused (same NodeId), not recreated"
    );
}
