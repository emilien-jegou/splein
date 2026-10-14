// Single responsibility: Text decoration flags and underline/strike/overline geometry.

use crate::foundation::ResolvedRect;
use crate::text::glyph::ShapedLine;
use std::ops::BitOr;

/// Set of decoration lines drawn under or over a text run.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextDecoration(pub u8);

impl TextDecoration {
    /// No decoration.
    pub const NONE: Self = Self(0);
    /// Underline below the baseline.
    pub const UNDERLINE: Self = Self(1);
    /// Strikethrough through the middle of the run.
    pub const LINE_THROUGH: Self = Self(2);
    /// Overline above the ascenders.
    pub const OVERLINE: Self = Self(4);

    /// Whether this set contains all flags in `other`.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no decoration flags are set.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for TextDecoration {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Rectangles for each enabled decoration line of a shaped line, in local space.
pub fn decoration_rules(
    line: &ShapedLine,
    size: f32,
    decoration: TextDecoration,
) -> Vec<ResolvedRect> {
    let Some((start, end)) = line_extent(line) else {
        return Vec::new();
    };
    let width = end - start;
    if width <= 0.0 {
        return Vec::new();
    }

    let thickness = (size * 0.06).max(1.0);
    let mut rects = Vec::new();
    if decoration.contains(TextDecoration::UNDERLINE) {
        rects.push(ResolvedRect::new(
            start,
            line.baseline + size * 0.10,
            width,
            thickness,
        ));
    }
    if decoration.contains(TextDecoration::LINE_THROUGH) {
        rects.push(ResolvedRect::new(
            start,
            line.baseline - size * 0.28,
            width,
            thickness,
        ));
    }
    if decoration.contains(TextDecoration::OVERLINE) {
        rects.push(ResolvedRect::new(
            start,
            line.baseline - size * 0.80,
            width,
            thickness,
        ));
    }
    rects
}

/// Horizontal visual extent of a line derived from its glyph placements.
fn line_extent(line: &ShapedLine) -> Option<(f32, f32)> {
    let mut start = f32::INFINITY;
    let mut end = f32::NEG_INFINITY;
    for glyph in &line.glyphs {
        start = start.min(line.align_offset + glyph.point.x);
        end = end.max(line.align_offset + glyph.point.x + glyph.advance);
    }
    if start.is_finite() && end.is_finite() {
        Some((start, end))
    } else {
        None
    }
}
