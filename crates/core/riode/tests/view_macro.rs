// Single responsibility: view! macro codegen — primitives, components, props inference.
use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::IntoElement;
use riode::foundation::{Color, Direction};
use riode::prelude::*;
use riode::reactive::ReactiveRuntime;
use riode::runtime::Engine;

#[component]
fn Card(title: Prop<String>, subtitle: Option<String>, children: Vec<Element>) -> impl IntoElement {
    // `subtitle` is intentionally unused: it exists to prove an omissible
    // `Option` prop compiles away when the RSX omits it.
    let _ = subtitle.as_ref();
    view! {
        <Group direction={Direction::Vertical} width={200.0} height={100.0} fill={Color::WHITE}>
            <Text size={16.0}>{title}</Text>
            {children}
        </Group>
    }
}

fn harness(root: riode::dsl::Element) -> Engine {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    // NOTE: components need the ambient scope; build inside one.
    let cx = Cx::new(Rc::clone(&runtime));
    let _ = &cx;
    Engine::with_runtime(Rc::clone(&runtime), root, 400, 400)
}

#[test]
#[ntest::timeout(5_000)]
fn view_builds_primitive_tree() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let el: riode::dsl::Element = cx.enter(|| {
        view! {
            <Group width={200.0} height={100.0} fill={Color::WHITE}>
                <Text size={14.0}>"hello"</Text>
            </Group>
        }
    });
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), el, 400, 400);
    engine.advance(1.0 / 60.0);
    engine.frame();
    assert!(engine.rect("").is_none()); // smoke: it mounted without panic
    let _ = harness;
}

#[test]
#[ntest::timeout(5_000)]
fn view_accepts_literal_inference() {
    use riode::anim::Transition;
    // Strings, hex numbers, and int literals flow through IntoProp/Into.
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let el: riode::dsl::Element = cx.enter(|| {
        view! {
            <Group width="fill" height={100} fill="#121212" radius="max" direction="vertical" margin="8 16">
                <Text size={12.0} color="rgb(255,255,255)">"lit"</Text>
                <Group width={200} height={50} fill={0xFF0000} />
            </Group>
        }
    });
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), el, 400, 400);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.advance(1.0 / 60.0);
    engine.frame();

    // Transition literals parse without a builder in sight.
    assert!(Transition::parse("spring").is_some());
    assert!(Transition::parse("spring:bouncy").is_some());
    assert!(Transition::parse("ease-out 300ms").is_some());
    assert!(Transition::parse("0.3s linear").is_some());
    assert!(Transition::parse("nonsense").is_none());
}

#[test]
#[ntest::timeout(5_000)]
fn view_builds_component_with_optional_prop() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let el: riode::dsl::Element = cx.enter(|| {
        view! {
            <Card title="Hi">
                <Text size={12.0}>"body"</Text>
            </Card>
        }
    });
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), el, 400, 400);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.advance(1.0 / 60.0);
    engine.frame();
    // The static "Hi" title must have shaped (nonzero text commands).
    let texts = engine
        .scene()
        .chunks
        .iter()
        .flat_map(|c| c.commands.iter())
        .filter(|c| matches!(c, riode::scene::SceneCommand::DrawText { .. }))
        .count();
    assert!(texts >= 2, "title + body text must paint, got {texts}");
}
