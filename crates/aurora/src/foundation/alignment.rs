// Main-axis direction, distribution, and cross-axis alignment models.

use crate::foundation::spacing::Gap;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Distribution {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Main-axis wrapping policy for flex containers.
///
/// `NoWrap` (default) preserves the historical single-line behavior: overflow
/// shrinks (down to min-content floors) or overflows. `Wrap` buckets children
/// into lines against the available main-axis space; lines stack along the
/// cross axis separated by the cross gap, and cross-axis `Alignment` applies
/// *within* each line band (whose height is the tallest child in that line).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum FlowWrap {
    #[default]
    NoWrap,
    Wrap,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LayoutConfig {
    pub direction: Direction,
    pub gap: Gap,
    pub alignment: Alignment,
    pub distribution: Distribution,
    pub flow_wrap: FlowWrap,
}

impl LayoutConfig {
    pub const fn new(direction: Direction, gap: Gap, alignment: Alignment) -> Self {
        Self {
            direction,
            gap,
            alignment,
            distribution: Distribution::Start,
            flow_wrap: FlowWrap::NoWrap,
        }
    }

    pub const fn with_distribution(
        direction: Direction,
        gap: Gap,
        alignment: Alignment,
        distribution: Distribution,
    ) -> Self {
        Self {
            direction,
            gap,
            alignment,
            distribution,
            flow_wrap: FlowWrap::NoWrap,
        }
    }

    pub const fn with_wrap(
        direction: Direction,
        gap: Gap,
        alignment: Alignment,
        distribution: Distribution,
        flow_wrap: FlowWrap,
    ) -> Self {
        Self {
            direction,
            gap,
            alignment,
            distribution,
            flow_wrap,
        }
    }
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            direction: Direction::Horizontal,
            gap: Gap::Fixed(0.0),
            alignment: Alignment::Start,
            distribution: Distribution::Start,
            flow_wrap: FlowWrap::NoWrap,
        }
    }
}
