// Single responsibility: Peniko Brush and Gradient conversions from Aurora Fills.

use crate::foundation::{Color, Fill};
use vello::kurbo::Point as KurboPoint;
use vello::peniko::{Brush, Color as VelloColor, ColorStop, ColorStops, Gradient};

pub fn build_vello_brush(fill: &Fill, opacity: f32) -> Option<Brush> {
    match fill {
        Fill::Solid(c) => Some(Brush::Solid(to_vello_color(*c, opacity))),
        Fill::LinearGradient(lg) => {
            let mut stops = ColorStops::new();
            for s in &lg.stops {
                stops.push(ColorStop {
                    offset: s.position,
                    color: to_vello_color(s.color, opacity),
                });
            }
            let start = KurboPoint::new(lg.start.x as f64, lg.start.y as f64);
            let end = KurboPoint::new(lg.end.x as f64, lg.end.y as f64);
            Some(Brush::Gradient(
                Gradient::new_linear(start, end).with_stops(stops.as_slice()),
            ))
        }
        Fill::RadialGradient(rg) => {
            let mut stops = ColorStops::new();
            for s in &rg.stops {
                stops.push(ColorStop {
                    offset: s.position,
                    color: to_vello_color(s.color, opacity),
                });
            }
            let center = KurboPoint::new(rg.center.x as f64, rg.center.y as f64);
            Some(Brush::Gradient(
                Gradient::new_two_point_radial(center, 0.0f32, center, rg.radius as f32)
                    .with_stops(stops.as_slice()),
            ))
        }
    }
}

#[inline(always)]
pub fn to_vello_color(c: Color, opacity: f32) -> VelloColor {
    VelloColor::rgba(
        c.r as f64,
        c.g as f64,
        c.b as f64,
        (c.a * opacity.clamp(0.0, 1.0)) as f64,
    )
}
