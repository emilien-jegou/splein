// Single responsibility: Text wrapping policy — wrap, refuse, or balance.

/// How a text run breaks lines against its inline width.
///
/// - `Wrap` (default): greedy break at word boundaries; emergency mid-word
///   breaks only when a single word exceeds the line (Parley
///   `OverflowWrap::Anywhere`, unchanged).
/// - `NoWrap`: never break at soft opportunities (Parley `TextWrapMode::NoWrap`).
///   The run keeps its max-content width; the parent caps the box and paint
///   clips or ellipsizes. Min-content equals max-content.
/// - `Balance`: for headings / short blocks (≤6 lines). Shapes at candidate
///   widths between min-content and the available width and keeps the break
///   with the most even line lengths. Parley 0.11 has no balance mode, so
///   this is implemented in `text/shape.rs` over the existing shaper + cache.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    #[default]
    Wrap,
    NoWrap,
    Balance,
}

impl TextWrap {
    pub fn is_balance(self) -> bool {
        matches!(self, TextWrap::Balance)
    }

    pub fn is_nowrap(self) -> bool {
        matches!(self, TextWrap::NoWrap)
    }
}
