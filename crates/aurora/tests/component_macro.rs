// Single responsibility: The #[component] attribute: hook scope, arguments, and one-shot bodies.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::{group, text, Component, IntoElement};
use aurora::foundation::{Color, Size};
use aurora::prelude::*;
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::Engine;

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
#[ntest::timeout(30_000)]
fn a_component_can_allocate_its_own_state_with_argument_free_hooks() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: aurora::dsl::Element = cx.enter(Badge);

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
#[ntest::timeout(30_000)]
fn a_component_forwards_its_arguments() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: aurora::dsl::Element = cx
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
#[ntest::timeout(30_000)]
fn a_component_body_runs_exactly_once_per_mount() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: aurora::dsl::Element = cx.enter(Badge);

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
#[ntest::timeout(30_000)]
fn a_component_can_drive_a_spring_with_no_engine_handle() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), false);
    let cx = Cx::new(Rc::clone(&runtime));
    let view: aurora::dsl::Element = cx.enter(|| Springy(open.clone()));

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
#[ntest::timeout(30_000)]
fn a_generic_component_keeps_its_parameters() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: aurora::dsl::Element = cx
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
        .filter(|c| matches!(c, aurora::scene::SceneCommand::DrawRect { .. }))
        .count();
    assert!(
        painted >= 2,
        "a generic component must expand its type parameter, painted {painted} rects"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_component_builds_a_keyed_child_per_list_item() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: aurora::dsl::Element = cx.enter(|| {
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
#[ntest::timeout(30_000)]
#[should_panic(expected = "hook called outside a component body")]
fn a_component_built_outside_a_scope_fails_loudly() {
    // No ambient Cx: silently allocating onto a throwaway graph would look like it worked
    // while driving nothing.
    let _ = Badge();
}

#[test]
#[ntest::timeout(30_000)]
fn a_component_can_nest_inside_another_components_scope() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: aurora::dsl::Element = cx
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

#[test]
#[ntest::timeout(30_000)]
fn the_hand_written_component_trait_still_works() {
    struct Panel;
    impl Component for Panel {
        fn render(self) -> aurora::dsl::Element {
            group().width(10.0).height(10.0).into_element()
        }
    }
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut engine =
        Engine::with_runtime(Rc::clone(&runtime), aurora::dsl::component(Panel), 100, 100);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert_eq!(engine.logical_size(), (100, 100));
}
