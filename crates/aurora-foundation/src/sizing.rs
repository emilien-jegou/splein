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
