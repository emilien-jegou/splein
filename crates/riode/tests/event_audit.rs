// Single responsibility: Audit of handler defaults, hit-test flags, visibility sugar,
// behavior attachment, and stop-propagation bubbling control.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{group, text, Behavior, GroupDef, IntoElement};
use riode::foundation::{Color, LayoutPresence, Margin, Size};
use riode::input::{HitTest, PointerPhase, Propagation};
use riode::reactive::{ReactiveRuntime, Signal};
use riode::runtime::Engine;
use riode_tiny_skia::HeadlessRaster;

const W: f32 = 400.0;
const H: f32 = 400.0;

fn frame(engine: &mut Engine, raster: &mut HeadlessRaster) {
    engine.advance(1.0 / 60.0);
    engine.frame();
    raster.draw_engine(&mut *engine);
}

fn send(engine: &mut Engine, raster: &mut HeadlessRaster, x: f32, y: f32, phase: PointerPhase) {
    engine.queue_pointer_event(riode::input::PointerEvent {
        position: riode::foundation::Point::new(x, y),
        button: riode::input::PointerButton::Primary,
        phase,
        click_count: 0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    });
    frame(&mut *engine, &mut *raster);
}

fn click(engine: &mut Engine, raster: &mut HeadlessRaster, x: f32, y: f32) {
    send(&mut *engine, &mut *raster, x, y, PointerPhase::Down);
    send(&mut *engine, &mut *raster, x, y, PointerPhase::Up);
}

/// Text builders declare no handlers: inert text stays free of the target search.
#[test]
#[ntest::timeout(5_000)]
fn text_nodes_carry_no_handlers() {
    let el = text("hello").into_element();
    match el {
        riode::dsl::Element::Text(_) => {}
        _ => panic!("text() must build a Text element"),
    }
    // Groups are also inert until a handler is attached.
    let bare = group().width(10.0).height(10.0);
    assert!(
        bare.handlers.is_empty(),
        "a bare group must have no handlers"
    );
    assert_eq!(bare.hit_test, HitTest::Auto);
}

/// Hit-test policy is retained on the builder for reconciliation.
#[test]
#[ntest::timeout(5_000)]
fn hit_test_policy_setter() {
    let opaque = group().hit_test(HitTest::Opaque);
    assert_eq!(opaque.hit_test, HitTest::Opaque);
    let passthrough = group().hit_test(HitTest::Passthrough);
    assert_eq!(passthrough.hit_test, HitTest::Passthrough);
}

/// `visible` sugar maps onto layout presence, static and dynamic.
#[test]
#[ntest::timeout(5_000)]
fn visible_maps_to_presence() {
    let shown = group().visible(true);
    let presence = shown.presence.expect("visible must set presence");
    assert_eq!(presence.evaluate(), LayoutPresence::Present);
    let hidden = group().visible(false);
    let presence = hidden.presence.expect("visible must set presence");
    assert_eq!(presence.evaluate(), LayoutPresence::Absent);

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let flag = Signal::new(Rc::clone(&runtime), true);
    let dyn_shown = group().visible(flag.clone());
    let presence = dyn_shown.presence.expect("visible must set presence");
    assert!(presence.is_dynamic());
    assert_eq!(presence.evaluate(), LayoutPresence::Present);
    flag.set(false);
    assert_eq!(presence.evaluate(), LayoutPresence::Absent);
}

/// A behavior owns its clones and fans out to typed slots.
struct TapCounter {
    taps: Signal<u32>,
}

impl Behavior for TapCounter {
    fn attach(self, row: GroupDef) -> GroupDef {
        let taps = self.taps.clone();
        row.on_click(move |_| taps.set(taps.get() + 1))
    }
}

#[test]
#[ntest::timeout(5_000)]
fn behavior_attach_wires_handlers() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let taps = Signal::new(Rc::clone(&runtime), 0u32);
    let row = group()
        .width(100.0)
        .height(100.0)
        .attach(TapCounter { taps: taps.clone() });
    assert!(
        !row.handlers.is_empty(),
        "attach must wire the behavior's handlers"
    );
}

/// `Stop` halts bubbling: the parent never observes the child's click.
#[test]
#[ntest::timeout(5_000)]
fn stop_halts_bubbling_to_parent() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let child_taps = Signal::new(Rc::clone(&runtime), 0u32);
    let parent_taps = Signal::new(Rc::clone(&runtime), 0u32);
    let c = child_taps.clone();
    let p = parent_taps.clone();

    let mut raster = HeadlessRaster::new();
    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .on_click(move |_| p.set(p.get() + 1))
            .children([group()
                .key("child")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_click_handled(move |_| {
                    c.set(c.get() + 1);
                    Propagation::Stop
                })
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine, &mut raster);

    click(&mut engine, &mut raster, 50.0, 50.0);
    assert_eq!(child_taps.get(), 1, "child must observe its own click");
    assert_eq!(
        parent_taps.get(),
        0,
        "Stop must prevent the parent from observing the click"
    );
}

/// Default (`Bubble`) preserves chip-and-card observation.
#[test]
#[ntest::timeout(5_000)]
fn bubble_reaches_both_child_and_parent() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let child_taps = Signal::new(Rc::clone(&runtime), 0u32);
    let parent_taps = Signal::new(Rc::clone(&runtime), 0u32);
    let c = child_taps.clone();
    let p = parent_taps.clone();

    let mut raster = HeadlessRaster::new();
    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .on_click(move |_| p.set(p.get() + 1))
            .children([group()
                .key("child")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_click(move |_| c.set(c.get() + 1))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine, &mut raster);

    click(&mut engine, &mut raster, 50.0, 50.0);
    assert_eq!(child_taps.get(), 1);
    assert_eq!(
        parent_taps.get(),
        1,
        "plain on_* handlers must keep bubbling to the parent"
    );
}

/// An Opaque overlay absorbs presses so nodes beneath never fire.
#[test]
#[ntest::timeout(5_000)]
fn opaque_overlay_blocks_nodes_beneath() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let danger = Signal::new(Rc::clone(&runtime), 0u32);
    let d = danger.clone();

    let mut raster = HeadlessRaster::new();
    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .children((
                group()
                    .key("danger")
                    .width(100.0)
                    .height(100.0)
                    .fill(Color::BLACK)
                    .on_click(move |_| d.set(d.get() + 1))
                    .into_element(),
                // Handler-less opaque overlay stacked over the same area via
                // negative margin (sibling overlap pattern).
                group()
                    .key("scrim")
                    .margin(Margin::left(-100.0))
                    .width(100.0)
                    .height(100.0)
                    .z_index(50)
                    .hit_test(HitTest::Opaque)
                    .into_element(),
            ))
            .into_element(),
        400,
        400,
    );
    frame(&mut engine, &mut raster);

    click(&mut engine, &mut raster, 50.0, 50.0);
    assert_eq!(
        danger.get(),
        0,
        "an opaque overlay must absorb the press so the button beneath never fires"
    );
}

/// Passthrough nodes never steal a press meant for the control beneath.
#[test]
#[ntest::timeout(5_000)]
fn passthrough_wrapper_does_not_swallow_clicks() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let taps = Signal::new(Rc::clone(&runtime), 0u32);
    let t = taps.clone();

    let mut raster = HeadlessRaster::new();
    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .children([group()
                .key("wrapper")
                .width(200.0)
                .height(200.0)
                .hit_test(HitTest::Passthrough)
                .children([group()
                    .key("button")
                    .width(100.0)
                    .height(100.0)
                    .fill(Color::BLACK)
                    .on_click(move |_| t.set(t.get() + 1))
                    .into_element()])
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine, &mut raster);

    click(&mut engine, &mut raster, 50.0, 50.0);
    assert_eq!(
        taps.get(),
        1,
        "a passthrough wrapper must not swallow the inner button's click"
    );
}

/// `visible(false)` removes the layout footprint; `visible(true)` restores it.
#[test]
#[ntest::timeout(5_000)]
fn visible_toggles_layout_footprint() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let flag = Signal::new(Rc::clone(&runtime), true);

    let mut raster = HeadlessRaster::new();
    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .children([group()
                .key("toggled")
                .width(100.0)
                .height(50.0)
                .fill(Color::BLACK)
                .visible(flag.clone())
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine, &mut raster);
    // flow_rect is the laid-out box (absolute_rect adds a 1px visual pad for
    // stroke/shadow bounds, so it reads 102x52 / 2x2 for a 100x50 / 0x0 node).
    let shown = engine
        .nodes()
        .flow_rect("toggled")
        .expect("visible node must have geometry");
    assert!(
        shown.width > 0.0 && shown.height > 0.0,
        "visible node must occupy space, got {shown:?}"
    );

    flag.set(false);
    frame(&mut engine, &mut raster);
    let hidden = engine
        .nodes()
        .flow_rect("toggled")
        .expect("hidden node must still resolve");
    assert_eq!(
        (hidden.width, hidden.height),
        (0.0, 0.0),
        "visible(false) must collapse the layout footprint"
    );
}
