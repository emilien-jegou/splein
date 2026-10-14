// Single responsibility: Translating platform window events into engine pointer events.

use winit::event::Modifiers;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::ModifiersKeyState;
use winit::keyboard::{Key, NamedKey};

use crate::foundation::Point;
use crate::input::{KeyCode, KeyEvent, PointerButton, PointerEvent, PointerPhase, WheelInfo};
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
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(p) = self.position else {
                    // Same rule as a press: no position, no target.
                    return false;
                };
                // Winit reports lines (notched wheels) or physical pixels
                // (touchpads). A line scrolls about three text rows.
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 48.0, y * 48.0),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                if dx == 0.0 && dy == 0.0 {
                    return false;
                }
                let modifiers = self.modifiers;
                engine.queue_wheel_event(WheelInfo {
                    position: p,
                    delta_x: dx,
                    delta_y: dy,
                    shift: down(modifiers.lshift_state()) || down(modifiers.rshift_state()),
                    alt: down(modifiers.lalt_state()) || down(modifiers.ralt_state()),
                    ctrl: down(modifiers.lcontrol_state()) || down(modifiers.rcontrol_state()),
                    meta: down(modifiers.lsuper_state()) || down(modifiers.rsuper_state()),
                });
                true
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Releases carry no editing intent; auto-repeats arrive as presses.
                if event.state != ElementState::Pressed {
                    return false;
                }
                let Some(code) = map_logical_key(&event.logical_key) else {
                    return false;
                };
                // Keys reach focused text or a focused key handler (e.g. a
                // scroll viewport); without either nothing can change, so this
                // asks for no redraw either.
                if !engine.has_keyboard_target() {
                    return false;
                }
                let modifiers = self.modifiers;
                engine.queue_key_event(KeyEvent {
                    code,
                    shift: down(modifiers.lshift_state()) || down(modifiers.rshift_state()),
                    alt: down(modifiers.lalt_state()) || down(modifiers.ralt_state()),
                    ctrl: down(modifiers.lcontrol_state()) || down(modifiers.rcontrol_state()),
                    meta: down(modifiers.lsuper_state()) || down(modifiers.rsuper_state()),
                    repeat: event.repeat,
                });
                true
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
                // Even with nothing held, route one offscreen move: hover
                // states (viewport enter/leave, scrollbar visibility) resolve
                // on hit position, and without this they stick visible with
                // no pointer left to correct them.
                engine.queue_pointer_move(-10.0, -10.0);
                true
            }
            WindowEvent::Focused(false) => {
                let was_held = self.held;
                self.held = false;
                self.position = None;
                if was_held {
                    engine.cancel_pointer_gesture();
                    return true;
                }
                // Same as CursorLeft: an app switch must resolve hover outward.
                engine.queue_pointer_move(-10.0, -10.0);
                true
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

/// Maps a platform logical key to the engine's selection vocabulary.
///
/// Pure so tests can cover it without constructing a full window event
/// (winit's `KeyEvent` carries crate-private platform data).
pub fn map_logical_key(key: &Key) -> Option<KeyCode> {
    match key {
        Key::Named(named) => match named {
            NamedKey::ArrowLeft => Some(KeyCode::Left),
            NamedKey::ArrowRight => Some(KeyCode::Right),
            NamedKey::ArrowUp => Some(KeyCode::Up),
            NamedKey::ArrowDown => Some(KeyCode::Down),
            NamedKey::Home => Some(KeyCode::Home),
            NamedKey::End => Some(KeyCode::End),
            NamedKey::PageUp => Some(KeyCode::PageUp),
            NamedKey::PageDown => Some(KeyCode::PageDown),
            NamedKey::Escape => Some(KeyCode::Escape),
            NamedKey::Space => Some(KeyCode::Char(' ')),
            _ => None,
        },
        Key::Character(text) => text.chars().next().map(KeyCode::Char),
        _ => None,
    }
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
