// Single responsibility: Line bucketing shared by wrapped flex measurement and placement.
//
// A wrapping container groups its main-axis desired sizes into lines: accumulate
// items plus the main gap until the next item would exceed the available space,
// then start a new line. An item wider than the space alone still occupies its
// own line (the allocator shrinks it to its min-content floor; residual overflow
// paints past the box, exactly like the single-line path).

use smallvec::SmallVec;

use crate::foundation::{
    Alignment, Distribution, Gap, Padding, Point, ResolvedRect,
};
use crate::layout::allocate_cross::resolve_child_cross;
use crate::layout::allocate_main::{allocate_main_axis, MainAxisItems};
use crate::layout::engine::layout_node_with_text;
use crate::layout::measure::{compute_child_desired, compute_child_desired_for_height};
use crate::layout::report::LayoutResult;
use crate::text::TextContext;
use crate::tree::{DirtyFlags, NodeId, TreeArena};
use crate::foundation::Constraints;

/// Index ranges (`start..end`) of children per line, in order.
pub fn plan_lines(desired: &[f32], avail: f32, gap_main: f32) -> Vec<(usize, usize)> {
    let mut lines: Vec<(usize, usize)> = Vec::new();
    if desired.is_empty() {
        return lines;
    }
    // Unbounded: no boundary exists, so everything is one line.
    if !avail.is_finite() {
        return vec![(0, desired.len())];
    }
    let avail = avail.max(0.0);
    let gap = gap_main.max(0.0);
    let mut start = 0usize;
    let mut width = 0.0_f32;
    for (i, d) in desired.iter().enumerate() {
        let add = if i == start { d.max(0.0) } else { gap + d.max(0.0) };
        // A non-empty line that would overflow commits before this item.
        // The `i == start` guard keeps an oversized loner on its own line.
        if i != start && width + add > avail + 0.001 {
            lines.push((start, i));
            start = i;
            width = d.max(0.0);
        } else {
            width += add;
        }
    }
    lines.push((start, desired.len()));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_three_plus_lonely() {
        // avail 100, gap 8: 30+8+30+8+30 = 106 > 100 → [30,30] then [30] + [80]...
        let lines = plan_lines(&[30.0, 30.0, 30.0, 80.0], 100.0, 8.0);
        assert_eq!(lines, vec![(0, 2), (2, 3), (3, 4)]);
    }

    #[test]
    fn oversized_loner_keeps_own_line() {
        let lines = plan_lines(&[200.0, 20.0], 100.0, 8.0);
        assert_eq!(lines, vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn unbounded_is_single_line() {
        let lines = plan_lines(&[200.0, 300.0], f32::INFINITY, 8.0);
        assert_eq!(lines, vec![(0, 2)]);
    }

    #[test]
    fn empty_has_no_lines() {
        assert!(plan_lines(&[], 100.0, 8.0).is_empty());
    }
}

/// Assigns a child's slot rect, marking it for layout when the slot changed.
///
/// Mirrors `engine::place_child`: a repositioned child must re-resolve or
/// grandchildren keep stale rects. Kept local so this module stays
/// self-contained; the two must agree (both mark MEASURE|LAYOUT on change).
fn place_wrapped_child(arena: &mut TreeArena, child: NodeId, rect: ResolvedRect) {
    if arena.get(child).resolved_rect != rect {
        arena.mark_dirty(child, DirtyFlags::MEASURE | DirtyFlags::LAYOUT);
    }
    arena.get_mut(child).resolved_rect = rect;
}

/// Base main-axis offset for one line under the container distribution.
fn line_base(dist: Distribution, avail: f32, total_used: f32) -> f32 {
    if !avail.is_finite() {
        return 0.0;
    }
    match dist {
        Distribution::Center => ((avail - total_used) * 0.5).max(0.0),
        Distribution::End => (avail - total_used).max(0.0),
        _ => 0.0,
    }
}

/// Places the children of a horizontal wrapping container line by line.
///
/// Each line is allocated independently against the full content width (shrink
/// only bites an oversized loner; nothing shrinks to make room for a sibling
/// that belongs on the next line). Cross-axis `Alignment` applies *within* each
/// line band, whose outer height is the tallest child in that line: `Stretch`
/// and `Fill` heights fill the band, not the container. Lines stack with the
/// cross gap. `content_h` is used only for `Percent` heights (resolved against
/// the container, as in the single-line path); pass `INFINITY` when unknown.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_wrapped_horizontal(
    arena: &mut TreeArena,
    normal: &[NodeId],
    desired: &[f32],
    is_fill: &[bool],
    shrink: &[f32],
    margins: &[f32],
    min_sizes: &[f32],
    content_w: f32,
    content_h: f32,
    pad: Padding,
    align: Alignment,
    dist: Distribution,
    gap_main: f32,
    gap_cross: f32,
    text_ctx: &TextContext,
    result: &mut LayoutResult,
    node_abs: Point,
) -> f32 {
    let lines = plan_lines(desired, content_w, gap_main);
    let mut line_y = 0.0_f32;
    let mut last_end = 0.0_f32;
    for (li, &(s, e)) in lines.iter().enumerate() {
        let n = e - s;
        let plan = allocate_main_axis(
            content_w,
            Gap::Fixed(gap_main),
            dist,
            &MainAxisItems {
                desired: &desired[s..e],
                is_fill: &is_fill[s..e],
                shrink: &shrink[s..e],
                margins: &margins[s..e],
                min_sizes: &min_sizes[s..e],
            },
        );
        // Band from Fit/Fixed heights; Fill takes leftover and never drives it.
        let mut band_outer = 0.0_f32;
        let mut intrinsics: SmallVec<[f32; 8]> = SmallVec::with_capacity(n);
        for k in 0..n {
            let idx = s + k;
            let child = normal[idx];
            let (m_top, m_bottom, height_intent) = {
                let c = arena.get(child);
                (
                    c.style.margin.top,
                    c.style.margin.bottom,
                    c.style.height,
                )
            };
            let intrinsic = if height_intent.is_fit() {
                let assigned_w = (plan.sizes[k] - margins[idx]).max(0.0);
                compute_child_desired(
                    arena,
                    child,
                    Constraints::tight_width(assigned_w),
                    None,
                    None,
                    text_ctx,
                )
                .height
            } else {
                0.0
            };
            intrinsics.push(intrinsic);
            let drive = match height_intent {
                crate::foundation::Size::Fit => intrinsic + m_top + m_bottom,
                crate::foundation::Size::Fixed(v) => v.max(0.0) + m_top + m_bottom,
                crate::foundation::Size::Percent(r) if content_h.is_finite() => {
                    (content_h * r).max(0.0)
                }
                _ => 0.0,
            };
            band_outer = band_outer.max(drive);
        }
        let total_used = if n > 0 {
            plan.sizes.iter().sum::<f32>() + (n - 1) as f32 * plan.gap_px
        } else {
            0.0
        };
        let mut cursor = line_base(dist, content_w, total_used);
        let line_start = cursor;
        for k in 0..n {
            let idx = s + k;
            let child = normal[idx];
            let (m_start, m_top, m_bottom, height_intent) = {
                let c = arena.get(child);
                (
                    c.style.margin.left,
                    c.style.margin.top,
                    c.style.margin.bottom,
                    c.style.height,
                )
            };
            // Percent resolves against the container; everything else bands.
            let parent = match height_intent {
                crate::foundation::Size::Percent(_) => content_h,
                _ => band_outer,
            };
            let (content, offset) = resolve_child_cross(
                align,
                parent,
                height_intent,
                intrinsics[k],
                m_top,
                m_bottom,
            );
            let rect = ResolvedRect::new(
                pad.left + cursor + m_start,
                pad.top + line_y + offset,
                (plan.sizes[k] - margins[idx]).max(0.0),
                content.max(0.0),
            );
            cursor += plan.sizes[k] + plan.gap_px;
            place_wrapped_child(arena, child, rect);
            layout_node_with_text(
                arena,
                child,
                Constraints::tight(rect.width, rect.height),
                text_ctx,
                result,
                node_abs,
            );
        }
        line_y += band_outer;
        if li + 1 < lines.len() {
            line_y += gap_cross;
        }
        last_end = line_start + total_used;
    }
    last_end
}

/// Places the children of a vertical wrapping container column by column.
///
/// Transpose of [`layout_wrapped_horizontal`]: children bucket along the main
/// (vertical) axis into columns, each column's outer width is its widest
/// child, columns advance left-to-right separated by the cross gap, and
/// `Alignment` applies within each column band. `content_w` serves `Percent`
/// widths only; pass `INFINITY` when unknown.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_wrapped_vertical(
    arena: &mut TreeArena,
    normal: &[NodeId],
    desired: &[f32],
    is_fill: &[bool],
    shrink: &[f32],
    margins: &[f32],
    min_sizes: &[f32],
    content_w: f32,
    content_h: f32,
    pad: Padding,
    align: Alignment,
    dist: Distribution,
    gap_main: f32,
    gap_cross: f32,
    text_ctx: &TextContext,
    result: &mut LayoutResult,
    node_abs: Point,
) -> f32 {
    let cols = plan_lines(desired, content_h, gap_main);
    let mut col_x = 0.0_f32;
    let mut last_end = 0.0_f32;
    for (ci, &(s, e)) in cols.iter().enumerate() {
        let n = e - s;
        let plan = allocate_main_axis(
            content_h,
            Gap::Fixed(gap_main),
            dist,
            &MainAxisItems {
                desired: &desired[s..e],
                is_fill: &is_fill[s..e],
                shrink: &shrink[s..e],
                margins: &margins[s..e],
                min_sizes: &min_sizes[s..e],
            },
        );
        // Column band from Fit/Fixed widths; Fill never drives it.
        let mut band_outer = 0.0_f32;
        let mut intrinsics: SmallVec<[f32; 8]> = SmallVec::with_capacity(n);
        for k in 0..n {
            let idx = s + k;
            let child = normal[idx];
            let (m_left, m_right, width_intent) = {
                let c = arena.get(child);
                (c.style.margin.left, c.style.margin.right, c.style.width)
            };
            // Column bands size content (wrapped role): nested text wraps at
            // the offered width instead of widening past the container.
            let intrinsic = if width_intent.is_fit() {
                compute_child_desired_for_height(
                    arena,
                    child,
                    Constraints::loose(content_w, content_h),
                    Some(content_w),
                    Some(content_h),
                    text_ctx,
                )
                .width
            } else {
                0.0
            };
            intrinsics.push(intrinsic);
            let drive = match width_intent {
                crate::foundation::Size::Fit => intrinsic + m_left + m_right,
                crate::foundation::Size::Fixed(v) => v.max(0.0) + m_left + m_right,
                crate::foundation::Size::Percent(r) if content_w.is_finite() => {
                    (content_w * r).max(0.0)
                }
                _ => 0.0,
            };
            band_outer = band_outer.max(drive);
        }
        let total_used = if n > 0 {
            plan.sizes.iter().sum::<f32>() + (n - 1) as f32 * plan.gap_px
        } else {
            0.0
        };
        let mut cursor = line_base(dist, content_h, total_used);
        let col_start = cursor;
        for k in 0..n {
            let idx = s + k;
            let child = normal[idx];
            let (m_top, m_left, m_right, width_intent) = {
                let c = arena.get(child);
                (
                    c.style.margin.top,
                    c.style.margin.left,
                    c.style.margin.right,
                    c.style.width,
                )
            };
            let parent = match width_intent {
                crate::foundation::Size::Percent(_) => content_w,
                _ => band_outer,
            };
            // `resolve_child_cross` is axis-agnostic: parent band, intent,
            // intrinsic, margins in axis order.
            let (content, offset) = resolve_child_cross(
                align,
                parent,
                width_intent,
                intrinsics[k],
                m_left,
                m_right,
            );
            let rect = ResolvedRect::new(
                pad.left + col_x + offset,
                pad.top + cursor + m_top,
                content.max(0.0),
                (plan.sizes[k] - margins[idx]).max(0.0),
            );
            cursor += plan.sizes[k] + plan.gap_px;
            place_wrapped_child(arena, child, rect);
            layout_node_with_text(
                arena,
                child,
                Constraints::tight(rect.width, rect.height),
                text_ctx,
                result,
                node_abs,
            );
        }
        col_x += band_outer;
        if ci + 1 < cols.len() {
            col_x += gap_cross;
        }
        last_end = col_start + total_used;
    }
    last_end
}
