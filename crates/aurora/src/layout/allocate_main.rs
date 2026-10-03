// Single responsibility: Main-axis flex distribution with iterative shrink and margin preservation.

use smallvec::SmallVec;

use crate::foundation::{Distribution, Gap};

/// Main-axis allocation plan containing resolved item sizes and gap interval.
pub struct MainAllocPlan {
    pub sizes: SmallVec<[f32; 8]>,
    pub gap_px: f32,
}

/// Index-aligned per-item inputs for one main-axis distribution.
pub struct MainAxisItems<'a> {
    /// Desired margin-box size of each item, outer margins included.
    pub desired: &'a [f32],
    /// Whether each item's main-axis sizing intent is `Size::Fill`.
    pub is_fill: &'a [bool],
    /// Shrink factor of each item.
    pub shrink: &'a [f32],
    /// Outer margin sum of each item along the main axis.
    pub margins: &'a [f32],
    /// Min-content margin-box floor of each item, before clamping by its desire.
    pub min_sizes: &'a [f32],
}

/// Allocates available main-axis space among flex siblings respecting shrink and margins.
///
/// Every item also keeps its `min-*: auto` content floor, so a demanding sibling cannot crush
/// another item's box to zero and silently remove its content from the display list.
pub fn allocate_main_axis(
    available_space: f32,
    gap: Gap,
    distribution: Distribution,
    items: &MainAxisItems<'_>,
) -> MainAllocPlan {
    let count = items.desired.len();
    if count == 0 {
        return MainAllocPlan {
            sizes: SmallVec::new(),
            gap_px: 0.0,
        };
    }

    let floors = resolve_floors(items);
    let fill_count = items.is_fill.iter().filter(|&&f| f).count();
    let mut gap_px = match gap {
        Gap::Fixed(px) => px,
        Gap::Full => 0.0,
    };
    let total_desired: f32 = items.desired.iter().sum();
    let total_fixed_gaps = (count - 1) as f32 * gap_px;
    let mut plan_sizes: SmallVec<[f32; 8]> = items.desired.iter().copied().collect();

    if fill_count > 0 && available_space.is_finite() {
        let fixed_sum: f32 = items
            .desired
            .iter()
            .enumerate()
            .filter(|(i, _)| !items.is_fill[*i])
            .map(|(_, s)| *s)
            .sum();
        let remaining_for_fills = (available_space - total_fixed_gaps - fixed_sum).max(0.0);
        let per_fill = remaining_for_fills / fill_count as f32;
        for i in 0..count {
            if items.is_fill[i] {
                plan_sizes[i] = per_fill.max(floors[i]);
            }
        }
    } else if distribution == Distribution::SpaceBetween && available_space.is_finite() {
        if count > 1 {
            let remaining_space = (available_space - total_desired).max(0.0);
            gap_px = remaining_space / (count - 1) as f32;
        } else {
            gap_px = 0.0;
        }
    } else if available_space.is_finite() {
        let space_for_items = (available_space - total_fixed_gaps).max(0.0);
        if total_desired > space_for_items {
            plan_sizes = distribute_shrink(space_for_items, &plan_sizes, items.shrink, &floors);
        }
    }

    MainAllocPlan {
        sizes: plan_sizes,
        gap_px,
    }
}

/// Clamps each content floor so it can protect content without ever inflating a box past its desire.
fn resolve_floors(items: &MainAxisItems<'_>) -> SmallVec<[f32; 8]> {
    (0..items.desired.len())
        .map(|i| {
            let margin_box = items.margins[i].max(0.0);
            let content_floor = items.min_sizes[i].max(margin_box);
            // A `Fill` item has no meaningful desire of its own, so its content governs directly.
            if items.is_fill[i] {
                content_floor
            } else {
                content_floor.min(items.desired[i].max(margin_box))
            }
        })
        .collect()
}

fn distribute_shrink(
    avail: f32,
    desired: &[f32],
    shrinks: &[f32],
    floors: &[f32],
) -> SmallVec<[f32; 8]> {
    let mut sizes: SmallVec<[f32; 8]> = desired.iter().copied().collect();
    let total_des: f32 = desired.iter().sum();
    let mut overflow = (total_des - avail).max(0.0);

    for _ in 0..8 {
        if overflow <= 0.001 {
            break;
        }
        let shrink_sum: f32 = sizes
            .iter()
            .enumerate()
            .filter(|(i, &s)| s > floors[*i] && shrinks[*i] > 0.0)
            .map(|(i, _)| shrinks[i])
            .sum();
        if shrink_sum <= 0.0 {
            break;
        }

        for i in 0..sizes.len() {
            if sizes[i] > floors[i] && shrinks[i] > 0.0 {
                let share = overflow * (shrinks[i] / shrink_sum);
                let new_size = (sizes[i] - share).max(floors[i]);
                let actual_shrink = sizes[i] - new_size;
                sizes[i] = new_size;
                overflow -= actual_shrink;
            }
        }
    }
    sizes
}
