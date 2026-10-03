// Single responsibility: Min-content inline width of a shaped run, its widest unbreakable segment.

use crate::text::glyph::ShapedLine;
use smallvec::SmallVec;

/// Widest whitespace-delimited segment of `text`, summed from the glyphs of an unbounded run.
///
/// Glyphs carry the byte index of the cluster they start, so segment widths come from a layout
/// that was already shaped and cached, instead of re-shaping the string once per word. Assumes
/// left-to-right runs; a bidirectional run may under- or over-report by one segment.
pub(crate) fn min_inline_width(text: &str, lines: &[ShapedLine]) -> f32 {
    let segments = segment_ranges(text);
    if segments.is_empty() {
        return 0.0;
    }

    let mut widest = 0.0_f32;
    let mut segment = 0usize;
    let mut running = 0.0_f32;
    for line in lines {
        for glyph in &line.glyphs {
            while segment < segments.len() && glyph.cluster >= segments[segment].1 {
                widest = widest.max(running);
                running = 0.0;
                segment += 1;
            }
            let inside_segment = segment < segments.len() && glyph.cluster >= segments[segment].0;
            if inside_segment {
                running += glyph.advance;
            }
        }
    }
    widest.max(running)
}

/// Byte ranges of the unbreakable segments a line may not be cut inside.
fn segment_ranges(text: &str) -> SmallVec<[(usize, usize); 8]> {
    let mut ranges = SmallVec::new();
    let mut start: Option<usize> = None;

    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(begin) = start.take() {
                ranges.push((begin, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(begin) = start {
        ranges.push((begin, text.len()));
    }
    ranges
}
