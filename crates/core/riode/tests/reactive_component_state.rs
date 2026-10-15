// Single responsibility: A component's own hook state must survive its list rebuilding it.
//
// A reactive child list is re-evaluated by re-running its builder, so a row component is
// constructed again on every list change. The engine can match the rebuilt row to the node it
// already has, but the signals the row allocated during construction are gone by then unless
// the hook allocator is told which instance it is allocating for. These tests pin that
// contract, including the way it is easy to get wrong.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::Element;
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this test composes repr-builders directly
use riode::reactive::{Cx, ReactiveRuntime, Signal};
use riode_tiny_skia::testing::Harness;

/// One click grows the row by 20. The height lives only in this component's hook state, so it
/// is the observable: nothing outside the row can set or read it.
#[component]
fn Row(_index: usize, label: String) -> impl IntoElement {
    let height = use_signal(40.0f32);
    let grow = height.clone();
    let bound = height.clone();
    group()
        .key(format!("row-{label}"))
        .width(Size::fill())
        .height(bound)
        .fill(Color::hex(0x1D4ED8))
        .on_click(move |_| grow.set(grow.get() + 20.0))
        .children([text(label).size(12.0).color(Color::WHITE)])
}

/// A component nested inside a keyed row, to check that nesting does not lose the identity.
#[component]
fn Leaf(label: String) -> impl IntoElement {
    let size = use_signal(10.0f32);
    let grow = size.clone();
    let bound = size.clone();
    group()
        .key(format!("leaf-{label}"))
        .width(Size::fill())
        .height(bound)
        .fill(Color::hex(0x22C55E))
        .on_click(move |_| grow.set(grow.get() + 10.0))
        .children([text(label).size(10.0).color(Color::WHITE)])
}

/// A keyed row whose child is itself a component with state.
#[component]
fn OuterRow(_index: usize, label: String) -> impl IntoElement {
    group()
        .key(format!("row-{label}"))
        .width(Size::fill())
        .direction(Direction::Vertical)
        .fill(Color::hex(0x1D4ED8))
        .children([Leaf(label.clone())])
}

fn list(labels: Signal<Vec<String>>) -> Element {
    group()
        .width(Size::fill())
        .height(Size::fill())
        .direction(Direction::Vertical)
        .children_labels_keyed(labels, |s: &String| s.clone(), Row)
        .into_element()
}

/// An unkeyed list, to pin the difference in contract.
fn nested_list(labels: Signal<Vec<String>>) -> Element {
    group()
        .width(Size::fill())
        .height(Size::fill())
        .direction(Direction::Vertical)
        .children_labels_keyed(labels, |s: &String| s.clone(), OuterRow)
        .into_element()
}

fn unkeyed_list(labels: Signal<Vec<String>>) -> Element {
    group()
        .width(Size::fill())
        .height(Size::fill())
        .direction(Direction::Vertical)
        .children_labels(labels, Row)
        .into_element()
}

fn mount(root: impl FnOnce() -> Element, rt: Rc<RefCell<ReactiveRuntime>>) -> Harness {
    let mut h = Harness::from_runtime(Rc::clone(&rt), Cx::new(rt).enter(root), 300, 400);
    h.steps(4);
    h
}

fn labels_signal(rt: &Rc<RefCell<ReactiveRuntime>>, items: &[&str]) -> Signal<Vec<String>> {
    Signal::new(
        Rc::clone(rt),
        items.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
}

fn height_of(h: &mut Harness, label: &str) -> f32 {
    h.located(&format!("row-{label}")).flow.height
}

fn tap(h: &mut Harness, label: &str, times: u32) {
    for _ in 0..times {
        h.click(&format!("row-{label}"));
        h.steps(3);
    }
}

#[test]
#[ntest::timeout(5_000)]
fn a_keyed_row_keeps_its_state_when_the_list_rebuilds() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let labels = labels_signal(&rt, &["a", "b"]);
    let mut h = mount(|| list(labels.clone()), rt);

    assert!(
        (height_of(&mut h, "a") - 40.0).abs() < 0.5,
        "rows start at their base height"
    );
    tap(&mut h, "a", 2);
    let grown = height_of(&mut h, "a");
    assert!(
        (grown - 80.0).abs() < 0.5,
        "the row's own state must drive its height, {grown}"
    );

    // Appending rebuilds the list: every row component is constructed again.
    let mut next = labels.get();
    next.push("c".to_string());
    labels.set(next);
    h.steps(8);

    assert!(
        (height_of(&mut h, "a") - grown).abs() < 0.5,
        "row-a lost its click count when the list rebuilt: {grown} -> {}",
        height_of(&mut h, "a")
    );
}

#[test]
#[ntest::timeout(5_000)]
fn after_a_reorder_each_row_keeps_its_own_state() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let labels = labels_signal(&rt, &["a", "b", "c"]);
    let mut h = mount(|| list(labels.clone()), rt);

    tap(&mut h, "a", 2);
    tap(&mut h, "b", 1);

    labels.set(vec!["b".to_string(), "c".to_string(), "a".to_string()]);
    h.steps(8);

    // Keyed by identity, so "a" keeps "a"'s history even though it now sits at the bottom.
    // Keyed by position instead, "a" would come back at 40 and "b" would wear "a"'s 80 --
    // state that is wrong rather than absent, which is the worse failure.
    assert!(
        (height_of(&mut h, "a") - 80.0).abs() < 0.5,
        "row-a should have kept its own two taps, it is at {}",
        height_of(&mut h, "a")
    );
    assert!(
        (height_of(&mut h, "b") - 60.0).abs() < 0.5,
        "row-b should have kept its own single tap, it is at {}",
        height_of(&mut h, "b")
    );
    assert!(
        (height_of(&mut h, "c") - 40.0).abs() < 0.5,
        "an untouched row should be untouched, it is at {}",
        height_of(&mut h, "c")
    );
}

#[test]
#[ntest::timeout(5_000)]
fn removing_and_re_adding_an_item_starts_fresh() {
    // React semantics: a keyed item that leaves the list unmounts, so its state goes with it.
    // Re-adding the same key mounts a *new* instance at the base value — state that should
    // survive a round trip out of the list belongs in props, not in the row's hook memory.
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let labels = labels_signal(&rt, &["a", "b"]);
    let mut h = mount(|| list(labels.clone()), rt);

    tap(&mut h, "a", 3);
    let grown = height_of(&mut h, "a");
    assert!(
        (grown - 100.0).abs() < 0.5,
        "three taps must grow the row before it leaves, it is at {grown}"
    );

    labels.set(vec!["b".to_string()]);
    h.steps(8);
    assert!(
        h.at("row-a").is_none(),
        "removing the item must remove its row"
    );

    labels.set(vec!["b".to_string(), "a".to_string()]);
    h.steps(8);
    assert!(
        (height_of(&mut h, "a") - 40.0).abs() < 0.5,
        "a re-added item must mount fresh at its base height, not resurrect the old state: {}",
        height_of(&mut h, "a")
    );
}

#[test]
#[ntest::timeout(5_000)]
fn an_unkeyed_list_starts_each_rebuild_from_scratch() {
    // Pinned deliberately. `children_labels` cannot preserve hook state, because nothing
    // tells the allocator which rebuilt row is which instance -- and guessing from position
    // would mis-attribute state on reorder. This test documents that callers who need state
    // must use `children_labels_keyed`, rather than leaving it to be discovered.
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let labels = labels_signal(&rt, &["a", "b"]);
    let mut h = mount(|| unkeyed_list(labels.clone()), rt);

    tap(&mut h, "a", 2);
    assert!((height_of(&mut h, "a") - 80.0).abs() < 0.5);

    labels.set(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    h.steps(8);

    assert!(
        (height_of(&mut h, "a") - 40.0).abs() < 0.5,
        "an unkeyed row is rebuilt fresh, it is at {}",
        height_of(&mut h, "a")
    );
}

#[test]
#[ntest::timeout(5_000)]
fn two_lists_keeping_separate_state_do_not_share_it() {
    // Identity is per list instance, so the same label in two lists must not collide.
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let left = labels_signal(&rt, &["a"]);
    let right = labels_signal(&rt, &["a"]);
    let left_c = left.clone();
    let right_c = right.clone();
    let mut h = mount(
        move || {
            group()
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Vertical)
                .children([
                    group()
                        .key("left")
                        .direction(Direction::Vertical)
                        .children_labels_keyed(
                            left_c.clone(),
                            |s: &String| format!("left/{s}"),
                            Row,
                        ),
                    group()
                        .key("right")
                        .direction(Direction::Vertical)
                        .children_labels_keyed(
                            right_c.clone(),
                            |s: &String| format!("right/{s}"),
                            Row,
                        ),
                ])
                .into_element()
        },
        Rc::clone(&rt),
    );

    tap(&mut h, "a", 1);
    assert!((height_of(&mut h, "a") - 60.0).abs() < 0.5);

    let mut next = right.get();
    next.push("z".to_string());
    right.set(next);
    h.steps(8);

    // Rebuilding the right list must not disturb the left one, even though both hold a
    // row-a. Keys are namespaced per list for exactly this reason.
    assert!(
        (height_of(&mut h, "a") - 60.0).abs() < 0.5,
        "rebuilding one list disturbed the other: row-a is at {}",
        height_of(&mut h, "a")
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_component_nested_inside_a_keyed_row_keeps_its_state_too() {
    // The nested component never learns its own key, so its identity has to be derived from
    // the keyed row containing it. If it were not, a rebuild would silently reset the leaf
    // while leaving the row around it looking perfectly healthy.
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let labels = labels_signal(&rt, &["a", "b"]);
    let mut h = mount(|| nested_list(labels.clone()), rt);

    let leaf = |h: &mut Harness| h.located("leaf-a").flow.height;
    for _ in 0..2 {
        h.click("leaf-a");
        h.steps(3);
    }
    let grown = leaf(&mut h);
    assert!(
        (grown - 30.0).abs() < 0.5,
        "the leaf must grow when clicked, it is at {grown}"
    );

    let mut next = labels.get();
    next.push("c".to_string());
    labels.set(next);
    h.steps(8);

    assert!(
        (leaf(&mut h) - grown).abs() < 0.5,
        "the nested component lost its state on rebuild: {grown} -> {}",
        leaf(&mut h)
    );
}
