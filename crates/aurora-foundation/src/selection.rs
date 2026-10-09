// Single responsibility: Readonly text selection data model (offsets only, no shaping).

use crate::color::Color;

/// Default highlight painted behind selected text, before the glyphs themselves.
pub const DEFAULT_SELECTION_COLOR: Color = Color::rgba(0.0, 0.47, 0.84, 0.30);

/// Width of the caret drawn for a focused collapsed selection.
///
/// One pixel, like a browser caret: it marks a position rather than looking
/// like an editable field.
pub const CARET_WIDTH: f32 = 1.0;

/// Byte-offset range inside one text node's content.
///
/// `anchor` is the fixed press point, `focus` the moving edge, so direction
/// survives re-expansion during a drag. Both snap to `char` boundaries.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub focus: usize,
}

impl TextSelection {
    /// Creates a collapsed caret at an offset.
    pub fn collapsed(offset: usize) -> Self {
        Self {
            anchor: offset,
            focus: offset,
        }
    }

    /// Whether the range selects nothing.
    pub fn is_collapsed(self) -> bool {
        self.anchor == self.focus
    }

    /// Ordered `(start, end)` byte offsets.
    pub fn normalized(self) -> (usize, usize) {
        (self.anchor.min(self.focus), self.anchor.max(self.focus))
    }
}

/// Click-count granularity latched for the active gesture.
///
/// A double-press drag extends by words, a triple-press by paragraph; the
/// latch is what keeps a word-drag from collapsing back to characters.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SelectionGranularity {
    #[default]
    Character,
    Word,
    Paragraph,
}
