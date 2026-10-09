// Single responsibility: Readonly text selection model and hit queries.
//
// Covers the mouse-only, opt-in model: byte-offset caret mapping from a shaped
// layout, word expansion for double-click, per-line highlight rectangles, and
// the document-order helpers the router uses to span a range across runs.
// Keyboard and clipboard are deliberately out of scope.

use crate::layout::TextLayout;
use aurora_foundation::{Point, ResolvedRect};
use unicode_segmentation::UnicodeSegmentation;

pub use aurora_foundation::{
    SelectionGranularity, TextSelection, CARET_WIDTH, DEFAULT_SELECTION_COLOR,
};

/// Whether a character belongs to a double-click word run.
///
/// Letters, digits, and underscore form words; CJK ideographs report as
/// alphanumeric so a CJK run selects as one word for now (UAX #29 segmentation
/// is the follow-up). Whitespace and punctuation break words.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether a UAX#29 chunk counts as a word stop for keyboard jumps.
///
/// Only alphanumeric runs (plus underscore) stop the caret; whitespace and
/// punctuation are skipped over, matching Ctrl+Arrow behavior.
fn is_word_chunk(chunk: &str) -> bool {
    chunk
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Snaps a byte offset to a valid `char` boundary, clamping to the content.
pub fn snap_to_boundary(content: &str, offset: usize) -> usize {
    let offset = offset.min(content.len());
    if content.is_char_boundary(offset) {
        return offset;
    }
    // Walk forward to the next boundary; the string end always is one.
    let mut next = offset + 1;
    while next < content.len() && !content.is_char_boundary(next) {
        next += 1;
    }
    next.min(content.len())
}

/// Word (or whitespace/punctuation run) containing `offset`, per UAX #29.
///
/// `"don't"` stays one word and CJK/emoji segment by the standard rules.
/// Double-clicking whitespace selects the whitespace run, punctuation the
/// punctuation run — *something* rather than nothing.
pub fn word_bounds(content: &str, offset: usize) -> (usize, usize) {
    if content.is_empty() {
        return (0, 0);
    }
    let offset = snap_to_boundary(content, offset);
    let mut last = (0, 0);
    for (start, chunk) in content.split_word_bound_indices() {
        let end = start + chunk.len();
        last = (start, end);
        if offset >= start && offset < end {
            return (start, end);
        }
    }
    // A caret at the very end belongs to the final chunk.
    last
}

/// Byte offset of every grapheme boundary, ending with the content length.
pub fn grapheme_boundaries(content: &str) -> Vec<usize> {
    let mut out: Vec<usize> = content.grapheme_indices(true).map(|(i, _)| i).collect();
    out.push(content.len());
    out
}

/// Steps one grapheme cluster left (`dir < 0`) or right.
///
/// From inside a cluster the caret first snaps to that cluster's edge, so
/// emoji ZWJ sequences and combining marks are never split.
pub fn move_grapheme(content: &str, offset: usize, dir: i8) -> usize {
    let offset = snap_to_boundary(content, offset);
    let bounds = grapheme_boundaries(content);
    if dir < 0 {
        bounds
            .iter()
            .rev()
            .find(|&&b| b < offset)
            .copied()
            .unwrap_or(0)
    } else {
        bounds
            .iter()
            .find(|&&b| b > offset)
            .copied()
            .unwrap_or(content.len())
    }
}

/// Jumps to the next word edge left (`dir < 0`) or right, for Ctrl+Arrow.
///
/// From inside a word the caret stops at that word's edge; otherwise it skips
/// whitespace and punctuation to the neighbouring word. Falls back to the
/// content ends when no word lies in that direction.
pub fn word_edge(content: &str, offset: usize, dir: i8) -> usize {
    let offset = snap_to_boundary(content, offset);
    let chunks: Vec<(usize, usize, bool)> = content
        .split_word_bound_indices()
        .map(|(s, chunk)| (s, s + chunk.len(), is_word_chunk(chunk)))
        .collect();
    if chunks.is_empty() {
        return 0;
    }
    if dir < 0 {
        // The word at or before the caret: from inside a word (not at its
        // start) stop at its start, else take the previous word's start.
        let mut prev_start = 0;
        for &(s, e, is_word) in &chunks {
            if !is_word {
                continue;
            }
            if offset > s && offset <= e {
                return if offset == s { prev_start } else { s };
            }
            if s >= offset {
                return prev_start;
            }
            prev_start = s;
        }
        prev_start
    } else {
        let mut next_end = content.len();
        for &(s, e, is_word) in chunks.iter().rev() {
            if !is_word {
                continue;
            }
            if offset >= s && offset < e {
                return if offset == e { next_end } else { e };
            }
            if e <= offset {
                return next_end;
            }
            next_end = e;
        }
        next_end
    }
}

/// Byte range of every visual line, derived from glyph clusters.
///
/// `starts[i]` is the smallest cluster in the line; ends chain into the next
/// line's start (or the content length), which keeps wrapped lines contiguous
/// without needing the forgotton cosmic line text.
pub fn line_byte_ranges(layout: &TextLayout, content: &str) -> Vec<(usize, usize)> {
    let total = content.len();
    if layout.lines.is_empty() {
        return vec![];
    }
    let mut starts = Vec::with_capacity(layout.lines.len());
    for (i, line) in layout.lines.iter().enumerate() {
        let min_cluster = line.glyphs.iter().map(|g| g.cluster).min();
        let start = match min_cluster {
            Some(c) => c.min(total),
            None => {
                // An empty line (bare newline) starts where the previous ends.
                if i == 0 {
                    0
                } else {
                    starts[i - 1]
                }
            }
        };
        starts.push(start);
    }
    // Repair non-monotonic starts (empty lines, zero-width runs): each line
    // starts at or after the previous one.
    for i in 1..starts.len() {
        if starts[i] < starts[i - 1] {
            starts[i] = starts[i - 1];
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let e = if i + 1 < starts.len() {
                starts[i + 1].max(s)
            } else {
                total.max(s)
            };
            (s, e)
        })
        .collect()
}

/// Visual line index containing a layout-local y.
///
/// Line `i` owns `[i * height, (i + 1) * height)`; shaping stacks uniform
/// line boxes from zero, so the top needs no ascent bookkeeping.
fn line_at_y(layout: &TextLayout, y: f32) -> usize {
    if layout.lines.is_empty() {
        return 0;
    }
    let height = layout.lines[0].height.max(1.0);
    if y <= 0.0 {
        return 0;
    }
    ((y / height) as usize).min(layout.lines.len() - 1)
}

/// X of a glyph's left edge in layout-local coordinates.
#[inline]
fn glyph_x(line: &crate::glyph::ShapedLine, glyph: &crate::glyph::ShapedGlyph) -> f32 {
    line.align_offset + glyph.point.x
}

/// Caret x for a byte offset inside one line: the left edge of the first glyph
/// at or after the offset, else the line end.
fn caret_x_in_line(
    line: &crate::glyph::ShapedLine,
    line_start: usize,
    line_end: usize,
    offset: usize,
) -> f32 {
    let offset = offset.clamp(line_start, line_end);
    for glyph in &line.glyphs {
        if glyph.cluster >= offset {
            return glyph_x(line, glyph);
        }
    }
    // Past the last glyph: end of line.
    line.glyphs
        .last()
        .map(|g| glyph_x(line, g) + g.advance)
        .unwrap_or(line.align_offset)
}

/// Maps a layout-local point to a byte offset (browser midpoint rule).
///
/// The pointer lands *between* glyphs: past a glyph's midpoint the caret goes
/// after it. Y picks the line first, so a drag sweeping down-right lands on
/// the next line's start rather than clinging to the previous line's end.
pub fn caret_at_point(layout: &TextLayout, content: &str, local: Point) -> usize {
    let total = content.len();
    if layout.lines.is_empty() || total == 0 {
        return 0;
    }
    let ranges = line_byte_ranges(layout, content);
    let idx = line_at_y(layout, local.y);
    let line = &layout.lines[idx];
    let (line_start, line_end) = ranges[idx];
    if line.glyphs.is_empty() {
        return snap_to_boundary(content, line_start);
    }
    // Left of the first glyph: line start. Right of the last glyph's end:
    // line end. Both clamp vertical overshoot onto the nearest line.
    let first_x = glyph_x(line, &line.glyphs[0]);
    if local.x <= first_x {
        return snap_to_boundary(content, line_start);
    }
    for glyph in &line.glyphs {
        let start = glyph_x(line, glyph);
        let mid = start + glyph.advance * 0.5;
        if local.x < mid {
            return snap_to_boundary(content, glyph.cluster.min(line_end).max(line_start));
        }
    }
    // Past every midpoint: the next cluster boundary, i.e. line end. Using the
    // line end (not last cluster + 1) keeps multi-byte glyphs exact.
    snap_to_boundary(content, line_end)
}

/// Expands an anchor/focus pair by granularity.
pub fn expand_selection(
    content: &str,
    anchor: usize,
    focus: usize,
    granularity: SelectionGranularity,
) -> TextSelection {
    let anchor = snap_to_boundary(content, anchor);
    let focus = snap_to_boundary(content, focus);
    match granularity {
        SelectionGranularity::Character => TextSelection { anchor, focus },
        SelectionGranularity::Word => {
            if focus >= anchor {
                let (s, _) = word_bounds(content, anchor);
                let (_, e) = word_bounds(content, focus);
                TextSelection { anchor: s, focus: e }
            } else {
                let (_, e) = word_bounds(content, anchor);
                let (s, _) = word_bounds(content, focus);
                TextSelection { anchor: e, focus: s }
            }
        }
        SelectionGranularity::Paragraph => {
            // One single-style leaf is one paragraph.
            if focus >= anchor {
                TextSelection {
                    anchor: 0,
                    focus: content.len(),
                }
            } else {
                TextSelection {
                    anchor: content.len(),
                    focus: 0,
                }
            }
        }
    }
}

/// Visual line index and caret x for a byte offset.
///
/// Boundary offsets resolve downstream: the end of one wrapped line is the
/// start of the next, so the caret prefers the line it visually continues on.
/// An explicit newline byte belongs to the line it terminates.
pub fn caret_line_col(
    layout: &TextLayout,
    content: &str,
    offset: usize,
) -> Option<(usize, f32)> {
    if layout.lines.is_empty() {
        return None;
    }
    let offset = snap_to_boundary(content, offset);
    let ranges = line_byte_ranges(layout, content);
    let mut idx = ranges.len() - 1;
    for (i, &(_, le)) in ranges.iter().enumerate() {
        if offset < le {
            idx = i;
            break;
        }
    }
    let line = &layout.lines[idx];
    let (ls, le) = ranges[idx];
    Some((idx, caret_x_in_line(line, ls, le, offset)))
}

/// Caret rectangle for an offset: full line-box height, `CARET_WIDTH` wide.
pub fn caret_rect(
    layout: &TextLayout,
    content: &str,
    offset: usize,
) -> Option<ResolvedRect> {
    let (idx, x) = caret_line_col(layout, content, offset)?;
    let height = layout.lines[idx].height;
    Some(ResolvedRect::new(x, idx as f32 * height, CARET_WIDTH, height))
}

/// Byte offset on a visual line at a goal x (for Up/Down movement).
pub fn offset_at_visual_line(
    layout: &TextLayout,
    content: &str,
    line_idx: usize,
    x: f32,
) -> Option<usize> {
    if layout.lines.is_empty() {
        return None;
    }
    let idx = line_idx.min(layout.lines.len() - 1);
    let height = layout.lines[idx].height.max(1.0);
    Some(caret_at_point(
        layout,
        content,
        Point::new(x, idx as f32 * height + height * 0.5),
    ))
}
/// Per-line highlight rectangles for a selection, in layout-local coordinates.
///
/// One rect per overlapped line (never a single spanning box), full line-box
/// height, collapsed selections producing nothing. Callers paint these
/// *behind* the glyphs.
pub fn selection_rects(
    layout: &TextLayout,
    content: &str,
    selection: &TextSelection,
) -> Vec<ResolvedRect> {
    let (start, end) = selection.normalized();
    if start >= end || layout.lines.is_empty() {
        return vec![];
    }
    let ranges = line_byte_ranges(layout, content);
    let mut rects = Vec::new();
    for (i, line) in layout.lines.iter().enumerate() {
        let (ls, le) = ranges[i];
        if end <= ls || start >= le {
            continue;
        }
        let x0 = caret_x_in_line(line, ls, le, start.max(ls));
        let x1 = caret_x_in_line(line, ls, le, end.min(le));
        if x1 > x0 {
            let top = i as f32 * line.height;
            rects.push(ResolvedRect::new(x0, top, x1 - x0, line.height));
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_foundation::Constraints;
    use crate::TextContext;

    fn context() -> TextContext {
        let ctx = TextContext::new();
        ctx.load_font(include_bytes!("../assets/inter-var.ttf"));
        ctx
    }

    fn shape(content: &str, width: f32) -> (TextContext, std::sync::Arc<TextLayout>) {
        let ctx = context();
        let cfg = crate::TextConfig {
            content: content.to_string(),
            size: 16.0,
            ..crate::TextConfig::default()
        };
        let layout = ctx.shape_config(&cfg, Constraints::loose(width, f32::INFINITY));
        (ctx, layout)
    }

    #[test]
    fn caret_starts_and_ends_at_edges() {
        let (_ctx, layout) = shape("hello", 400.0);
        assert_eq!(layout.lines.len(), 1);
        let start = caret_at_point(&layout, "hello", Point::new(-100.0, 5.0));
        assert_eq!(start, 0);
        let end = caret_at_point(&layout, "hello", Point::new(10000.0, 5.0));
        assert_eq!(end, 5);
    }

    #[test]
    fn caret_monotonic_across_line() {
        let (_ctx, layout) = shape("hello world", 400.0);
        let mut last = 0;
        let mut x = 0.0;
        while x < 400.0 {
            let c = caret_at_point(&layout, "hello world", Point::new(x, 5.0));
            assert!(c >= last, "caret moved backwards at x={x}");
            last = c;
            x += 2.0;
        }
        assert_eq!(last, "hello world".len());
    }

    #[test]
    fn caret_picks_line_by_y() {
        let content = "Aurora wraps this paragraph to the box width and keeps rhythm";
        let (_ctx, layout) = shape(content, 160.0);
        assert!(layout.lines.len() > 1);
        let h = layout.lines[0].height;
        let ranges = line_byte_ranges(&layout, content);
        let first = caret_at_point(&layout, content, Point::new(4.0, h * 0.5));
        let second = caret_at_point(&layout, content, Point::new(4.0, h * 1.5));
        assert_eq!(first, ranges[0].0);
        assert_eq!(second, ranges[1].0);
        // Below the last line clamps to the last line, then maps x normally:
        // far right lands at the content end, an interior x inside it.
        let below_end = caret_at_point(&layout, content, Point::new(10000.0, h * 50.0));
        assert_eq!(below_end, content.len());
        let below_inside = caret_at_point(&layout, content, Point::new(4.0, h * 50.0));
        let last_range = ranges[ranges.len() - 1];
        assert!(
            below_inside >= last_range.0 && below_inside <= last_range.1,
            "below-line click stays on the last line"
        );
    }

    #[test]
    fn word_bounds_select_words_and_runs() {
        assert_eq!(word_bounds("hello world", 1), (0, 5));
        assert_eq!(word_bounds("hello world", 7), (6, 11));
        // Whitespace selects the whitespace run.
        assert_eq!(word_bounds("a  b", 1), (1, 3));
        // Empty content is safe.
        assert_eq!(word_bounds("", 0), (0, 0));
        // End of content probes the last word.
        assert_eq!(word_bounds("hello", 5), (0, 5));
        // UAX#29 keeps contractions together.
        assert_eq!(word_bounds("don't stop", 1), (0, 5));
    }

    #[test]
    fn grapheme_steps_never_split_clusters() {
        let content = "a🇫🇷b";
        assert_eq!(move_grapheme(content, 0, 1), 1);
        // The flag is one grapheme: stepping right from inside it exits whole.
        assert_eq!(move_grapheme(content, 2, 1), content.len() - 1);
        assert_eq!(move_grapheme(content, content.len(), -1), content.len() - 1);
        assert_eq!(move_grapheme(content, 1, -1), 0);
        assert_eq!(move_grapheme(content, 0, -1), 0);
    }

    #[test]
    fn word_edges_jump_words() {
        let content = "hello  brave world";
        // Inside a word: stop at its edge.
        assert_eq!(word_edge(content, 2, -1), 0);
        assert_eq!(word_edge(content, 2, 1), 5);
        // At a word start going left: previous word start, skipping spaces.
        assert_eq!(word_edge(content, 7, -1), 0);
        // From a word end going right: next word end, skipping spaces.
        assert_eq!(word_edge(content, 5, 1), 12);
        // Ends clamp to the content.
        assert_eq!(word_edge(content, 0, -1), 0);
        assert_eq!(word_edge(content, content.len(), 1), content.len());
    }

    #[test]
    fn caret_rect_tracks_offset() {
        let content = "Aurora wraps this paragraph to the box width and keeps rhythm";
        let (_ctx, layout) = shape(content, 160.0);
        let h = layout.lines[0].height;
        let ranges = line_byte_ranges(&layout, content);
        let r0 = caret_rect(&layout, content, ranges[0].0).expect("caret");
        assert_eq!((r0.y, r0.height), (0.0, h));
        assert_eq!(r0.width, CARET_WIDTH);
        let r1 = caret_rect(&layout, content, ranges[1].0).expect("caret");
        assert_eq!((r1.y, r1.height), (h, h));
        // Boundary offsets resolve downstream onto the next line.
        let (idx, _) = caret_line_col(&layout, content, ranges[1].0).expect("line");
        assert_eq!(idx, 1);
        // Up/Down round-trips through a goal x.
        let (_, x) = caret_line_col(&layout, content, ranges[0].0 + 2).expect("line");
        let down = offset_at_visual_line(&layout, content, 1, x).expect("offset");
        let back = offset_at_visual_line(&layout, content, 0, x).expect("offset");
        let (ri, _) = caret_line_col(&layout, content, down).expect("line");
        assert_eq!(ri, 1);
        let (r0i, _) = caret_line_col(&layout, content, back).expect("line");
        assert_eq!(r0i, 0);
    }

    #[test]
    fn selection_rects_span_lines() {
        let content = "Aurora wraps this paragraph to the box width and keeps rhythm";
        let (_ctx, layout) = shape(content, 160.0);
        assert!(layout.lines.len() > 1);
        let full = TextSelection {
            anchor: 0,
            focus: content.len(),
        };
        let rects = selection_rects(&layout, content, &full);
        assert_eq!(rects.len(), layout.lines.len());
        for r in &rects {
            assert!(r.width > 0.0 && r.height > 0.0);
        }
        // Collapsed selection highlights nothing.
        let collapsed = TextSelection::collapsed(3);
        assert!(selection_rects(&layout, content, &collapsed).is_empty());
        // Partial range touches a subset of lines.
        let ranges = line_byte_ranges(&layout, content);
        let partial = TextSelection {
            anchor: ranges[0].0,
            focus: ranges[0].1,
        };
        let rects = selection_rects(&layout, content, &partial);
        assert_eq!(rects.len(), 1);
    }
}
