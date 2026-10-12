// Single responsibility: Bottom-up intrinsic size computation for flex Group containers.

use crate::foundation::{Constraints, Direction, FlowWrap, IntrinsicSize};
use crate::layout::measure::{compute_child_desired, compute_child_desired_for_height};
use crate::layout::wrap::plan_lines;
use crate::text::TextContext;
use crate::tree::NodeId;
use crate::tree::TreeArena;

/// Computes intrinsic bounding size of a flex group from its children.
///
/// `for_height` propagates to children: height probes wrap nested text at the
/// offer, item probes keep max-content (see `TextConfig::wrap_width`).
pub fn measure_group_intrinsic(
    arena: &mut TreeArena,
    group_id: NodeId,
    constraints: Constraints,
    text_ctx: &TextContext,
    for_height: bool,
) -> IntrinsicSize {
    let node = arena.get(group_id);
    let (dir, gap, flow_wrap) = (
        node.style.layout.direction,
        node.style.layout.gap,
        node.style.layout.flow_wrap,
    );
    let children: Vec<NodeId> = arena
        .children(group_id)
        .iter()
        .copied()
        .filter(|&id| {
            !arena.get(id).style.is_absolute && arena.get(id).state.core.presence.occupies_space()
        })
        .collect();

    let pad = arena.get(group_id).style.padding;
    let constraints = Constraints {
        min_width: (constraints.min_width - pad.horizontal()).max(0.0),
        max_width: (constraints.max_width - pad.horizontal()).max(0.0),
        min_height: (constraints.min_height - pad.vertical()).max(0.0),
        max_height: (constraints.max_height - pad.vertical()).max(0.0),
    };
    if children.is_empty() {
        return IntrinsicSize {
            width: pad.horizontal(),
            height: pad.vertical(),
        };
    }

    let mut main_total = 0.0f32;
    let mut cross_max = 0.0f32;

    let ref_w = if constraints.max_width.is_finite() {
        Some(constraints.max_width)
    } else {
        None
    };
    let ref_h = if constraints.max_height.is_finite() {
        Some(constraints.max_height)
    } else {
        None
    };

    let mut mains: Vec<f32> = Vec::with_capacity(children.len());
    let mut crosses: Vec<f32> = Vec::with_capacity(children.len());
    for &child in &children {
        let child_desired = if for_height {
            compute_child_desired_for_height(arena, child, constraints, ref_w, ref_h, text_ctx)
        } else {
            compute_child_desired(arena, child, constraints, ref_w, ref_h, text_ctx)
        };
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
        mains.push(main_d);
        crosses.push(cross_d);
        main_total += main_d;
        cross_max = cross_max.max(cross_d);
    }

    let gap_main = gap.main_px();
    let gap_cross = gap.cross_px();

    // Wrapped intrinsic: bucket mains into lines against the bounded axis.
    // Width is the widest line (capped by the offer); height stacks line
    // crosses with the cross gap. Child crosses are measured at the offered
    // width rather than the final line share — an approximation shared with
    // the single-line path, which likewise measures before allocation.
    // Unbounded wrapping degrades to the single-line sum below.
    let wrapping = flow_wrap == FlowWrap::Wrap;
    if wrapping {
        let bound = match dir {
            Direction::Horizontal => constraints.max_width,
            Direction::Vertical => constraints.max_height,
        };
        if bound.is_finite() {
            let lines = plan_lines(&mains, bound, gap_main);
            let mut wide = 0.0_f32;
            let mut tall = 0.0_f32;
            for (li, &(s, e)) in lines.iter().enumerate() {
                let line_w: f32 = mains[s..e].iter().sum::<f32>()
                    + (e.saturating_sub(s + 1)) as f32 * gap_main;
                wide = wide.max(line_w);
                let line_c: f32 = crosses[s..e].iter().cloned().fold(0.0, f32::max);
                tall += line_c;
                if li + 1 < lines.len() {
                    tall += gap_cross;
                }
            }
            return match dir {
                Direction::Horizontal => IntrinsicSize {
                    width: (wide + pad.horizontal()).min(constraints.max_width),
                    height: tall + pad.vertical(),
                },
                Direction::Vertical => IntrinsicSize {
                    width: tall + pad.horizontal(),
                    height: (wide + pad.vertical()).min(constraints.max_height),
                },
            };
        }
    }

    main_total += (children.len().saturating_sub(1) as f32) * gap_main;

    match dir {
        Direction::Horizontal => IntrinsicSize {
            width: main_total + pad.horizontal(),
            height: cross_max + pad.vertical(),
        },
        Direction::Vertical => IntrinsicSize {
            width: cross_max + pad.horizontal(),
            height: main_total + pad.vertical(),
        },
    }
}
