// Single responsibility: Backend-neutral pointer event vocabulary shared by the engine and the DSL.

use aurora_foundation::Point;

/// Which pointer button a press refers to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

/// What a pointer event reports about the button it concerns.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    /// The button went down on the target.
    Down,
    /// The button came up on the target.
    Up,
    /// The pointer moved with no button held.
    Move,
    /// The button is held and the pointer moved.
    Drag,
}

/// A single pointer occurrence in absolute window coordinates.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PointerEvent {
    pub position: Point,
    pub button: PointerButton,
    pub phase: PointerPhase,
    /// Phase counters, so a handler can distinguish a double click from two clicks.
    pub click_count: u32,
    /// Whether the platform reported modifiers at the time of the event.
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub meta: bool,
}

impl PointerEvent {
    /// Whether this event carries a held button, whether it moved or not.
    #[inline(always)]
    pub fn is_dragging(&self) -> bool {
        matches!(self.phase, PointerPhase::Drag)
    }
}

/// Data handed to a drag gesture callback.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct DragInfo {
    /// Pointer position in absolute window coordinates.
    pub position: Point,
    /// Pointer displacement since the gesture began, in window pixels.
    pub delta: Point,
    /// Pointer velocity in window pixels per second, averaged over the recent history.
    pub velocity: Point,
    /// Whether the button is still held.
    pub held: bool,
}

/// Data handed to a click callback.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ClickInfo {
    pub position: Point,
    /// Number of clicks in the current burst, so a handler can react to a double click.
    pub click_count: u32,
}

/// Data handed to a hover callback.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HoverInfo {
    pub position: Point,
}

/// Pointer cursor shape the engine asks the platform for.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CursorIcon {
    /// The ordinary arrow.
    #[default]
    Default,
    /// The I-beam over selectable text.
    Text,
}
