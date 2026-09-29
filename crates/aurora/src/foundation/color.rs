// Single responsibility: Normalized RGBA color representation and construction utilities.

/// Normalized RGBA color structure with 32-bit floating point channels.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const RED: Self = Self { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const GREEN: Self = Self { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
    pub const BLUE: Self = Self { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Constructs color from 24-bit RGB hexadecimal integer (e.g. 0xFF00AA).
    pub fn hex(hex: u32) -> Self {
        let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let b = (hex & 0xFF) as f32 / 255.0;
        Self { r, g, b, a: 1.0 }
    }

    /// Constructs color from 24-bit hex integer with normalized alpha float.
    pub fn hex_alpha(hex: u32, a: f32) -> Self {
        let mut c = Self::hex(hex);
        c.a = a.clamp(0.0, 1.0);
        c
    }

    /// Constructs white with the specified opacity alpha.
    pub fn white_alpha(a: f32) -> Self {
        Self::rgba(1.0, 1.0, 1.0, a)
    }

    pub fn with_alpha(mut self, a: f32) -> Self {
        self.a = a;
        self
    }
}

impl From<u32> for Color {
    #[inline(always)]
    fn from(hex: u32) -> Self {
        Self::hex(hex)
    }
}
