// Single responsibility: Overflow behavior and ellipsis truncation for text lines.

use crate::layout::TextLayout;
use crate::shaper::TextShapeParams;

/// Behavior when text does not fit within its allowed line count.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    /// Draw every line; the parent clips as needed.
    #[default]
    Clip,
    /// Clamp to the line budget and end the last visible line with an ellipsis.
    Ellipsis,
}

/// Upper bound on word-backoff re-shapes: each pass drops at least one word,
/// so this caps pathological multi-shape loops while staying far above any
/// realistic word count on one line.
const MAX_BACKOFF_SHAPES: usize = 8;

/// Clamps an already-shaped layout to `max_lines`, ending the last line with an ellipsis.
///
/// Walks the visible last line's glyph clusters backwards, preferring a word boundary, so
/// truncation never splits a grapheme and normally needs a single extra shaping pass.
pub fn truncate_to_lines(
    params: &TextShapeParams,
    full: TextLayout,
    max_lines: usize,
    mut shape: impl FnMut(&str) -> TextLayout,
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
    // Running prefix widths: one O(g) pass instead of re-summing the prefix
    // on every shrink step.
    let mut prefix = Vec::with_capacity(last.glyphs.len() + 1);
    prefix.push(0.0f32);
    for g in &last.glyphs {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + g.advance);
    }
    let mut keep = last.glyphs.len();
    while keep > 0 {
        if prefix[keep] + ellipsis_w <= available + 0.01 {
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
    // Guard against shaping surprises: keep backing off whole words until it
    // fits, bounded so a pathological breaker cannot loop reshaping forever.
    // Each pass drops at least one word, so the bound only bites when the
    // breaker disagrees with the advance sums above.
    let mut backoffs = 0;
    while layout.lines.len() > max_lines && backoffs < MAX_BACKOFF_SHAPES {
        match text[..text.len() - 1].rfind(char::is_whitespace) {
            Some(cut) => {
                text.truncate(cut);
                text.push('\u{2026}');
                layout = shape(&text);
                backoffs += 1;
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
