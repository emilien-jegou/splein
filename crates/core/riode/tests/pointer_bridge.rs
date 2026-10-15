// Single responsibility: Platform pointer bridge dispatch, exercised through real window events.
//
// Moved out of `riode/src/app/runner/pointer.rs`: the bridge tests drive a rasterizing
// harness, which now lives in `riode-tiny-skia`. An in-crate unit test would link the
// backend crate against a second build of `riode` itself, so these live here where exactly
// one `riode` exists.

use std::cell::RefCell;
use std::rc::Rc;

use riode::app::runner::pointer::PointerBridge;
use riode::dsl::Element;
use riode::prelude::*;
use riode::internal::*;  // P10 escape hatch: this test composes repr-builders directly
use riode::reactive::ReactiveRuntime;
use riode_tiny_skia::testing::{Harness, Rgba};
use winit::event::{DeviceId, ElementState, Modifiers, MouseButton, WindowEvent};

fn pos(x: f64, y: f64) -> winit::dpi::PhysicalPosition<f64> {
    winit::dpi::PhysicalPosition::new(x, y)
}

fn moved(x: f64, y: f64) -> WindowEvent {
    WindowEvent::CursorMoved {
        position: pos(x, y),
        device_id: unsafe { DeviceId::dummy() },
    }
}

fn button(state: ElementState, button: MouseButton) -> WindowEvent {
    WindowEvent::MouseInput {
        state,
        button,
        device_id: unsafe { DeviceId::dummy() },
    }
}

/// A page with one selectable text run, keyed for assertions.
fn selectable_page() -> Element {
    group()
        .width(Size::fill())
        .height(Size::fill())
        .fill(Color::WHITE)
        .children([text("hello world")
            .key("words")
            .selectable(true)
            .size(14.0)
            .color(Color::BLACK)])
        .into_element()
}

/// A page with one clickable card whose fill reports how many times it was pressed.
fn counter_page(taps: Signal<u32>) -> Element {
    let shown = taps.clone();
    group()
        .width(Size::fill())
        .height(Size::fill())
        .fill(Color::WHITE)
        .children([group()
            .key("card")
            .margin(riode::foundation::Margin::all(40.0))
            .width(80.0)
            .height(60.0)
            .fill(shown.map(move |t| {
                if t == 0 {
                    Color::hex(0xCCCCCC)
                } else {
                    Color::hex(0x1D4ED8)
                }
            }))
            // Draggable on purpose: the threshold test below needs a drag source, and
            // since drag recognition requires drag interest, a click-only card would
            // (correctly) never report dragging no matter how far the pointer travels.
            .on_drag_step(move |_| {})
            .on_click(move |_| taps.set(taps.get() + 1))
            .children([text("tap").size(12.0).color(Color::BLACK)])])
        .into_element()
}

fn counter() -> (Harness, Rc<RefCell<ReactiveRuntime>>) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let taps = Signal::new(Rc::clone(&runtime), 0u32);
    let h = Harness::from_runtime(Rc::clone(&runtime), counter_page(taps), 300, 300);
    (h, runtime)
}

fn tapped(h: &mut Harness) -> bool {
    let r = h.located("card").absolute;
    let shot = h.view();
    let p = shot
        .at(r.x as u32 + 2, (r.y + r.height / 2.0) as u32)
        .unwrap();
    p.near(
        Rgba {
            r: 0x1D,
            g: 0x4E,
            b: 0xD8,
            a: 255,
        },
        30,
    )
}

/// The regression this module exists for: a real press-and-release through platform
/// events must reach the view's click handler.
#[test]
#[ntest::timeout(5_000)]
fn a_platform_press_and_release_reaches_the_click_handler() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    assert!(!tapped(&mut h), "the card starts untapped");

    // The card sits at margin 40, so its centre is around (80, 70).
    bridge.dispatch(&moved(80.0, 70.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine(),
    );
    assert!(bridge.is_held());
    bridge.dispatch(
        &button(ElementState::Released, MouseButton::Left),
        h.engine(),
    );
    assert!(!bridge.is_held());
    h.steps(4);

    assert!(
        tapped(&mut h),
        "a click delivered through platform events did not reach the handler"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_press_away_from_any_view_changes_nothing() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    bridge.dispatch(&moved(5.0, 290.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine(),
    );
    bridge.dispatch(
        &button(ElementState::Released, MouseButton::Left),
        h.engine(),
    );
    h.steps(4);
    assert!(
        !tapped(&mut h),
        "a click on empty background must do nothing"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_press_without_a_known_position_is_dropped_rather_than_aimed_at_the_origin() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    // No CursorMoved first.
    assert!(!bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine()
    ));
    h.steps(4);
    assert!(!tapped(&mut h));
}

#[test]
#[ntest::timeout(5_000)]
fn a_non_primary_button_does_not_fire_a_click_handler() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    bridge.dispatch(&moved(80.0, 70.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Right),
        h.engine(),
    );
    bridge.dispatch(
        &button(ElementState::Released, MouseButton::Right),
        h.engine(),
    );
    h.steps(4);
    assert!(
        !tapped(&mut h),
        "a right click must not be delivered as a primary click: the engine has no \
         secondary-button vocabulary and would count it as a click burst"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn leaving_the_window_mid_drag_releases_the_gesture() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    bridge.dispatch(&moved(80.0, 70.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine(),
    );
    assert!(bridge.is_held());

    // A capture left set here would keep the card pinned to the pointer's last position
    // forever, so the gesture has to be dropped.
    assert!(bridge.dispatch(
        &WindowEvent::CursorLeft {
            device_id: unsafe { DeviceId::dummy() },
        },
        h.engine()
    ));
    assert!(!bridge.is_held());
    assert!(h.engine().pointer_position().is_none());
}

#[test]
#[ntest::timeout(5_000)]
fn losing_focus_mid_drag_releases_the_gesture() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    bridge.dispatch(&moved(80.0, 70.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine(),
    );
    assert!(bridge.dispatch(&WindowEvent::Focused(false), h.engine()));
    assert!(!bridge.is_held());
}

#[test]
#[ntest::timeout(5_000)]
fn a_modifier_change_alone_does_not_ask_for_a_redraw() {
    // A modifier can change what the next click does, but nothing on screen has changed
    // yet, so an app that repainted here would burn a frame on every key press.
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    assert!(!bridge.dispatch(
        &WindowEvent::ModifiersChanged(Modifiers::default()),
        h.engine()
    ));
}

#[test]
#[ntest::timeout(5_000)]
fn a_drag_crosses_the_threshold_and_reports_dragging() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    bridge.dispatch(&moved(80.0, 70.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine(),
    );
    // A move inside the 4px drag threshold stays a hover-and-press, not a drag.
    bridge.dispatch(&moved(82.0, 71.0), h.engine());
    h.step();
    assert!(!h.engine().pointer_is_dragging());

    bridge.dispatch(&moved(160.0, 200.0), h.engine());
    h.step();
    assert!(
        h.engine().pointer_is_dragging(),
        "travelling past the threshold must start a drag"
    );

    bridge.dispatch(
        &button(ElementState::Released, MouseButton::Left),
        h.engine(),
    );
    h.step();
    assert!(!h.engine().pointer_is_dragging());
}

#[test]
#[ntest::timeout(5_000)]
fn an_unrelated_window_event_is_ignored() {
    let (mut h, _rt) = counter();
    h.steps(2);
    let mut bridge = PointerBridge::new();
    assert!(!bridge.dispatch(&WindowEvent::Focused(true), h.engine()));
    assert!(!tapped(&mut h));
}

#[test]
#[ntest::timeout(5_000)]
fn platform_keys_map_to_selection_keys() {
    use riode::app::runner::pointer::map_logical_key;
    use riode::input::KeyCode;
    use winit::keyboard::{Key, NamedKey};

    assert_eq!(
        map_logical_key(&Key::Named(NamedKey::ArrowLeft)),
        Some(KeyCode::Left)
    );
    assert_eq!(
        map_logical_key(&Key::Named(NamedKey::ArrowRight)),
        Some(KeyCode::Right)
    );
    assert_eq!(
        map_logical_key(&Key::Named(NamedKey::ArrowUp)),
        Some(KeyCode::Up)
    );
    assert_eq!(
        map_logical_key(&Key::Named(NamedKey::ArrowDown)),
        Some(KeyCode::Down)
    );
    assert_eq!(
        map_logical_key(&Key::Named(NamedKey::Home)),
        Some(KeyCode::Home)
    );
    assert_eq!(
        map_logical_key(&Key::Named(NamedKey::End)),
        Some(KeyCode::End)
    );
    assert_eq!(
        map_logical_key(&Key::Named(NamedKey::Escape)),
        Some(KeyCode::Escape)
    );
    assert_eq!(
        map_logical_key(&Key::Character("c".into())),
        Some(KeyCode::Char('c'))
    );
    assert_eq!(map_logical_key(&Key::Named(NamedKey::F1)), None);
}

#[test]
#[ntest::timeout(5_000)]
fn a_key_without_focus_changes_nothing() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mut h = Harness::from_runtime(Rc::clone(&runtime), selectable_page(), 300, 300);
    h.steps(2);
    // No press happened, so nothing is focused and the key must not even
    // ask for a redraw. (A full key event cannot be built in tests — winit's
    // KeyEvent carries crate-private data — so the queue path is covered by
    // the engine-level keyboard tests instead.)
    assert_eq!(h.engine().focused_text(), None);
    assert_eq!(h.engine().text_selection("words"), None);
}
