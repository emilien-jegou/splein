// Single responsibility: Abstract blend modes and CSS compositing keywords.

/// Blend modes for shader-level compositing across surfaces, effects, and leaf elements.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    ColorDodge = 6,
    ColorBurn = 7,
    HardLight = 8,
    SoftLight = 9,
    Difference = 10,
    Exclusion = 11,
    Hue = 12,
    Saturation = 13,
    Color = 14,
    Luminosity = 15,
    Additive = 16,
}

impl BlendMode {
    pub const fn is_normal(self) -> bool {
        matches!(self, Self::Normal)
    }

    pub const fn as_uniform_id(self) -> f32 {
        self as u8 as f32
    }

    pub const fn from_uniform_id(id: f32) -> Self {
        match id as i32 {
            1 => Self::Multiply,
            2 => Self::Screen,
            3 => Self::Overlay,
            4 => Self::Darken,
            5 => Self::Lighten,
            6 => Self::ColorDodge,
            7 => Self::ColorBurn,
            8 => Self::HardLight,
            9 => Self::SoftLight,
            10 => Self::Difference,
            11 => Self::Exclusion,
            12 => Self::Hue,
            13 => Self::Saturation,
            14 => Self::Color,
            15 => Self::Luminosity,
            16 => Self::Additive,
            _ => Self::Normal,
        }
    }
}

pub fn parse_blend(s: &str) -> Option<BlendMode> {
    match s.trim().to_lowercase().replace(['-', '_'], "").as_str() {
        "normal" => Some(BlendMode::Normal),
        "multiply" => Some(BlendMode::Multiply),
        "screen" => Some(BlendMode::Screen),
        "overlay" => Some(BlendMode::Overlay),
        "darken" => Some(BlendMode::Darken),
        "lighten" => Some(BlendMode::Lighten),
        "colordodge" => Some(BlendMode::ColorDodge),
        "colorburn" => Some(BlendMode::ColorBurn),
        "hardlight" => Some(BlendMode::HardLight),
        "softlight" => Some(BlendMode::SoftLight),
        "difference" => Some(BlendMode::Difference),
        "exclusion" => Some(BlendMode::Exclusion),
        "hue" => Some(BlendMode::Hue),
        "saturation" => Some(BlendMode::Saturation),
        "color" => Some(BlendMode::Color),
        "luminosity" => Some(BlendMode::Luminosity),
        "additive" | "add" => Some(BlendMode::Additive),
        _ => None,
    }
}

impl std::str::FromStr for BlendMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        parse_blend(s).ok_or_else(|| format!("invalid blend {s}"))
    }
}

impl From<&str> for BlendMode {
    fn from(s: &str) -> Self {
        parse_blend(s).unwrap_or(BlendMode::Normal)
    }
}
