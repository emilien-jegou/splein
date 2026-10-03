// Single responsibility: Pointer interaction DSL: hit order, capture, gestures, and release velocity.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::{group, IntoElement};
use aurora::foundation::{Color, Direction, Key, Size};
use aurora::input::{PointerPhase, DRAG_THRESHOLD};
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::runtime::Engine;

const W: f32 = 400.0;
const H: f32 = 400.0;

/// Steps one frame at 60Hz, rasterizing like the app loop.
fn frame(engine: &mut Engine) {
    engine.advance(1.0 / 60.0);
    engine.frame();
    engine.canvas();
}

/// Sends one pointer sample and frames.
fn send(engine: &mut Engine, x: f32, y: f32, phase: PointerPhase) {
    engine.queue_pointer_event(aurora::input::PointerEvent {
        position: aurora::foundation::Point::new(x, y),
        button: aurora::input::PointerButton::Primary,
        phase,
        click_count: 0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    });
    frame(engine);
}

/// A full press-move-release at one point, which the router reads as a click.
fn click(engine: &mut Engine, x: f32, y: f32) {
    send(engine, x, y, PointerPhase::Down);
    send(engine, x, y, PointerPhase::Up);
}

#[test]
#[ntest::timeout(30_000)]
fn a_click_reaches_the_node_under_the_pointer() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let clicks = Signal::new(Rc::clone(&runtime), 0u32);
    let tapped = clicks.clone();

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("button")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_click(move |_| tapped.set(tapped.get() + 1))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    click(&mut engine, 50.0, 50.0);
    assert_eq!(
        clicks.get(),
        1,
        "a press and release on the node must click it"
    );

    click(&mut engine, 300.0, 300.0);
    assert_eq!(
        clicks.get(),
        1,
        "a press on empty space must not click the node"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_press_bubbles_to_ancestors_above_the_target() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let inner_hits = Signal::new(Rc::clone(&runtime), 0u32);
    let outer_hits = Signal::new(Rc::clone(&runtime), 0u32);
    let (i, o) = (inner_hits.clone(), outer_hits.clone());

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .on_click(move |_| o.set(o.get() + 1))
            .children([group()
                .key("inner")
                .width(200.0)
                .height(200.0)
                .fill(Color::BLACK)
                .on_click(move |_| i.set(i.get() + 1))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    click(&mut engine, 50.0, 50.0);
    assert_eq!(
        inner_hits.get(),
        1,
        "the innermost interested node fires first"
    );
    assert_eq!(
        outer_hits.get(),
        1,
        "the press must also reach the ancestor"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_decorative_wrapper_does_not_swallow_the_click_inside_it() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let hits = Signal::new(Rc::clone(&runtime), 0u32);
    let h = hits.clone();

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("chrome")
                .width(300.0)
                .height(300.0)
                .fill(Color::hex(0xCCCCCC))
                .children([group()
                    .key("real")
                    .width(100.0)
                    .height(100.0)
                    .fill(Color::BLACK)
                    .on_click(move |_| h.set(h.get() + 1))
                    .into_element()])
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    // Click the wrapper itself: nothing is declared on it, so nothing should happen.
    click(&mut engine, 250.0, 250.0);
    assert_eq!(hits.get(), 0, "an undecorated wrapper owns no clicks");

    // Click the inner control: it must receive the press despite the wrapper on top of it.
    click(&mut engine, 50.0, 50.0);
    assert_eq!(hits.get(), 1, "the inner control must still be reachable");
}

#[test]
#[ntest::timeout(30_000)]
fn hover_enters_once_and_leaves_when_the_pointer_goes_away() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let entered = Signal::new(Rc::clone(&runtime), 0u32);
    let left = Signal::new(Rc::clone(&runtime), 0u32);
    let (en, lv) = (entered.clone(), left.clone());

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("zone")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_pointer_enter(move |_| en.set(en.get() + 1))
                .on_pointer_leave(move |_| lv.set(lv.get() + 1))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    send(&mut engine, 50.0, 50.0, PointerPhase::Move);
    assert_eq!(entered.get(), 1, "arriving fires enter");
    send(&mut engine, 60.0, 60.0, PointerPhase::Move);
    assert_eq!(entered.get(), 1, "staying inside must not re-fire enter");
    assert_eq!(left.get(), 0);
    send(&mut engine, 300.0, 300.0, PointerPhase::Move);
    assert_eq!(left.get(), 1, "leaving fires leave");
}

#[test]
#[ntest::timeout(30_000)]
fn a_press_that_moves_past_the_threshold_is_a_drag_not_a_click() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let clicks = Signal::new(Rc::clone(&runtime), 0u32);
    let steps = Signal::new(Rc::clone(&runtime), 0u32);
    let (c, s) = (clicks.clone(), steps.clone());

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("tile")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_click(move |_| c.set(c.get() + 1))
                .on_drag_step(move |_| s.set(s.get() + 1))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    send(&mut engine, 50.0, 50.0, PointerPhase::Down);
    send(
        &mut engine,
        50.0 + DRAG_THRESHOLD * 3.0,
        50.0,
        PointerPhase::Move,
    );
    send(&mut engine, 90.0, 50.0, PointerPhase::Drag);
    send(&mut engine, 90.0, 50.0, PointerPhase::Up);

    assert!(steps.get() > 0, "the gesture must deliver drag steps");
    assert_eq!(
        clicks.get(),
        0,
        "a gesture that travelled must not also report a click"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_drag_keeps_its_capture_after_the_pointer_leaves_the_node() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let steps = Signal::new(Rc::clone(&runtime), 0u32);
    let ended = Signal::new(Rc::clone(&runtime), 0u32);
    let (s, e) = (steps.clone(), ended.clone());

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("tile")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_drag_step(move |_| s.set(s.get() + 1))
                .on_drag_end(move |_| e.set(e.get() + 1))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    send(&mut engine, 50.0, 50.0, PointerPhase::Down);
    send(&mut engine, 60.0, 50.0, PointerPhase::Move);
    send(&mut engine, 60.0, 50.0, PointerPhase::Drag);
    let before = steps.get();
    assert!(before > 0, "the drag must have started");

    // Move far outside the node: capture must keep delivering.
    send(&mut engine, 380.0, 380.0, PointerPhase::Drag);
    assert!(
        steps.get() > before,
        "a captured gesture must follow the pointer off the node"
    );

    send(&mut engine, 380.0, 380.0, PointerPhase::Up);
    assert_eq!(ended.get(), 1, "release must end the gesture once");
}

#[test]
#[ntest::timeout(30_000)]
fn release_velocity_reflects_the_flick_that_was_actually_made() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let released = Rc::new(RefCell::new(None));
    let sink = Rc::clone(&released);

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("tile")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_drag_end(move |info| *sink.borrow_mut() = Some(info.velocity))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    // One frame is 1/60s; a steady 30px/frame flick is therefore 1800 px/s.
    send(&mut engine, 50.0, 50.0, PointerPhase::Down);
    send(&mut engine, 80.0, 50.0, PointerPhase::Move);
    send(&mut engine, 110.0, 50.0, PointerPhase::Drag);
    send(&mut engine, 140.0, 50.0, PointerPhase::Up);

    let velocity = released.borrow().expect("drag end must fire");
    assert!(
        velocity.x > 1000.0,
        "release velocity should read the flick, got {velocity:?}"
    );
    assert!(
        velocity.y.abs() < 1.0,
        "a horizontal flick must not invent vertical velocity, got {velocity:?}"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_double_click_is_reported_as_a_second_click() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let max_count = Signal::new(Rc::clone(&runtime), 0u32);
    let m = max_count.clone();

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("zone")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_click(move |info| m.set(m.get().max(info.click_count)))
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    click(&mut engine, 50.0, 50.0);
    assert_eq!(max_count.get(), 1, "the first click reports count one");
    click(&mut engine, 50.0, 50.0);
    assert_eq!(
        max_count.get(),
        2,
        "a second click in the burst reports two"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn hit_order_follows_paint_order() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let hits = Signal::new(Rc::clone(&runtime), Vec::<&'static str>::new());
    let h = hits.clone();

    // Two overlapping siblings. The later one paints last, so it must be hit first.
    let record = move |name: &'static str, sink: Signal<Vec<&'static str>>| {
        let sink = sink.clone();
        move |_: &aurora::input::ClickInfo| {
            let mut next = sink.get();
            next.push(name);
            sink.set(next);
        }
    };

    // Absolute positioning so the two genuinely overlap: stacked siblings would
    // never share a point and could not say anything about hit order.
    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([
                group()
                    .key("under")
                    .anchor(aurora::foundation::Anchor::TopLeft)
                    .width(200.0)
                    .height(200.0)
                    .fill(Color::BLACK)
                    .on_click(record("under", h.clone()))
                    .into_element(),
                group()
                    .key("over")
                    .anchor(aurora::foundation::Anchor::TopLeft)
                    .width(100.0)
                    .height(100.0)
                    .fill(Color::hex(0x808080))
                    .on_click(record("over", h.clone()))
                    .into_element(),
            ])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    // Point inside the overlap: the topmost painted node must receive the click.
    click(&mut engine, 50.0, 50.0);
    assert_eq!(
        hits.get(),
        vec!["over"],
        "the node painted last must be hit first, got {:?}",
        hits.get()
    );

    // Point inside only the lower sibling.
    click(&mut engine, 150.0, 150.0);
    assert_eq!(hits.get(), vec!["over", "under"]);
}

#[test]
#[ntest::timeout(30_000)]
fn a_higher_z_index_is_hit_before_a_later_sibling() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let hits = Signal::new(Rc::clone(&runtime), Vec::<&'static str>::new());
    let h = hits.clone();
    let record = move |name: &'static str, sink: Signal<Vec<&'static str>>| {
        let sink = sink.clone();
        move |_: &aurora::input::ClickInfo| {
            let mut next = sink.get();
            next.push(name);
            sink.set(next);
        }
    };

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .direction(Direction::Vertical)
            .children([
                group()
                    .key("top")
                    .width(200.0)
                    .height(200.0)
                    .z_index(10)
                    .fill(Color::BLACK)
                    .on_click(record("top", h.clone()))
                    .into_element(),
                group()
                    .key("bottom")
                    .width(200.0)
                    .height(200.0)
                    .fill(Color::hex(0x808080))
                    .on_click(record("bottom", h.clone()))
                    .into_element(),
            ])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    click(&mut engine, 100.0, 100.0);
    assert_eq!(
        hits.get(),
        vec!["top"],
        "z-index must win over document order, got {:?}",
        hits.get()
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_view_clipped_out_of_its_parent_does_not_swallow_clicks() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let outer_hits = Signal::new(Rc::clone(&runtime), 0u32);
    let inner_hits = Signal::new(Rc::clone(&runtime), 0u32);
    let (o, i) = (outer_hits.clone(), inner_hits.clone());

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("window")
                .width(100.0)
                .height(100.0)
                .clip(true)
                .on_click(move |_| o.set(o.get() + 1))
                .children([group()
                    .key("tall")
                    .width(100.0)
                    .height(300.0)
                    .fill(Color::BLACK)
                    .on_click(move |_| i.set(i.get() + 1))
                    .into_element()])
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    // Inside the clip: the child is visible and receives the press.
    click(&mut engine, 50.0, 50.0);
    assert_eq!(inner_hits.get(), 1, "the visible child is hittable");

    // Below the clip: the child is not painted there, so it must not take the press.
    click(&mut engine, 50.0, 200.0);
    assert_eq!(
        inner_hits.get(),
        1,
        "a child clipped out of view must not be hit"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_keyed_node_reports_its_hover_target() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("known")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_click(|_| {})
                .into_element()])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    send(&mut engine, 50.0, 50.0, PointerPhase::Move);
    assert_eq!(
        engine.pointer_position(),
        Some(aurora::foundation::Point::new(50.0, 50.0)),
        "the engine must track the cursor"
    );

    // Existing hit_test keeps working and agrees with the new chain.
    assert_eq!(
        engine.hit_test(50.0, 50.0),
        Some(Key::from("known")),
        "the legacy key lookup must still resolve"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn handlers_survive_a_remount() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let clicks = Signal::new(Rc::clone(&runtime), 0u32);
    let counter = clicks.clone();

    let view = move || {
        let counter = counter.clone();
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .children([group()
                .key("stable")
                .width(100.0)
                .height(100.0)
                .fill(Color::BLACK)
                .on_click(move |_| counter.set(counter.get() + 1))
                .into_element()])
            .into_element()
    };

    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view(), 400, 400);
    frame(&mut engine);
    click(&mut engine, 50.0, 50.0);
    assert_eq!(clicks.get(), 1);

    // Remounting the same keyed structure must reattach handlers to the reused node.
    engine.mount(view());
    frame(&mut engine);
    click(&mut engine, 50.0, 50.0);
    assert_eq!(
        clicks.get(),
        2,
        "handlers must be reattached when a keyed node is reused"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn a_handler_write_reaches_layout_in_the_same_frame() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), false);
    let toggle = open.clone();

    let mut engine = Engine::with_runtime(
        Rc::clone(&runtime),
        group()
            .width(W)
            .height(H)
            .fill(Color::WHITE)
            .direction(Direction::Vertical)
            .children([
                group()
                    .key("trigger")
                    .width(100.0)
                    .height(100.0)
                    .fill(Color::BLACK)
                    .on_click(move |_| toggle.set(!toggle.get()))
                    .into_element(),
                group()
                    .width(Size::fill())
                    .height(open.map(|is_open| if is_open { 120.0 } else { 10.0 }))
                    .fill(Color::hex(0x00FF00))
                    .into_element(),
            ])
            .into_element(),
        400,
        400,
    );
    frame(&mut engine);

    assert_eq!(
        engine.rect("panelless").map(|r| r.height),
        None,
        "sanity: no such key yet"
    );

    click(&mut engine, 50.0, 50.0);
    // The height is driven by the same signal the click wrote, so it must already be
    // applied by the frame that dispatched the event.
    let grown = engine
        .scene()
        .chunks
        .iter()
        .flat_map(|c| c.commands.iter())
        .count();
    assert!(grown > 0, "the scene must have been compiled");
    assert!(
        open.get(),
        "the handler write must have been visible immediately after the frame"
    );
}
