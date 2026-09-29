// Main-axis direction, distribution, and cross-axis alignment models.

use crate::foundation::spacing::Gap;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum Direction {
    #[default]
    Horizontal,
    Vertical,
}


#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}


#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum Distribution {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}


#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LayoutConfig {
    pub direction: Direction,
    pub gap: Gap,
    pub alignment: Alignment,
    pub distribution: Distribution,
}

impl LayoutConfig {
    pub const fn new(direction: Direction, gap: Gap, alignment: Alignment) -> Self {
        Self {
            direction,
            gap,
            alignment,
            distribution: Distribution::Start,
        }
    }

    pub const fn with_distribution(direction: Direction, gap: Gap, alignment: Alignment, distribution: Distribution) -> Self {
        Self {
            direction,
            gap,
            alignment,
            distribution,
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
        }
    }
}
