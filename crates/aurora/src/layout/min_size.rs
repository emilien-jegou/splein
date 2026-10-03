// Single responsibility: Min-content size of a node along one axis, the CSS `min-*: auto` shrink floor.

use smallvec::SmallVec;

use crate::foundation::{Constraints, Direction, Gap, IntrinsicSize};
use crate::text::TextContext;
use crate::tree::cache::LayoutCache;
use crate::tree::{NodeId, NodeKind, TreeArena};

/// Smallest main-axis size a node can be shrunk to before its own content is crushed.
///
/// Mirrors CSS flexbox's automatic minimum size: content cannot be starved to zero by a
/// competing sibling. Callers clamp this by the item's own desired size so the guard can
/// never inflate a box past what the author asked for.
pub fn compute_min_size(
    arena: &mut TreeArena,
    id: NodeId,
    axis: Direction,
    text_ctx: &TextContext,
) -> f32 {
    if let Some(cached) = cached_min_size(&arena.get(id).state.cache, axis) {
        return cached;
    }

    let measured = measure_min_size(arena, id, axis, text_ctx).max(0.0);
    match axis {
        Direction::Horizontal => arena.get_mut(id).state.cache.min_inline = Some(measured),
        Direction::Vertical => arena.get_mut(id).state.cache.min_block = Some(measured),
    }
    measured
}

/// Reads the memoized floor for one axis.
fn cached_min_size(cache: &LayoutCache, axis: Direction) -> Option<f32> {
    match axis {
        Direction::Horizontal => cache.min_inline,
        Direction::Vertical => cache.min_block,
    }
}

/// Measures the floor: longest word for text, one line box vertically, flex rules for groups.
fn measure_min_size(
    arena: &mut TreeArena,
    id: NodeId,
    axis: Direction,
    text_ctx: &TextContext,
) -> f32 {
    // CSS exempts items that clip their overflow: clipping is precisely how an author opts out
    // of the automatic minimum, so a `clip(true)` box stays freely crushable.
    if arena.get(id).style.clip {
        return 0.0;
    }
    let text = match &arena.get(id).kind {
        NodeKind::Text(config) => Some(config.clone()),
        NodeKind::Custom(primitive) => {
            // A primitive reports its own intrinsic size; that is the smallest it can be drawn.
            let IntrinsicSize { width, height } = primitive.measure(Constraints::UNCONSTRAINED);
            return if axis == Direction::Horizontal {
                width
            } else {
                height
            };
        }
        NodeKind::Group => None,
    };

    if let Some(config) = text {
        return match axis {
            Direction::Horizontal => text_ctx.min_inline_width(&config),
            // A run always needs one line box, never zero, so it cannot be crushed out of existence.
            Direction::Vertical => config.line_height.resolve(config.size),
        };
    }

    min_group_size(arena, id, axis, text_ctx)
}

/// Flex min-content of a container: summed along its main axis, widest child across its cross axis.
fn min_group_size(
    arena: &mut TreeArena,
    id: NodeId,
    axis: Direction,
    text_ctx: &TextContext,
) -> f32 {
    let (own_dir, gap) = {
        let layout = arena.get(id).style.layout;
        (layout.direction, layout.gap)
    };

    // Out-of-flow children do not participate in intrinsic sizing, exactly as in CSS.
    let flow: SmallVec<[NodeId; 8]> = arena
        .children(id)
        .iter()
        .copied()
        .filter(|child| {
            !arena.get(*child).style.is_absolute
                && arena.get(*child).state.presence.occupies_space()
        })
        .collect();
    if flow.is_empty() {
        return 0.0;
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

    if own_dir != axis {
        return widest;
    }
    let gap_px = match gap {
        Gap::Fixed(px) if px.is_finite() => px.max(0.0),
        Gap::Fixed(_) | Gap::Full => 0.0,
    };
    sum + gap_px * (flow.len().saturating_sub(1)) as f32
}
