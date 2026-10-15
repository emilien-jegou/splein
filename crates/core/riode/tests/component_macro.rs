// Single responsibility: The #[component] attribute: hook scope, arguments, and one-shot bodies.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{group, text, Component, IntoElement};
use riode::foundation::{Color, Size};
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this test composes repr-builders directly
use riode::reactive::{ReactiveRuntime, Signal};
use riode::runtime::Engine;

/// A component with no arguments that allocates its own state.
#[component]
fn Badge() -> impl IntoElement {
    let pressed = use_signal(false);
    group()
        .key("badge")
        .width(60.0)
        .height(30.0)
        .fill(Color::BLACK)
        .scale(pressed.map(|p| if p { 0.8 } else { 1.0 }))
        .on_click(move |_| pressed.set(!pressed.get()))
}

/// A component taking typed arguments and returning an opaque `impl IntoElement`.
#[component]
fn Row(label: &'static str, width: f32) -> impl IntoElement {
    group()
        .key(format!("row-{label}"))
        .direction(Direction::Horizontal)
        .width(Size::fill())
        .height(40.0)
        .children([
            group()
                .width(width)
                .height(40.0)
                .fill(Color::hex(0x808080))
                .into_element(),
            text(label).size(14.0).into_element(),
        ])
}

/// A component whose body drives a spring through a hook, with no engine involved.
#[component]
fn Springy(open: Signal<bool>) -> impl IntoElement {
    let target = open.map(|is_open| if is_open { 120.0 } else { 0.0 });
    let x = Transition::spring(Spring::snappy()).on(target);
    group()
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .children([group()
            .key("card")
            .width(60.0)
            .height(40.0)
            .fill(Color::BLACK)
            .x(x)
            .into_element()])
}

/// A generic component, to prove the macro preserves type parameters.
///
/// Elements are move-only by design, so this takes ownership of one item rather than
/// cloning it: the tree is built by value and reconciled by key, never duplicated.
#[component]
fn Wrap<T: IntoElement + 'static>(item: T, gap: f32) -> impl IntoElement {
    group()
        .direction(Direction::Vertical)
        .gap(gap)
        .children([item])
}

/// A component that builds many keyed children from a list.
#[component]
fn Labels(items: Vec<String>) -> impl IntoElement {
    group().direction(Direction::Vertical).children(
        items
            .into_iter()
            .map(|label| {
                group()
                    .key(format!("label-{label}"))
                    .width(Size::fill())
                    .height(20.0)
                    .fill(Color::BLACK)
                    .into_element()
            })
            .collect::<Vec<_>>(),
    )
}

fn press(engine: &mut Engine, x: f32, y: f32) {
    engine.queue_pointer_down(x, y);
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.queue_pointer_up(x, y);
    engine.advance(1.0 / 60.0);
    engine.frame();
}

#[test]
#[ntest::timeout(5_000)]
fn a_component_can_allocate_its_own_state_with_argument_free_hooks() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(Badge);

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();

    let scale_of = |engine: &Engine| engine.transform("badge").map_or(1.0, |t| t.a);
    assert!((scale_of(&engine) - 1.0).abs() < 0.01, "starts unpressed");

    press(&mut engine, 30.0, 15.0);
    for _ in 0..60 {
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
    assert!(
        (scale_of(&engine) - 0.8).abs() < 0.05,
        "the component's own signal must drive its node, scale is {}",
        scale_of(&engine)
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_component_forwards_its_arguments() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx
        .enter(|| {
            group()
                .width(400.0)
                .height(200.0)
                .children([Row("alpha", 120.0)])
        })
        .into_element();

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.advance(1.0 / 60.0);
    engine.frame();

    assert!(
        engine.rect("row-alpha").is_some(),
        "the component must build its keyed subtree from the arguments it was given"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_component_body_runs_exactly_once_per_mount() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(Badge);

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    // Many frames with no structural change must not rebuild the component, or its signal
    // would be reallocated and the interaction state would reset every frame.
    for _ in 0..30 {
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
    let before = engine
        .scene()
        .chunks
        .iter()
        .map(|c| c.commands.len())
        .sum::<usize>();

    for _ in 0..30 {
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
    let after = engine
        .scene()
        .chunks
        .iter()
        .map(|c| c.commands.len())
        .sum::<usize>();
    assert_eq!(
        before, after,
        "a static component's display list must be stable across idle frames"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_component_can_drive_a_spring_with_no_engine_handle() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), false);
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| Springy(open.clone()));

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();
    let resting = engine.transform("card").map_or(0.0, |t| t.tx);
    assert!(resting.abs() < 1.0, "starts closed, at {resting}");

    open.set(true);
    for _ in 0..90 {
        engine.advance(1.0 / 60.0);
        engine.frame();
    }
    let settled = engine.transform("card").map_or(0.0, |t| t.tx);
    assert!(
        (settled - 120.0).abs() < 2.0,
        "the component's spring must settle on its target, at {settled}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_generic_component_keeps_its_parameters() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx
        .enter(|| {
            group()
                .width(400.0)
                .height(400.0)
                .fill(Color::WHITE)
                .children([Wrap(
                    group().width(100.0).height(20.0).fill(Color::BLACK),
                    4.0,
                )])
        })
        .into_element();

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();

    let painted = engine
        .scene()
        .chunks
        .iter()
        .flat_map(|c| c.commands.iter())
        .filter(|c| matches!(c, riode::scene::SceneCommand::DrawRect { .. }))
        .count();
    assert!(
        painted >= 2,
        "a generic component must expand its type parameter, painted {painted} rects"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_component_builds_a_keyed_child_per_list_item() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| {
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([Labels(vec![
                "one".to_string(),
                "two".to_string(),
                "three".to_string(),
            ])])
            .into_element()
    });

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();

    for label in ["one", "two", "three"] {
        assert!(
            engine.rect(format!("label-{label}")).is_some(),
            "the component must build a keyed child for {label}"
        );
    }
}

#[test]
#[ntest::timeout(5_000)]
#[should_panic(expected = "hook called outside a component body")]
fn a_component_built_outside_a_scope_fails_loudly() {
    // No ambient Cx: silently allocating onto a throwaway graph would look like it worked
    // while driving nothing.
    let _ = Badge();
}

#[test]
#[ntest::timeout(5_000)]
fn a_component_can_nest_inside_another_components_scope() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx
        .enter(|| {
            group()
                .width(400.0)
                .height(400.0)
                .fill(Color::WHITE)
                .children([Row("nested", 50.0), Badge()])
        })
        .into_element();

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(
        engine.rect("row-nested").is_some(),
        "outer child must exist"
    );
    press(&mut engine, 30.0, 55.0);
}

/// A component taking a whole element as a prop, for nested-RSX attributes.
#[component]
fn Headed(head: Element) -> impl IntoElement {
    view! {
        <Group direction="vertical" width={400.0} height={400.0}>
            <Group direction="horizontal" width="fill" height={40.0}>
                {head}
            </Group>
        </Group>
    }
}

#[test]
#[ntest::timeout(5_000)]
fn an_attribute_holds_a_nested_element() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| {
        view! {
            <Headed head={<Row label="nested-attr" width={50.0} />} />
        }
    });

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(
        engine.rect("row-nested-attr").is_some(),
        "a nested element in attribute position must build and mount"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn the_hand_written_component_trait_still_works() {
    struct Panel;
    impl Component for Panel {
        fn render(self) -> riode::dsl::Element {
            group().width(10.0).height(10.0).into_element()
        }
    }
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut engine =
        Engine::with_runtime(Rc::clone(&runtime), riode::dsl::component(Panel), 100, 100);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(engine.logical_size(), (100, 100));
}

/// A `<for>` list: one keyed row per item, state following identity across reorder.
#[component]
fn ForList(items: Signal<Vec<String>>) -> impl IntoElement {
    view! {
        <Group direction="vertical" width={400.0} height={400.0}>
            <for (_i, s) in items key={.clone()}>
                <Group key={s.clone()} width={400.0} height={20.0} />
            </for>
        </Group>
    }
}

#[test]
#[ntest::timeout(5_000)]
fn for_node_builds_a_keyed_row_per_item() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let items: Signal<Vec<String>> = Cx::new(Rc::clone(&runtime))
        .enter(|| use_signal(vec!["one".to_string(), "two".to_string()]));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| ForList(items.clone()));

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();

    for key in ["one", "two"] {
        assert!(
            engine.rect(key).is_some(),
            "<for> must build a keyed row for {key}"
        );
    }

    // Reorder: rows follow identity, not position.
    items.set(vec!["two".to_string(), "one".to_string()]);
    engine.advance(1.0 / 60.0);
    engine.frame();
    for key in ["one", "two"] {
        assert!(
            engine.rect(key).is_some(),
            "<for> must keep row {key} across reorder"
        );
    }
}

/// Static siblings around a `<for>`: order interleaves, dynamics stay live.
#[component]
fn MixedList(items: Signal<Vec<String>>) -> impl IntoElement {
    view! {
        <Group direction="vertical" width={400.0} height={400.0}>
            <Group key="head" width={400.0} height={20.0} />
            <for (_i, s) in items key={.clone()}>
                <Group key={s.clone()} width={400.0} height={20.0} />
            </for>
            <Group key="foot" width={400.0} height={20.0} />
        </Group>
    }
}

#[test]
#[ntest::timeout(5_000)]
fn static_and_dynamic_siblings_interleave_in_order() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let items: Signal<Vec<String>> = Cx::new(Rc::clone(&runtime))
        .enter(|| use_signal(vec!["one".to_string(), "two".to_string()]));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| MixedList(items.clone()));

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();

    let y_of = |engine: &Engine, key: &str| engine.rect(key).map(|r| r.y);
    let (head, one, foot) = (
        y_of(&engine, "head"),
        y_of(&engine, "one"),
        y_of(&engine, "foot"),
    );
    assert!(
        matches!((head, one, foot), (Some(h), Some(o), Some(f)) if h < o && o < f),
        "static head, dynamic row, static foot must stack in order, got {head:?} {one:?} {foot:?}"
    );

    // Reordering the dynamic segment keeps static siblings put.
    items.set(vec!["two".to_string(), "one".to_string()]);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(
        y_of(&engine, "head"),
        head,
        "static head must not move on reorder"
    );
    assert_eq!(
        y_of(&engine, "foot"),
        foot,
        "static foot must not move on reorder"
    );
    let (two, one) = (y_of(&engine, "two"), y_of(&engine, "one"));
    assert!(
        matches!((two, one), (Some(t), Some(o)) if t < o),
        "rows must swap order, got two={two:?} one={one:?}"
    );
}

/// Control flow nested in control flow: an `<if>` inside each `<for>` row.
#[component]
fn NestedBranch(items: Signal<Vec<String>>, show: Signal<bool>) -> impl IntoElement {
    view! {
        <Group direction="vertical" width={400.0} height={400.0}>
            <for (i, s) in items key={.clone()}>
                <Group key={s.clone()} width={400.0} height={20.0}>
                    <if show.get()>
                        <Group key={format!("tag-{s}")} width={20.0} height={20.0} />
                    </if>
                </Group>
            </for>
        </Group>
    }
}

#[test]
#[ntest::timeout(5_000)]
fn control_nodes_nest_without_carriers() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let items: Signal<Vec<String>> =
        Cx::new(Rc::clone(&runtime)).enter(|| use_signal(vec!["one".to_string()]));
    let show: Signal<bool> = Cx::new(Rc::clone(&runtime)).enter(|| use_signal(false));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| NestedBranch(items.clone(), show.clone()));

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(engine.rect("one").is_some(), "row must mount");
    assert!(
        engine.rect("tag-one").is_none(),
        "nested branch starts hidden"
    );

    show.set(true);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(
        engine.rect("tag-one").is_some(),
        "nested branch must appear per row"
    );
}

/// An `<if>` branch: swaps children when the signal flips, remounting.
#[component]
fn IfBranch(open: Signal<bool>) -> impl IntoElement {
    view! {
        <Group direction="vertical" width={400.0} height={400.0}>
            <if open.get()>
                <Group key="open-branch" width={400.0} height={20.0} />
                <else>
                    <Group key="closed-branch" width={400.0} height={20.0} />
                </else>
            </if>
        </Group>
    }
}

#[test]
#[ntest::timeout(5_000)]
fn if_node_swaps_branches_on_signal_change() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open: Signal<bool> = Cx::new(Rc::clone(&runtime)).enter(|| use_signal(false));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| IfBranch(open.clone()));

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(
        engine.rect("closed-branch").is_some(),
        "<if> must render the else arm"
    );
    assert!(
        engine.rect("open-branch").is_none(),
        "<if> must not render the then arm"
    );

    open.set(true);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(
        engine.rect("open-branch").is_some(),
        "<if> must swap to the then arm"
    );
    assert!(
        engine.rect("closed-branch").is_none(),
        "<if> must unmount the else arm"
    );
}

#[derive(Clone, PartialEq)]
enum LoadState {
    Loading,
    Ready(String),
    Failed,
}

/// A `<match>` branch: patterns bind, guards filter, `_` catches the rest.
#[component]
fn MatchBranch(state: Signal<LoadState>) -> impl IntoElement {
    view! {
        <Group direction="vertical" width={400.0} height={400.0}>
            <match state.get()>
                <case LoadState::Loading>
                    <Group key="loading" width={400.0} height={20.0} />
                </case>
                <case LoadState::Ready(title) if title.len() > 3>
                    <Group key="ready-long" width={400.0} height={20.0} />
                </case>
                <case LoadState::Ready(_)>
                    <Group key="ready-short" width={400.0} height={20.0} />
                </case>
                <case _>
                    <Group key="other" width={400.0} height={20.0} />
                </case>
            </match>
        </Group>
    }
}

#[test]
#[ntest::timeout(5_000)]
fn match_node_dispatches_arms_guards_and_wildcard() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let state: Signal<LoadState> =
        Cx::new(Rc::clone(&runtime)).enter(|| use_signal(LoadState::Loading));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: riode::dsl::Element = cx.enter(|| MatchBranch(state.clone()));

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 400, 400);
    let shown = |engine: &Engine| {
        for key in ["loading", "ready-long", "ready-short", "other"] {
            if engine.rect(key).is_some() {
                return key;
            }
        }
        "none"
    };
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(shown(&engine), "loading");

    state.set(LoadState::Ready("hi".to_string()));
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(
        shown(&engine),
        "ready-short",
        "guard must reject short titles"
    );

    state.set(LoadState::Ready("hello".to_string()));
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(shown(&engine), "ready-long", "guard must pass long titles");

    state.set(LoadState::Failed);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(shown(&engine), "other", "wildcard must catch the rest");
}
