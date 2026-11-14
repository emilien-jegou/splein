// Single responsibility: Min-content size of a node along one axis, the CSS `min-*: auto` shrink floor.

use smallvec::SmallVec;

use crate::foundation::{Direction, FlowWrap};
use crate::text::TextContext;
use crate::tree::cache::LayoutCache;
use crate::tree::TreeArena;
use crate::tree::NodeId;

pub fn compute_min_size(
    arena: &mut TreeArena,
    id: NodeId,
    axis: Direction,
    text_ctx: &TextContext,
) -> f32 {
    if let Some(cached) = cached_min_size(&arena.get(id).state.core.cache, axis) {
        return cached;
    }

    let measured = measure_min_size(arena, id, axis, text_ctx).max(0.0);
    match axis {
        Direction::Horizontal => arena.get_mut(id).state.core.cache.min_inline = Some(measured),
        Direction::Vertical => arena.get_mut(id).state.core.cache.min_block = Some(measured),
    }
    measured
}

fn cached_min_size(cache: &LayoutCache, axis: Direction) -> Option<f32> {
    match axis {
        Direction::Horizontal => cache.min_inline,
        Direction::Vertical => cache.min_block,
    }
}

fn measure_min_size(
    arena: &mut TreeArena,
    id: NodeId,
    axis: Direction,
    text_ctx: &TextContext,
) -> f32 {
    if arena.get(id).style.clip {
        return 0.0;
    }

    // Encapsulated polymorphic dispatch: no leaking of leaf variants
    if let Some(floor) = arena.get(id).kind.min_content(axis, text_ctx) {
        let pad = arena.get(id).style.padding;
        return floor
            + match axis {
                Direction::Horizontal => pad.horizontal(),
                Direction::Vertical => pad.vertical(),
            };
    }

    min_group_size(arena, id, axis, text_ctx)
}

fn min_group_size(
    arena: &mut TreeArena,
    id: NodeId,
    axis: Direction,
    text_ctx: &TextContext,
) -> f32 {
    let (own_dir, gap, wrapping) = {
        let layout = arena.get(id).style.layout;
        (layout.direction, layout.gap, layout.flow_wrap == FlowWrap::Wrap)
    };

    let flow: SmallVec<[NodeId; 8]> = arena
        .each_child(id)
        .filter(|child| {
            !arena.get(*child).style.is_absolute && arena.get(*child).state.core.presence.occupies_space()
        })
        .collect();
    if flow.is_empty() {
        let pad = arena.get(id).style.padding;
        return match axis {
            Direction::Horizontal => pad.horizontal(),
            Direction::Vertical => pad.vertical(),
        };
    }

    let mut sum = 0.0_f32;
    let mut widest = 0.0_f32;
    for child in &flow {
        let child_min = compute_min_size(arena, *child, axis, text_ctx);
        let margins = match axis {
            Direction::Horizontal => arena.get(*child).style.margin.horizontal(),
            Direction::Vertical => arena.get(*child).style.margin.vertical(),
        };
        let item = child_min + margins;
        sum += item;
        widest = widest.max(item);
    }

    if own_dir != axis || (own_dir == axis && wrapping) {
        let pad = arena.get(id).style.padding;
        return widest
            + match axis {
                Direction::Horizontal => pad.horizontal(),
                Direction::Vertical => pad.vertical(),
            };
    }
    let gap_px = gap.main_px();
    let pad = arena.get(id).style.padding;
    sum + gap_px * (flow.len().saturating_sub(1)) as f32
        + match axis {
            Direction::Horizontal => pad.horizontal(),
            Direction::Vertical => pad.vertical(),
        }
}
