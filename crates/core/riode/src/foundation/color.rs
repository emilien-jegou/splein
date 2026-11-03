// Single responsibility: the RGBA value type every surface speaks.

use std::str::FromStr;

/// Straight (non-premultiplied) linear-ish RGBA in 0..1. Plain data, `Copy`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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

    pub const fn hex(v: u32) -> Self {
        let r = ((v >> 16) & 0xff) as f32 / 255.0;
        let g = ((v >> 8) & 0xff) as f32 / 255.0;
        let b = (v & 0xff) as f32 / 255.0;
        Self { r, g, b, a: 1.0 }
    }

    pub const fn lerp(self, other: Self, t: f32) -> Self {
        let t = if t < 0.0 { 0.0 } else if t > 1.0 { 1.0 } else { t };
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Self { r: self.r, g: self.g, b: self.b, a }
    }

    pub const fn scale(self, f: f32) -> Self {
        Self { r: self.r * f, g: self.g * f, b: self.b * f, a: self.a * f }
    }

    pub const fn premul(self) -> Self {
        Self { r: self.r * self.a, g: self.g * self.a, b: self.b * self.a, a: self.a }
    }

    pub const fn unpremul(self) -> Self {
        if self.a <= 0.0 {
            self
        } else {
            let inv = 1.0 / self.a;
            Self { r: self.r * inv, g: self.g * inv, b: self.b * inv, a: self.a }
        }
    }

    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    pub fn white_alpha(a: f32) -> Self {
        Self::rgba(1.0, 1.0, 1.0, a)
    }

    pub fn hex_alpha(v: u32, a: f32) -> Self {
        let mut c = Self::hex(v);
        c.a = a;
        c
    }
}

impl From<u32> for Color {
    #[inline(always)]
    fn from(hex: u32) -> Self {
        Self::hex(hex)
    }
}

impl From<i32> for Color {
    fn from(hex: i32) -> Self {
        Color::hex(hex as u32)
    }
}

impl FromStr for Color {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty".into());
        }
        if s.starts_with('#') {
            let hex = &s[1..];
            if hex.len() == 6 {
                if let Ok(v) = u32::from_str_radix(hex, 16) {
                    return Ok(Color::hex(v));
                }
            } else if hex.len() == 8 {
                if let Ok(v) = u32::from_str_radix(hex, 16) {
                    let r = ((v >> 24) & 0xff) as f32 / 255.0;
                    let g = ((v >> 16) & 0xff) as f32 / 255.0;
                    let b = ((v >> 8) & 0xff) as f32 / 255.0;
                    let a = (v & 0xff) as f32 / 255.0;
                    return Ok(Color { r, g, b, a });
                }
            }
            return Err(format!("bad hex {s}"));
        }
        match s.to_lowercase().as_str() {
            "transparent" => Ok(Color::TRANSPARENT),
            "black" => Ok(Color::BLACK),
            "white" => Ok(Color::WHITE),
            "red" => Ok(Color::RED),
            "green" => Ok(Color::GREEN),
            "blue" => Ok(Color::BLUE),
            _ => Err(format!("bad color {s}")),
        }
    }
}

impl From<&str> for Color {
    fn from(s: &str) -> Self {
        Self::from_str(s).unwrap_or(Color::rgb(1.0, 0.0, 1.0))
    }
}

impl From<String> for Color {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
