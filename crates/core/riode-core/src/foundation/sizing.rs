// Responsibility: Sizing intent specification, aspect ratio representation, and parsing for elements.

use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Size {
    Fixed(f32),
    Fit,
    Fill,
    Percent(f32),
    Fold,
}

impl Size {
    pub const fn fixed(value: f32) -> Self {
        Self::Fixed(value)
    }

    pub const fn fit() -> Self {
        Self::Fit
    }

    pub const fn fill() -> Self {
        Self::Fill
    }

    pub const fn percent(ratio: f32) -> Self {
        Self::Percent(ratio)
    }

    pub const fn fold() -> Self {
        Self::Fold
    }

    pub fn is_fill(&self) -> bool {
        matches!(self, Self::Fill)
    }

    pub fn is_fit(&self) -> bool {
        matches!(self, Self::Fit)
    }

    pub fn is_fold(&self) -> bool {
        matches!(self, Self::Fold)
    }
}

impl From<f32> for Size {
    fn from(value: f32) -> Self {
        Self::Fixed(value)
    }
}

impl From<u32> for Size {
    fn from(v: u32) -> Self {
        Size::Fixed(v as f32)
    }
}

impl From<i32> for Size {
    fn from(v: i32) -> Self {
        Size::Fixed(v as f32)
    }
}

/// Width-to-height ratio (`width / height`) for aspect-ratio sizing.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AspectRatio(pub f32);

impl AspectRatio {
    pub fn new(ratio: f32) -> Option<Self> {
        if ratio.is_finite() && ratio > 0.0 {
            Some(Self(ratio))
        } else {
            None
        }
    }

    pub fn ratio(self) -> f32 {
        self.0
    }
}

impl From<f32> for AspectRatio {
    fn from(value: f32) -> Self {
        Self(if value.is_finite() && value > 0.0 {
            value
        } else {
            1.0
        })
    }
}

fn parse_aspect_ratio(s: &str) -> Option<AspectRatio> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once('/') {
        let a: f32 = a.trim().parse().ok()?;
        let b: f32 = b.trim().parse().ok()?;
        if b != 0.0 {
            return Some(AspectRatio::from(a / b));
        }
        return None;
    }
    if let Some((a, b)) = s.split_once(':') {
        let a: f32 = a.trim().parse().ok()?;
        let b: f32 = b.trim().parse().ok()?;
        if b != 0.0 {
            return Some(AspectRatio::from(a / b));
        }
        return None;
    }
    if let Ok(v) = s.parse::<f32>() {
        return Some(AspectRatio::from(v));
    }
    None
}

impl FromStr for AspectRatio {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        parse_aspect_ratio(s).ok_or_else(|| "bad aspect_ratio".to_string())
    }
}

impl From<&str> for AspectRatio {
    fn from(s: &str) -> Self {
        parse_aspect_ratio(s).unwrap_or(AspectRatio(1.0))
    }
}

impl From<String> for AspectRatio {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
