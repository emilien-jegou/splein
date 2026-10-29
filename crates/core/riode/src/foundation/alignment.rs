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


use std::str::FromStr;

fn parse_direction(s: &str) -> Option<Direction> {
    match s.trim().to_lowercase().as_str() {
        "horizontal" | "row" | "x" => Some(Direction::Horizontal),
        "vertical" | "column" | "col" | "y" => Some(Direction::Vertical),
        _ => None,
    }
}

fn parse_alignment(s: &str) -> Option<Alignment> {
    match s.trim().to_lowercase().as_str() {
        "start" => Some(Alignment::Start),
        "center" => Some(Alignment::Center),
        "end" => Some(Alignment::End),
        "stretch" => Some(Alignment::Stretch),
        _ => None,
    }
}

fn parse_distribution(s: &str) -> Option<Distribution> {
    match s.trim().to_lowercase().as_str() {
        "start" => Some(Distribution::Start),
        "center" => Some(Distribution::Center),
        "end" => Some(Distribution::End),
        "space-between" | "spacebetween" => Some(Distribution::SpaceBetween),
        _ => None,
    }
}

fn parse_flow_wrap(s: &str) -> Option<FlowWrap> {
    match s.trim().to_lowercase().as_str() {
        "wrap" => Some(FlowWrap::Wrap),
        "nowrap" | "no" | "none" => Some(FlowWrap::NoWrap),
        _ => None,
    }
}

impl FromStr for Direction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        parse_direction(s).ok_or_else(|| "bad direction".to_string())
    }
}
impl From<&str> for Direction {
    fn from(s: &str) -> Self {
        parse_direction(s).unwrap_or(Direction::Horizontal)
    }
}
impl From<String> for Direction {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl FromStr for Alignment {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        parse_alignment(s).ok_or_else(|| "bad alignment".to_string())
    }
}
impl From<&str> for Alignment {
    fn from(s: &str) -> Self {
        parse_alignment(s).unwrap_or(Alignment::Start)
    }
}
impl From<String> for Alignment {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl FromStr for Distribution {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        parse_distribution(s).ok_or_else(|| "bad distribution".to_string())
    }
}
impl From<&str> for Distribution {
    fn from(s: &str) -> Self {
        parse_distribution(s).unwrap_or(Distribution::Start)
    }
}
impl From<String> for Distribution {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl FromStr for FlowWrap {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        parse_flow_wrap(s).ok_or_else(|| "bad flow_wrap".to_string())
    }
}
impl From<&str> for FlowWrap {
    fn from(s: &str) -> Self {
        parse_flow_wrap(s).unwrap_or(FlowWrap::NoWrap)
    }
}
impl From<String> for FlowWrap {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
