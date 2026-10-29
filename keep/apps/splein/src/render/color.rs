// crates/splein/src/render/color.rs

use crate::domain::canvas::Rgba;
use tiny_skia::Color;

/// Maps domain RGBA to Wayland native Argb8888 (BGRA byte order on little-endian).
#[inline]
pub fn to_native_color(c: Rgba) -> Color {
    Color::from_rgba(c.b, c.g, c.r, c.a).unwrap_or(Color::BLACK)
}
