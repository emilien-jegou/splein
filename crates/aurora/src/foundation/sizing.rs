// Sizing intent specification for elements.

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Size {
    Fixed(f32),
    Fit,
    Fill,
    Percent(f32),
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

    pub fn is_fill(&self) -> bool {
        matches!(self, Self::Fill)
    }

    pub fn is_fit(&self) -> bool {
        matches!(self, Self::Fit)
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
