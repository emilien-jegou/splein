// Single responsibility: Converts USVG paint specifications and gradients into Peniko brushes.

use resvg::usvg;
use vello::kurbo::Point as KurboPoint;
use vello::peniko::{Brush, Color as VelloColor, ColorStop, ColorStops, Gradient};

/// Converts a USVG Paint into an analytical Vello GPU compute brush.
pub fn convert_usvg_paint(paint: &usvg::Paint, opacity: f32) -> Option<Brush> {
    match paint {
        usvg::Paint::Color(c) => {
            let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(Brush::Solid(VelloColor::rgba8(c.red, c.green, c.blue, a)))
        }
        usvg::Paint::LinearGradient(lg) => {
            let mut stops = ColorStops::new();
            for s in lg.stops() {
                let a = ((s.opacity().get() * opacity).clamp(0.0, 1.0) * 255.0).round() as u8;
                stops.push(ColorStop {
                    offset: s.offset().get(),
                    color: VelloColor::rgba8(s.color().red, s.color().green, s.color().blue, a),
                });
            }
            let start = KurboPoint::new(lg.x1() as f64, lg.y1() as f64);
            let end = KurboPoint::new(lg.x2() as f64, lg.y2() as f64);
            Some(Brush::Gradient(Gradient::new_linear(start, end).with_stops(stops.as_slice())))
        }
        usvg::Paint::RadialGradient(rg) => {
            let mut stops = ColorStops::new();
            for s in rg.stops() {
                let a = ((s.opacity().get() * opacity).clamp(0.0, 1.0) * 255.0).round() as u8;
                stops.push(ColorStop {
                    offset: s.offset().get(),
                    color: VelloColor::rgba8(s.color().red, s.color().green, s.color().blue, a),
                });
            }
            let center = KurboPoint::new(rg.cx() as f64, rg.cy() as f64);
            let focal = KurboPoint::new(rg.fx() as f64, rg.fy() as f64);
            Some(Brush::Gradient(
                Gradient::new_two_point_radial(focal, 0.0f32, center, rg.r().get() as f32).with_stops(stops.as_slice()),
            ))
        }
        usvg::Paint::Pattern(_) => None,
    }
}
