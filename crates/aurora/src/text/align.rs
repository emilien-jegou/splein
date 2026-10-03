// Single responsibility: Horizontal paragraph alignment (start, center, end, justify).

use crate::text::glyph::ShapedLine;

/// Horizontal alignment of text lines within the paragraph box.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    /// Lines flush to the leading edge.
    #[default]
    Start,
    /// Lines centered within the box.
    Center,
    /// Lines flush to the trailing edge.
    End,
    /// Inter-word gaps stretched so every line but the last fills the box.
    Justify,
}

/// Applies alignment to shaped lines, storing per-line offsets instead of moving glyphs.
///
/// Center and End record a constant `align_offset`; only Justify mutates glyph positions,
/// since its slack is distributed across inter-word gaps rather than the whole line.
pub fn apply_alignment(lines: &mut [ShapedLine], box_width: f32, align: TextAlign, text: &str) {
    let last = lines.len().saturating_sub(1);
    for (index, line) in lines.iter_mut().enumerate() {
        line.align_offset = 0.0;
        if align == TextAlign::Start || !box_width.is_finite() {
            continue;
        }
        let slack = (box_width - line.width).max(0.0);
        if slack <= 0.0 {
            continue;
        }
        match align {
            TextAlign::Start => {}
            TextAlign::Center => line.align_offset = slack * 0.5,
            TextAlign::End => line.align_offset = slack,
            TextAlign::Justify => {
                if index != last {
                    justify(line, slack, text);
                }
            }
        }
    }
}

/// Distributes slack across inter-word gaps, leaving the words themselves intact.
fn justify(line: &mut ShapedLine, slack: f32, text: &str) {
    let bytes = text.as_bytes();
    let gaps = line
        .glyphs
        .iter()
        .filter(|g| is_whitespace(bytes, g.cluster))
        .count();
    if gaps == 0 {
        return;
    }
    let per_gap = slack / gaps as f32;
    let mut extra = 0.0f32;
    for glyph in &mut line.glyphs {
        glyph.point.x += extra;
        if is_whitespace(bytes, glyph.cluster) {
            extra += per_gap;
        }
    }
}

/// Whether the source byte at this cluster offset is ASCII whitespace.
fn is_whitespace(bytes: &[u8], cluster: usize) -> bool {
    matches!(bytes.get(cluster), Some(byte) if byte.is_ascii_whitespace())
}
