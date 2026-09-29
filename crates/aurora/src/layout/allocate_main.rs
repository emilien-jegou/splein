// Single responsibility: Main-axis flex distribution with iterative shrink and margin preservation.

use crate::foundation::{Distribution, Gap};
use smallvec::SmallVec;

/// Main-axis allocation plan containing resolved item sizes and gap interval.
pub struct MainAllocPlan {
    pub sizes: SmallVec<[f32; 8]>,
    pub gap_px: f32,
}

/// Allocates available main-axis space among flex siblings respecting shrink and margins.
pub fn allocate_main_axis(
    available_space: f32,
    desired_sizes: &[f32],
    is_fills: &[bool],
    shrinks: &[f32],
    gap: Gap,
    child_margins: &[f32],
    distribution: Distribution,
) -> MainAllocPlan {
    let count = desired_sizes.len();
    if count == 0 {
        return MainAllocPlan {
            sizes: SmallVec::new(),
            gap_px: 0.0,
        };
    }

    let fill_count = is_fills.iter().filter(|&&f| f).count();
    let mut gap_px = match gap {
        Gap::Fixed(px) => px,
        Gap::Full => 0.0,
    };
    let total_desired: f32 = desired_sizes.iter().sum();
    let total_fixed_gaps = (count - 1) as f32 * gap_px;
    let mut plan_sizes: SmallVec<[f32; 8]> = desired_sizes.iter().copied().collect();

    if fill_count > 0 && available_space.is_finite() {
        let fixed_sum: f32 = desired_sizes
            .iter()
            .enumerate()
            .filter(|(i, _)| !is_fills[*i])
            .map(|(_, s)| *s)
            .sum();
        let remaining_for_fills = (available_space - total_fixed_gaps - fixed_sum).max(0.0);
        let per_fill = remaining_for_fills / fill_count as f32;
        for i in 0..count {
            if is_fills[i] {
                plan_sizes[i] = per_fill.max(child_margins[i]);
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
            plan_sizes = distribute_shrink(space_for_items, desired_sizes, shrinks, child_margins);
        }
    }

    MainAllocPlan {
        sizes: plan_sizes,
        gap_px,
    }
}

fn distribute_shrink(
    avail: f32,
    desired: &[f32],
    shrinks: &[f32],
    margins: &[f32],
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
            .filter(|(i, &s)| s > margins[*i] && shrinks[*i] > 0.0)
            .map(|(i, _)| shrinks[i])
            .sum();
        if shrink_sum <= 0.0 {
            break;
        }

        for i in 0..sizes.len() {
            if sizes[i] > margins[i] && shrinks[i] > 0.0 {
                let share = overflow * (shrinks[i] / shrink_sum);
                let new_size = (sizes[i] - share).max(margins[i]);
                let actual_shrink = sizes[i] - new_size;
                sizes[i] = new_size;
                overflow -= actual_shrink;
            }
        }
    }
    sizes
}
