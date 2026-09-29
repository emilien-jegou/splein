// Single responsibility: Kurbo path and rounded-rectangle geometry conversions for Vello.

use crate::foundation::{Radius, ResolvedRect};
use vello::kurbo::{Rect as KurboRect, RoundedRect as KurboRoundedRect, RoundedRectRadii};

/// Converts an Aurora ResolvedRect and Radius into a Kurbo RoundedRect with W3C 4-corner clamping.
#[inline(always)]
pub fn build_kurbo_rounded_rect(rect: &ResolvedRect, radius: Radius) -> KurboRoundedRect {
    let w = rect.width.max(0.0) as f64;
    let h = rect.height.max(0.0) as f64;
    let c = radius.resolve_corners(rect.width, rect.height);
    let radii = RoundedRectRadii::new(c.top_left as f64, c.top_right as f64, c.bottom_right as f64, c.bottom_left as f64);
    let (x0, y0) = (rect.x as f64, rect.y as f64);
    KurboRoundedRect::new(x0, y0, x0 + w, y0 + h, radii)
}

/// Converts an Aurora ResolvedRect into an unrounded Kurbo Rect clamped to non-negative bounds.
#[inline(always)]
pub fn build_kurbo_rect(rect: &ResolvedRect) -> KurboRect {
    let x0 = rect.x as f64;
    let y0 = rect.y as f64;
    KurboRect::new(x0, y0, x0 + rect.width.max(0.0) as f64, y0 + rect.height.max(0.0) as f64)
}
