// Single responsibility: Backend-neutral keyboard event vocabulary for selection editing.
//
// Mirrors the pointer vocabulary in `event.rs`: the engine never sees winit
// types, and the bridge translates platform keys into these.

/// A key the engine acts on for text selection.
///
/// Deliberately small: arrows, line edges, and characters (for Ctrl+A/C and
/// future editing). Typing, function keys, and IME composition are out of scope
/// for readonly selection.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Escape,
    /// A printable character, already resolved through the platform layout.
    Char(char),
}

/// A single key press in engine coordinates.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    /// Whether shift was held (extends the selection).
    pub shift: bool,
    /// Whether alt/option was held.
    pub alt: bool,
    /// Whether control was held (word jumps, select-all, copy).
    pub ctrl: bool,
    /// Whether the platform key (Cmd/Super) was held (treated like control).
    pub meta: bool,
    /// Whether the platform delivered this as an auto-repeat.
    pub repeat: bool,
}

impl KeyEvent {
    /// Whether this press carries a word-jump modifier.
    #[inline]
    pub fn word_modifier(self) -> bool {
        self.ctrl || self.meta
    }

    /// Whether this press carries a select/copy modifier.
    #[inline]
    pub fn command_modifier(self) -> bool {
        self.ctrl || self.meta
    }
}
