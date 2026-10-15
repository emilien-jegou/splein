// Single responsibility: Context provide/use behavior — resolution, shadowing,
// fallback, missing-provider placeholders, and survival across reorders.
//
// All engine behavior (no visuals asserted beyond pixel probes): the human
// judges demos; these pin the machinery underneath.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::component::ProviderProps;
use riode::initContext;
use riode::prelude::*;
use riode::reactive::{ReactiveRuntime, Signal};
use riode::runtime::Engine;
use riode_tiny_skia::testing::{Harness, Rgba};

#[derive(Clone)]
struct Theme {
    surface: Color,
    name: String,
}

static THEME: Context<Theme> = initContext!();

fn dark() -> Theme {
    Theme {
        surface: Color::hex(0x111111),
        name: "dark".to_string(),
    }
}

fn light() -> Theme {
    Theme {
        surface: Color::hex(0xEEEEEE),
        name: "light".to_string(),
    }
}

fn surface_of(theme: &Ctx<Theme>) -> Ctx<Fill> {
    theme.clone().map(|t| Fill::solid(t.surface))
}

/// Card reading fill + label from context.
#[component]
fn ThemedCard() -> impl IntoElement {
    let theme = THEME.get();
    view! {
        <Group width={100.0} height={100.0} fill={surface_of(&theme)} />
    }
}

#[component]
fn ThemedLabel() -> impl IntoElement {
    let theme = THEME.get();
    view! {
        <Group width={400.0} height={40.0}>
            <Text size={14.0} color={Color::WHITE}>{theme.map(|t| t.name.clone())}</Text>
        </Group>
    }
}

fn harness(view: Element, runtime: &Rc<RefCell<ReactiveRuntime>>) -> Harness {
    Harness::from_runtime(Rc::clone(runtime), view, 400, 400)
}

fn count_near(h: &mut Harness, color: Rgba) -> usize {
    h.view().count_matching(3, |p| p.near(color, 12))
}

fn dark_rgba() -> Rgba {
    Rgba {
        r: 0x11,
        g: 0x11,
        b: 0x11,
        a: 255,
    }
}

fn light_rgba() -> Rgba {
    Rgba {
        r: 0xEE,
        g: 0xEE,
        b: 0xEE,
        a: 255,
    }
}

#[test]
#[ntest::timeout(5_000)]
fn provided_value_reaches_consumer() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let theme: Signal<Theme> = Cx::new(Rc::clone(&runtime)).enter(|| use_signal(dark()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: Element = cx.enter(|| {
        view! {
            <Provider grant={THEME.provide(theme)} >
                <ThemedCard key="card" />
            </Provider>
        }
    });

    let mut h = harness(view, &runtime);
    h.step();
    assert!(
        h.at("card").is_some(),
        "consumer must mount under a provider"
    );
    assert!(
        count_near(&mut h, dark_rgba()) > 1000,
        "consumer must paint the provided value"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn nearest_enclosing_provider_wins() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let (outer, inner): (Signal<Theme>, Signal<Theme>) =
        cx.enter(|| (use_signal(dark()), use_signal(light())));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: Element = cx.enter(|| {
        view! {
            <Provider grant={THEME.provide(outer)} >
                <ThemedCard key="outer-card" />
                <Provider grant={THEME.provide(inner)} >
                    <ThemedCard key="inner-card" />
                </Provider>
            </Provider>
        }
    });

    let mut h = harness(view, &runtime);
    h.step();
    assert!(h.at("outer-card").is_some() && h.at("inner-card").is_some());
    assert!(
        count_near(&mut h, dark_rgba()) > 1000,
        "outer consumer must see the outer value"
    );
    assert!(
        count_near(&mut h, light_rgba()) > 1000,
        "inner consumer must see the override"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn provider_signal_updates_propagate() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let theme: Signal<Theme> = Cx::new(Rc::clone(&runtime)).enter(|| use_signal(dark()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: Element = cx.enter(|| {
        view! {
            <Provider grant={THEME.provide(theme.clone())} >
                <ThemedCard key="card" />
            </Provider>
        }
    });

    let mut h = harness(view, &runtime);
    h.step();
    assert!(count_near(&mut h, dark_rgba()) > 1000);

    theme.set(light());
    h.step();
    assert!(
        count_near(&mut h, light_rgba()) > 1000,
        "provider writes must repaint consumers"
    );
    assert_eq!(count_near(&mut h, dark_rgba()), 0, "old value must be gone");
}

#[test]
#[ntest::timeout(5_000)]
fn missing_provider_mounts_placeholder_and_survives() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: Element = cx.enter(|| {
        view! {
            <Group direction="vertical" width={400.0} height={400.0}>
                <ThemedCard key="orphan" />
                <Group key="sibling" width={100.0} height={100.0} fill={Color::WHITE} />
            </Group>
        }
    });

    let mut h = harness(view, &runtime);
    h.step();
    // The placeholder keeps the requested key binding (bound pre-evaluation),
    // so absence is geometric, not nominal: zero size where a card would be.
    let orphan = h.at("orphan").expect("placeholder keeps the key binding");
    assert!(
        orphan.flow.width == 0.0 && orphan.flow.height == 0.0,
        "unprovided consumer must mount empty, got {:?}",
        orphan.flow
    );
    assert!(
        h.at("sibling").is_some(),
        "the rest of the tree must survive a missing provider"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn fallback_renders_without_provider() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: Element = cx.enter(|| {
        let theme = THEME.get_or(dark());
        view! {
            <Group key="fb" width={100.0} height={100.0} fill={surface_of(&theme)} />
        }
    });

    let mut h = harness(view, &runtime);
    h.step();
    assert!(h.at("fb").is_some(), "fallback consumer must mount");
    assert!(
        count_near(&mut h, dark_rgba()) > 1000,
        "fallback value must paint"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn text_content_reads_context() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let theme: Signal<Theme> = Cx::new(Rc::clone(&runtime)).enter(|| use_signal(dark()));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: Element = cx.enter(|| {
        view! {
            <Provider grant={THEME.provide(theme)} >
                <ThemedLabel />
            </Provider>
        }
    });

    let mut h = harness(view, &runtime);
    h.step();
    // White 14px "dark" copy paints non-background pixels; absence would mean
    // the text path never resolved the marker.
    assert!(
        count_near(
            &mut h,
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            }
        ) > 20,
        "themed label must render its text"
    );
}

/// Rows resolving context in their own keyed scopes (the `RowSpec` case):
/// lookups must keep working across rebuilds, not just at mount.
#[component]
fn ThemedRow(label: String) -> impl IntoElement {
    let theme = THEME.get();
    view! {
        <Group
            key={format!("trow-{label}")}
            width={400.0}
            height={20.0}
            fill={surface_of(&theme)}
        />
    }
}

#[component]
fn ThemedList(items: Signal<Vec<String>>) -> impl IntoElement {
    view! {
        <Group direction="vertical" width={400.0} height={400.0}>
            <for (i, s) in items key={.clone()}>
                <ThemedRow label={s} />
            </for>
        </Group>
    }
}

#[test]
#[ntest::timeout(5_000)]
fn context_survives_list_rebuilds() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let theme: Signal<Theme> = Cx::new(Rc::clone(&runtime)).enter(|| use_signal(dark()));
    let items: Signal<Vec<String>> = Cx::new(Rc::clone(&runtime))
        .enter(|| use_signal(vec!["one".to_string(), "two".to_string()]));
    let cx = Cx::new(Rc::clone(&runtime));
    let view: Element = cx.enter(|| {
        view! {
            <Provider grant={THEME.provide(theme)} >
                <ThemedList items={items.clone()} />
            </Provider>
        }
    });

    let mut h = harness(view, &runtime);
    h.step();
    let painted = count_near(&mut h, dark_rgba());
    assert!(painted > 1000, "themed rows must paint, got {painted}");

    items.set(vec!["two".to_string(), "one".to_string()]);
    h.step();
    assert!(h.at("trow-one").is_some() && h.at("trow-two").is_some());
    assert!(
        count_near(&mut h, dark_rgba()) > 1000,
        "rows must stay themed across reorder (no stale/missing resolution)"
    );
}
