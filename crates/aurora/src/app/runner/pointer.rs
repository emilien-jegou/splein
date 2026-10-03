// Single responsibility: Translating platform window events into engine pointer events.

use winit::event::Modifiers;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::ModifiersKeyState;

use crate::foundation::Point;
use crate::input::{PointerButton, PointerEvent, PointerPhase};
use crate::runtime::Engine;

/// Bridges a platform's window events to the engine's pointer vocabulary.
///
/// This exists because the two sides disagree about shape, and nothing else reconciles them.
/// Winit reports a stream of `CursorMoved` and `MouseInput` occurrences with no notion of a
/// gesture, and a `MouseInput` carries no position at all; the engine wants press, release and
/// drag phases in logical window coordinates and does its own capture, click-burst and
/// velocity tracking. Without a bridge the engine's interaction core is complete and
/// unreachable, which is exactly what happened: every demo drove its engine from tests and
/// nothing ever fed it a real pointer.
///
/// Drag classification is deliberately left to the engine, which knows whether the pointer
/// has travelled past the drag threshold. Re-deriving "is this a drag" here would duplicate
/// that threshold and let the two disagree.
pub struct PointerBridge {
    /// Last position the platform reported, in the engine's coordinate space.
    position: Option<Point>,
    /// Whether any button is currently down.
    held: bool,
    modifiers: Modifiers,
}

impl Default for PointerBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl PointerBridge {
    pub fn new() -> Self {
        Self {
            position: None,
            held: false,
            modifiers: Modifiers::default(),
        }
    }

    /// Whether a button is currently down.
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// Delivers one window event to the engine, returning whether anything was dispatched.
    ///
    /// Anything that changes what the user sees returns true so the caller can request a
    /// redraw; a hover with no view under it deliberately does not, because an idle app
    /// should not repaint just because the pointer moved across empty page background.
    pub fn dispatch(&mut self, event: &WindowEvent, engine: &mut Engine) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let p = to_logical(*position);
                self.position = Some(p);
                // The engine decides between Move and Drag from its own threshold state.
                engine.queue_pointer_move(p.x, p.y);
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // The engine's handler vocabulary is primary-only: there is no secondary
                // click callback, and its tracker counts a click burst per button without
                // distinguishing which. Forwarding a right click would therefore fire every
                // `on_click` in the tree, so a context-menu gesture would activate the button
                // underneath it. Non-primary buttons are dropped here rather than at the
                // engine, which keeps the engine's semantics about gestures and leaves the
                // platform-policy decision in one place.
                if *button != MouseButton::Left {
                    return false;
                }
                let Some(p) = self.position else {
                    // A press with no known position cannot be aimed at anything. Winit
                    // reports one only when the pointer entered without moving, which does not
                    // happen on any desktop we target, and inventing (0, 0) would fire a
                    // handler at whatever happens to live in the corner.
                    return false;
                };
                let phase = match state {
                    ElementState::Pressed => {
                        self.held = true;
                        PointerPhase::Down
                    }
                    ElementState::Released => {
                        self.held = false;
                        PointerPhase::Up
                    }
                };
                let modifiers = self.modifiers;
                engine.queue_pointer_event(PointerEvent {
                    position: p,
                    button: PointerButton::Primary,
                    phase,
                    // The engine counts click bursts itself, from the press/release timing it
                    // already tracks, so this stays zero rather than duplicating that.
                    click_count: 0,
                    shift: down(modifiers.lshift_state()) || down(modifiers.rshift_state()),
                    alt: down(modifiers.lalt_state()) || down(modifiers.ralt_state()),
                    ctrl: down(modifiers.lcontrol_state()) || down(modifiers.rcontrol_state()),
                    meta: down(modifiers.lsuper_state()) || down(modifiers.rsuper_state()),
                });
                true
            }
            WindowEvent::ModifiersChanged(state) => {
                self.modifiers = *state;
                // A modifier can change what a handler does on the next event, but nothing on
                // screen has changed yet, so this does not ask for a redraw.
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.position = None;
                // Leaving the window mid-drag must not strand a capture on the view that was
                // pressed, or the view stays stuck to the pointer's last position forever.
                if self.held {
                    self.held = false;
                    engine.cancel_pointer_gesture();
                    return true;
                }
                false
            }
            WindowEvent::Focused(false) => {
                let was_held = self.held;
                self.held = false;
                self.position = None;
                if was_held {
                    engine.cancel_pointer_gesture();
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// Whether one modifier key is held.
///
/// Both the left and right key are checked separately because a handler asking about "shift"
/// means shift, not left-shift, and treating the right key as a different modifier is the
/// kind of platform detail that silently breaks shortcuts.
fn down(state: ModifiersKeyState) -> bool {
    state == ModifiersKeyState::Pressed
}

/// Converts a physical position into the engine's window coordinate space.
///
/// The engine's viewport is set from `WindowEvent::Resized`, which winit also reports in
/// physical pixels, so input deliberately stays in that same space. Dividing by the scale
/// factor here without also dividing the viewport would aim clicks at half the window on a
/// HiDPI display, which is worse than a UI that is sized in physical pixels throughout.
fn to_logical(position: winit::dpi::PhysicalPosition<f64>) -> Point {
    Point::new(position.x as f32, position.y as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::Element;
    use crate::prelude::*;
    use crate::reactive::ReactiveRuntime;
    use crate::testing::Harness;
    use std::cell::RefCell;
    use std::rc::Rc;
    use winit::event::DeviceId;

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

    /// A page with one clickable card whose fill reports how many times it was pressed.
    ///
    /// Built from an explicit signal rather than a `#[component]`, because that macro expands
    /// to absolute `::aurora::` paths and so cannot be used from inside the crate itself.
    fn counter_page(taps: Signal<u32>) -> Element {
        let shown = taps.clone();
        group()
            .width(Size::fill())
            .height(Size::fill())
            .fill(Color::WHITE)
            .children([group()
                .key("card")
                .margin(crate::foundation::Margin::all(40.0))
                .width(80.0)
                .height(60.0)
                .fill(shown.map(move |t| {
                    if t == 0 {
                        Color::hex(0xCCCCCC)
                    } else {
                        Color::hex(0x1D4ED8)
                    }
                }))
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
            crate::testing::Rgba {
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
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
    #[ntest::timeout(30_000)]
    fn an_unrelated_window_event_is_ignored() {
        let (mut h, _rt) = counter();
        h.steps(2);
        let mut bridge = PointerBridge::new();
        assert!(!bridge.dispatch(&WindowEvent::Focused(true), h.engine()));
        assert!(!tapped(&mut h));
    }
}
