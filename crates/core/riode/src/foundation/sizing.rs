// Sizing intent specification for elements.

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Size {
    Fixed(f32),
    Fit,
    Fill,
    Percent(f32),
    /// Take whatever width the siblings decide, and fold into it.
    ///
    /// Intrinsic sizing reports min-content (longest word), so a `Fold` child
    /// never stretches a fit parent past its siblings; once the parent
    /// assigns a box, the content re-shapes (folds) at the assigned width.
    /// Against an unbounded offer it degrades to `Fit`. Opt-in only: bare
    /// nodes keep CSS `fit-content`.
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

/// Width-to-height ratio (`width / height`) for aspect-ratio sizing.
///
/// Accepts a plain ratio (`16.0 / 9.0`), `"16/9"` / `"16:9"` pairs, or a bare
/// number in markup. Non-positive or non-finite values are rejected by the
/// parser (falling back to the sentinel); the builder clamps defensively.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AspectRatio(f32);

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


use std::str::FromStr;

fn parse_aspect_ratio(s: &str) -> Option<AspectRatio> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once('/') {
        let a: f32 = a.trim().parse().ok()?;
        let b: f32 = b.trim().parse().ok()?;
        if b != 0.0 { return Some(AspectRatio::from(a / b)); }
        return None;
    }
    if let Some((a, b)) = s.split_once(':') {
        let a: f32 = a.trim().parse().ok()?;
        let b: f32 = b.trim().parse().ok()?;
        if b != 0.0 { return Some(AspectRatio::from(a / b)); }
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
        parse_aspect_ratio(s).unwrap_or(AspectRatio::from(1.0))
    }
}
impl From<String> for AspectRatio {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
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
