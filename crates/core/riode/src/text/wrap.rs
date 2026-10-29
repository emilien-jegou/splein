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


use std::str::FromStr;

fn parse_text_wrap(s: &str) -> Option<TextWrap> {
    match s.trim().to_lowercase().as_str() {
        "wrap" | "normal" | "prewrap" => Some(TextWrap::Wrap),
        "nowrap" | "no" | "none" => Some(TextWrap::NoWrap),
        "balance" => Some(TextWrap::Balance),
        _ => None,
    }
}

impl FromStr for TextWrap {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        parse_text_wrap(s).ok_or_else(|| "bad text_wrap".to_string())
    }
}
impl From<&str> for TextWrap {
    fn from(s: &str) -> Self {
        parse_text_wrap(s).unwrap_or(TextWrap::Wrap)
    }
}
impl From<String> for TextWrap {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
