// Single responsibility: The reported symptom, end to end: a click in a real demo.
//
// Every other input test drives the engine directly, which is exactly why the reported bug
// survived so long: the interaction core was complete and thoroughly tested, and nothing ever
// fed it a platform event. These tests go through the same bridge the window event loop uses,
// against the actual demo pages, so the wiring itself is covered.

#[allow(dead_code)]
#[path = "../../../examples/motion_accordion/main.rs"]
mod accordion;

#[allow(dead_code)]
#[path = "../../../examples/motion_press/main.rs"]
mod press;

#[allow(dead_code)]
#[path = "../../../examples/motion_reorder/main.rs"]
mod reorder;

use std::cell::RefCell;
use std::rc::Rc;

use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

use aurora::app::runner::pointer::PointerBridge;
use aurora::reactive::{Cx, ReactiveRuntime, Signal};
use aurora_tiny_skia::testing::Harness;

fn moved(x: f64, y: f64) -> WindowEvent {
    WindowEvent::CursorMoved {
        position: winit::dpi::PhysicalPosition::new(x, y),
        device_id: unsafe { DeviceId::dummy() },
    }
}

fn button(state: ElementState) -> WindowEvent {
    WindowEvent::MouseInput {
        state,
        button: MouseButton::Left,
        device_id: unsafe { DeviceId::dummy() },
    }
}

/// Presses and releases over the centre of a keyed view, as a window would.
fn click(h: &mut Harness, bridge: &mut PointerBridge, key: &str) {
    let at = h.located(key).center();
    bridge.dispatch(&moved(at.x as f64, at.y as f64), h.engine());
    bridge.dispatch(&button(ElementState::Pressed), h.engine());
    bridge.dispatch(&button(ElementState::Released), h.engine());
}

fn device() -> DeviceId {
    unsafe { DeviceId::dummy() }
}

#[test]
#[ntest::timeout(5_000)]
fn clicking_an_accordion_header_in_a_real_window_switches_the_open_panel() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&rt), Some(0usize));
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(|| accordion::Page(open.clone())),
        accordion::WIDTH,
        accordion::HEIGHT,
    );
    h.steps(10);
    let mut bridge = PointerBridge::new();
    assert_eq!(open.get(), Some(0), "panel 0 starts open");

    click(&mut h, &mut bridge, "item-2");
    h.steps(30);

    assert_eq!(
        open.get(),
        Some(2),
        "a click on a header must reach the handler the demo hangs its toggle on"
    );
    let opened = h.located("panel-2").flow.height;
    let closed = h.located("panel-0").flow.height;
    assert!(
        opened > closed,
        "the clicked panel must be the tall one: {opened} vs {closed}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn clicking_a_second_accordion_header_moves_the_open_panel() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&rt), Some(0usize));
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(|| accordion::Page(open.clone())),
        accordion::WIDTH,
        accordion::HEIGHT,
    );
    h.steps(10);
    let mut bridge = PointerBridge::new();

    // Clicking the open header closes it; the demos are driven by real pointers now, so this
    // is the path that used to do nothing at all.
    click(&mut h, &mut bridge, "item-0");
    h.steps(20);
    assert_eq!(open.get(), None, "clicking the open header closes the list");

    click(&mut h, &mut bridge, "item-1");
    h.steps(20);
    assert_eq!(open.get(), Some(1));
}

#[test]
#[ntest::timeout(5_000)]
fn a_press_and_hold_on_the_demo_button_squeezes_it() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(press::Page),
        press::WIDTH,
        press::HEIGHT,
    );
    h.steps(10);
    let mut bridge = PointerBridge::new();

    let at = h.located("cta").center();
    bridge.dispatch(&moved(at.x as f64, at.y as f64), h.engine());
    bridge.dispatch(&button(ElementState::Pressed), h.engine());
    for _ in 0..10 {
        // A real finger or mouse wanders; the engine must keep squeezing while it is down.
        bridge.dispatch(&moved(at.x as f64 + 1.0, at.y as f64), h.engine());
        h.step();
    }

    let held = h.transform("cta").a;
    assert!(
        held < 0.98,
        "holding must reach the press handler, scale is {held}"
    );

    bridge.dispatch(&button(ElementState::Released), h.engine());
    assert!(h.settle(600), "the button must spring back to rest");
    let released = h.transform("cta").a;
    assert!(
        (released - 1.0).abs() < 0.02,
        "releasing must restore the button, scale is {released}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn dragging_a_reorder_row_by_pointer_reorders_the_list() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(reorder::Page),
        reorder::WIDTH,
        reorder::HEIGHT,
    );
    assert!(h.settle(900), "the list must settle before it is aimed at");

    let before = h.located("row-Squiggle").flow.y;
    let at = h.located("row-Baseline").center();
    let mut bridge = PointerBridge::new();

    bridge.dispatch(&moved(at.x as f64, at.y as f64), h.engine());
    bridge.dispatch(&button(ElementState::Pressed), h.engine());
    for step in 1..=8 {
        let y = at.y as f64 + 20.0 * step as f64;
        bridge.dispatch(&moved(at.x as f64, y), h.engine());
        h.step();
    }
    bridge.dispatch(&button(ElementState::Released), h.engine());
    assert!(h.settle(900), "the reordered list must settle");

    // The drop target is measured from where the gesture started, which is what makes it track the
    // pointer rather than accumulate a swap per step.
    let landed = h.located("row-Baseline").flow.y;
    assert!(
        (landed - before).abs() > 1.0,
        "a pointer drag must move the row to another slot, y {before} -> {landed}"
    );

    // The slot is read off the measured pitch rather than a declared stride: the rows fill the
    // list and divide whatever height it was given, so the row's index is the only stable way to
    // say where it ended up.
    // The pitch, measured from two rows that are adjacent *and* unmoved. Measuring it from the
    // row that was dragged would fold the drag into the pitch; measuring it across a gap would
    // multiply it by the number of gaps.
    let stride = (h.located("row-Highlight").flow.y - h.located("row-Squiggle").flow.y).abs();
    assert!(
        stride > 1.0,
        "the measured pitch must be usable as a stride, got {stride}"
    );

    // 160px of drag, measured from the slot the gesture began in. At the measured pitch that is
    // the slot the row belongs at -- which is the property worth asserting, since it is what a
    // constant stride got wrong whenever the window changed the row height.
    let index = (landed / stride).round() as usize;
    assert!(
        (2..=3).contains(&index),
        "a 160px drag should land the row two or three slots down at a pitch of {stride:.1}, \
         but it landed at slot {index} (y {landed})"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn leaving_the_window_mid_drag_does_not_pin_the_row_to_the_pointer() {
    // The failure this guards against is nasty because it looks like a rendering bug: the row
    // stays stuck where the pointer was when it left, and nothing moves it again.
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(reorder::Page),
        reorder::WIDTH,
        reorder::HEIGHT,
    );
    assert!(h.settle(900));

    let at = h.located("row-Baseline").center();
    let mut bridge = PointerBridge::new();
    bridge.dispatch(&moved(at.x as f64, at.y as f64), h.engine());
    bridge.dispatch(&button(ElementState::Pressed), h.engine());
    bridge.dispatch(&moved(at.x as f64 + 40.0, at.y as f64 + 60.0), h.engine());
    h.step();

    bridge.dispatch(
        &WindowEvent::CursorLeft {
            device_id: device(),
        },
        h.engine(),
    );
    assert!(
        !bridge.is_held(),
        "the gesture must be dropped, not left held"
    );
    assert!(h.engine().pointer_position().is_none());
}
