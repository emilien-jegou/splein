// Single responsibility: Bottom-up intrinsic size computation for flex Group containers.

use crate::foundation::{Constraints, Direction, Gap, IntrinsicSize};
use crate::layout::measure::compute_child_desired;
use crate::text::TextContext;
use crate::tree::{NodeId, TreeArena};

/// Computes intrinsic bounding size of a flex group from its children.
pub fn measure_group_intrinsic(
    arena: &mut TreeArena,
    group_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
) -> IntrinsicSize {
    let node = arena.get(group_id);
    let (dir, gap) = (node.style.layout.direction, node.style.layout.gap);
    let children: Vec<NodeId> = arena
        .children(group_id)
        .iter()
        .copied()
        .filter(|&id| {
            !arena.get(id).style.is_absolute && arena.get(id).state.presence.occupies_space()
        })
        .collect();

    if children.is_empty() { return IntrinsicSize { width: 0.0, height: 0.0 }; }

    let mut main_total = 0.0f32;
    let mut cross_max = 0.0f32;

    let ref_w = if constraints.max_width.is_finite() { Some(constraints.max_width) } else { None };
    let ref_h = if constraints.max_height.is_finite() { Some(constraints.max_height) } else { None };

    for &child in &children {
        let child_desired = compute_child_desired(arena, child, constraints, ref_w, ref_h, text_ctx);
        let child_node = arena.get(child);
        let (main_d, cross_d) = match dir {
            Direction::Horizontal => (
                child_desired.width + child_node.style.margin.horizontal(),
                child_desired.height + child_node.style.margin.vertical(),
            ),
            Direction::Vertical => (
                child_desired.height + child_node.style.margin.vertical(),
                child_desired.width + child_node.style.margin.horizontal(),
            ),
        };
        main_total += main_d;
        cross_max = cross_max.max(cross_d);
    }

    let gap_px = match gap { Gap::Fixed(px) => px, Gap::Full => 0.0 };
    main_total += (children.len().saturating_sub(1) as f32) * gap_px;

    match dir {
        Direction::Horizontal => IntrinsicSize { width: main_total, height: cross_max },
        Direction::Vertical => IntrinsicSize { width: cross_max, height: main_total },
    }
}
