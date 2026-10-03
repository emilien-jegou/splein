// Single responsibility: The surface `docs/transitions.md` documents exists and compiles.
//
// Documentation drifts silently: a renamed variant or a changed signature leaves prose that reads
// perfectly and compiles nowhere. This file exists so the two cannot diverge — every API the guide
// names is exercised here, and if one moves or disappears this fails to build.
//
// The assertions are deliberately shallow. This is not testing behaviour, which the behavioural
// suites already cover; it is checking that the call shapes in the guide are real.

use aurora::dsl::{use_flip, use_follow, use_ref, ElementRef, Trigger};
use aurora::prelude::*;
use aurora::reactive::hooks::use_transition;
use aurora::testing::Harness;

// ---------------------------------------------------------------- section 1

#[component]
fn DocumentedBasicTransition() -> impl IntoElement {
    let on = use_signal(false);

    let progress =
        Transition::ease(180.ms(), Ease::OutCubic)
            .on(on.clone().map(|on| if on { 1.0 } else { 0.0 }));

    group()
        .key("card")
        .width(Size::fill())
        .height(Size::fill())
        .opacity(progress.clone())
        .children([text(progress.map(|p| format!("{:.0}%", p * 100.0)))])
        .into_element()
}

#[test]
fn section_1_transition_on_a_signal() {
    let mut h = Harness::page(DocumentedBasicTransition, 200, 120);
    h.step();
    assert!(
        h.located("card").flow.width > 0.0,
        "the keyed card must be laid out"
    );
}

// The modifier chain the guide shows.
#[test]
fn section_1_modifiers_exist() {
    let t = Transition::spring(Spring::snappy())
        .from(0.0)
        .to(1.0)
        .repeat(Repeat::PingPong);
    let mut h = Harness::page(DocumentedBasicTransition, 200, 120);
    let (_out, handle) = aurora::reactive::Cx::new(h.runtime()).playback(t, 0.0, 1.0);
    handle.pause();
    handle.resume();
    handle.restart();
    handle.seek(0.5);
    let _state = handle.state();
    let _out = handle.output();
}

// ---------------------------------------------------------------- section 2

#[component]
fn DocumentedAnimatedBox() -> impl IntoElement {
    let expanded = use_signal(false);

    // The whole point of the guide: one value, read by both the box and its content.
    let width =
        Transition::spring(Spring::snappy())
            .on(expanded.clone().map(|e| if e { 196.0 } else { 52.0 }));

    group()
        .width(Size::fill())
        .height(Size::fill())
        .fill(Color::WHITE)
        .children([group()
            .key("action")
            .width(width.clone())
            .height(52.0)
            .fill(Color::hex(0x4F46E5))
            .on_click(move |_| expanded.set(!expanded.get()))
            .children([text(width.map(|w| {
                if w > 124.0 {
                    String::from("Validate")
                } else {
                    String::from("Go")
                }
            }))])])
        .into_element()
}

#[test]
fn section_2_the_box_and_its_label_come_from_one_value() {
    let mut h = Harness::page(DocumentedAnimatedBox, 300, 120);
    h.step();
    let collapsed = h.located("action").flow.width;
    assert!(collapsed < 100.0, "collapsed to {collapsed}");

    h.click("action");
    assert!(h.settle(300), "the width must settle");
    let expanded = h.located("action").flow.width;
    assert!(expanded > 150.0, "expanded to {expanded}");
}

// ---------------------------------------------------------------- section 4

#[component]
fn DocumentedElementRef() -> impl IntoElement {
    let me: ElementRef = use_ref();
    group().key("card").bind_ref(me).into_element()
}

#[test]
fn section_4_reading_geometry() {
    let rt = std::rc::Rc::new(std::cell::RefCell::new(
        aurora::reactive::ReactiveRuntime::new(),
    ));
    let card = ElementRef::new(std::rc::Rc::clone(&rt));
    let tree = aurora::reactive::Cx::new(std::rc::Rc::clone(&rt)).enter(|| {
        group()
            .width(400.0)
            .height(300.0)
            .children([group()
                .key("card")
                .width(120.0)
                .height(60.0)
                .bind_ref(card.clone())])
            .into_element()
    });
    let mut h = Harness::from_runtime(std::rc::Rc::clone(&rt), tree, 400, 300);
    h.step();

    assert_eq!(card.rect().map(|r| r.width), Some(120.0));
    assert!(card.position().is_some());
    assert!(card.center().is_some());
    assert_eq!(card.size().map(|p| p.x), Some(120.0));
    assert_eq!(card.child_rects().len(), 0);
    assert!(card.is_bound());
}

// ---------------------------------------------------------------- sections 5 & 6

#[component]
fn DocumentedRow(spec: RowSpec) -> impl IntoElement {
    let row_ref = use_ref();

    let flip = use_flip(
        row_ref.clone(),
        spec.revision.map(Trigger::from),
        Spring::snappy(),
    );
    let follow = use_follow(row_ref.clone(), spec.want_centre, spec.grab_dy);

    // The held-row rule from the guide: gate on the hold, not on a non-zero offset.
    let is_held = spec.held.map({
        let label = spec.label.clone();
        move |held| held.as_deref() == Some(label.as_str())
    });
    let flip = flip.combine(
        is_held,
        |flip, held| {
            if held {
                Transform::IDENTITY
            } else {
                flip
            }
        },
    );

    let offset = flip.combine(follow, |a, b| {
        Transform::from_translation(a.tx + b.tx, a.ty + b.ty)
    });

    group()
        .key(&format!("row-{}", spec.label))
        .width(200.0)
        .height(40.0)
        .bind_ref(row_ref)
        .transform(offset)
        .into_element()
}

/// The props the guide's FLIP example threads through.
#[derive(Clone)]
struct RowSpec {
    label: String,
    revision: Signal<u32>,
    held: Signal<Option<String>>,
    want_centre: Signal<Option<f32>>,
    grab_dy: Signal<f32>,
}

#[test]
fn sections_5_and_6_flip_and_follow_compose() {
    let rt = std::rc::Rc::new(std::cell::RefCell::new(
        aurora::reactive::ReactiveRuntime::new(),
    ));
    let spec = RowSpec {
        label: "a".into(),
        revision: Signal::new(std::rc::Rc::clone(&rt), 0u32),
        held: Signal::new(std::rc::Rc::clone(&rt), None),
        want_centre: Signal::new(std::rc::Rc::clone(&rt), None),
        grab_dy: Signal::new(std::rc::Rc::clone(&rt), 0.0f32),
    };
    let spec2 = spec.clone();
    let tree = aurora::reactive::Cx::new(std::rc::Rc::clone(&rt)).enter(|| {
        group()
            .width(400.0)
            .height(400.0)
            .children([DocumentedRow(spec2)])
            .into_element()
    });
    let mut h = Harness::from_runtime(std::rc::Rc::clone(&rt), tree, 400, 400);
    h.step();

    // Bump the revision the way the guide says to: this is what FLIP watches.
    spec.revision.set(1);
    h.step();
    assert!(h.settle(300));
}

// ---------------------------------------------------------------- section 7

#[component]
fn DocumentedSurvivesRebuild() -> impl IntoElement {
    let order = use_signal(vec!["a".to_string(), "b".to_string()]);
    let index = order.map({
        let label = "a".to_string();
        move |o| o.iter().position(|l| *l == label).unwrap_or(0) as f32
    });
    // Recalls last time's cell, so position and velocity survive the rebuild.
    let animated = use_transition(Transition::spring(Spring::snappy()), index.clone());
    let offset = animated.combine(index, |a, i| a - i);

    group()
        .key("card")
        .width(Size::fill())
        .height(40.0)
        .transform(offset.map(|v| Transform::from_translation(0.0, v)))
        .children([text(order.map(|o| o.join(",")))])
        .into_element()
}

#[test]
fn section_7_use_transition_survives_a_rebuild() {
    let mut h = Harness::page(DocumentedSurvivesRebuild, 300, 200);
    h.step();
    assert!(h.settle(300), "must settle");
}

// ---------------------------------------------------------------- sections 10-12

#[test]
fn section_10_every_documented_easing_exists() {
    // The guide lists these in full; if one is renamed this stops compiling.
    let _ = [
        Ease::Linear,
        Ease::InCubic,
        Ease::OutCubic,
        Ease::InOutCubic,
        Ease::OutQuint,
        Ease::InOutQuint,
        Ease::OutExpo,
    ];
    let _ = [Repeat::Once, Repeat::PingPong, Repeat::Loop];
    let _ = [Spring::smooth(), Spring::snappy(), Spring::bouncy()];
}

#[test]
fn section_12_the_documented_harness_calls_exist() {
    let mut h = Harness::page(DocumentedBasicTransition, 200, 120);
    h.step();
    h.pause();
    assert!(h.is_paused(), "pause must freeze the clock");
    let before = h.frame();
    h.step_once();
    assert_eq!(
        h.frame(),
        before + 1,
        "step_once advances exactly one frame while frozen"
    );
    h.resume();
    assert!(!h.is_paused());
    assert!(h.settle(120));

    // "Measure geometry, don't import a declared constant."
    let _measured = h.located("card").flow.height;
    let _centre = h.located("card").center();
}
