// Single responsibility: Overflow behavior and ellipsis truncation for text lines.

use crate::text::layout::TextLayout;
use crate::text::shaper::TextShapeParams;

/// Behavior when text does not fit within its allowed line count.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    /// Draw every line; the parent clips as needed.
    #[default]
    Clip,
    /// Clamp to the line budget and end the last visible line with an ellipsis.
    Ellipsis,
}

/// Clamps an already-shaped layout to `max_lines`, ending the last line with an ellipsis.
///
/// Walks the visible last line's glyph clusters backwards, preferring a word boundary, so
/// truncation never splits a grapheme and normally needs a single extra shaping pass.
pub fn truncate_to_lines(
    params: &TextShapeParams,
    full: TextLayout,
    max_lines: usize,
    shape: impl Fn(&str) -> TextLayout,
) -> TextLayout {
    if max_lines == 0 || full.lines.len() <= max_lines {
        return full;
    }

    let available = if params.constraints.max_width.is_finite() {
        params.constraints.max_width.max(0.0)
    } else {
        f32::INFINITY
    };
    let ellipsis_w = shape("\u{2026}")
        .lines
        .first()
        .map_or(0.0, |line| line.width);

    let last = &full.lines[max_lines - 1];
    let bytes = params.text.as_bytes();
    let mut keep = last.glyphs.len();
    while keep > 0 {
        let kept: f32 = last.glyphs[..keep].iter().map(|g| g.advance).sum();
        if kept + ellipsis_w <= available + 0.01 {
            break;
        }
        keep -= 1;
    }
    // Only drop glyphs to reach a word boundary when the line actually had to shrink.
    if keep < last.glyphs.len() {
        if let Some(pos) = last.glyphs[..keep]
            .iter()
            .rposition(|g| is_whitespace(bytes, g.cluster))
        {
            if pos > 0 {
                keep = pos;
            }
        }
    }

    let end_byte = if keep < last.glyphs.len() {
        last.glyphs[keep].cluster
    } else {
        full.lines[max_lines]
            .glyphs
            .first()
            .map_or(params.text.len(), |g| g.cluster)
    };

    let mut text: String = params.text[..end_byte].trim_end().to_string();
    text.push('\u{2026}');
    let mut layout = shape(&text);
    // Guard against shaping surprises: keep backing off whole words until it fits.
    while layout.lines.len() > max_lines {
        match text[..text.len() - 1].rfind(char::is_whitespace) {
            Some(cut) => {
                text.truncate(cut);
                text.push('\u{2026}');
                layout = shape(&text);
            }
            None => break,
        }
    }
    layout
}

/// Whether the source byte at this cluster offset is ASCII whitespace.
fn is_whitespace(bytes: &[u8], cluster: usize) -> bool {
    matches!(bytes.get(cluster), Some(byte) if byte.is_ascii_whitespace())
}
