// Construction of TinySkia shaders for solid colors and gradients.

use tiny_skia::{Color as SkiaColor, GradientStop, LinearGradient, Paint, Point as SkiaPoint, RadialGradient, SpreadMode, Transform};
use crate::foundation::{Color, Fill};

pub fn build_paint<'a>(fill: &Fill, opacity: f32) -> Option<Paint<'a>> {
    let mut paint = Paint::default();
    match fill {
        Fill::Solid(c) => {
            paint.set_color_rgba8(
                (c.r * 255.0) as u8,
                (c.g * 255.0) as u8,
                (c.b * 255.0) as u8,
                (c.a * opacity * 255.0) as u8,
            );
            Some(paint)
        }
        Fill::LinearGradient(lg) => {
            let stops: Vec<GradientStop> = lg.stops.iter().map(|s| {
                GradientStop::new(s.position, to_skia_color(s.color, opacity))
            }).collect();
            // FIX: previously used lg.end.y for both x and y — use lg.end.x for x.
            let start = SkiaPoint::from_xy(lg.start.x, lg.start.y);
            let end = SkiaPoint::from_xy(lg.end.x, lg.end.y);
            let shader = LinearGradient::new(start, end, stops, SpreadMode::Pad, Transform::identity())?;
            paint.shader = shader;
            Some(paint)
        }
        Fill::RadialGradient(rg) => {
            let stops: Vec<GradientStop> = rg.stops.iter().map(|s| {
                GradientStop::new(s.position, to_skia_color(s.color, opacity))
            }).collect();
            let center = SkiaPoint::from_xy(rg.center.x, rg.center.y);
            let shader = RadialGradient::new(center, center, rg.radius, stops, SpreadMode::Pad, Transform::identity())?;
            paint.shader = shader;
            Some(paint)
        }
    }
}

fn to_skia_color(c: Color, opacity: f32) -> SkiaColor {
    SkiaColor::from_rgba(c.r, c.g, c.b, c.a * opacity).unwrap_or(SkiaColor::BLACK)
}
