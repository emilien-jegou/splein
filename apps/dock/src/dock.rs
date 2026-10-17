// Single responsibility: Dock geometry + magnification law.
//
// Ports the numbers in `backp/src/index.css` (.dock-*) and the physics in
// `backp/src/components/Dock.jsx` (gaussian falloff, 0.42 growth, -12 lift).

/// Icon box, logical px (`.dock-icon`: 56x56).
pub const ICON: f32 = 56.0;
/// Narrow-viewport icon box (`@media (max-width: 1100px)`).
pub const ICON_NARROW: f32 = 46.0;
/// Bar height (`.dock-container`: 72, 62 narrow).
pub const BAR_H: f32 = 72.0;
pub const BAR_H_NARROW: f32 = 62.0;
/// Gap between items (`.dock-container` gap 6).
pub const GAP: f32 = 6.0;
/// Bar corner radius (22).
pub const RADIUS: f32 = 22.0;
/// Bar padding: 6 vertical / 8 horizontal.
pub const PAD_V: f32 = 6.0;
pub const PAD_H: f32 = 8.0;
/// Running-indicator dot: 4px, 5px below the icon.
pub const DOT: f32 = 4.0;
pub const DOT_DROP: f32 = 5.0;
/// Separator: 1px wide, 4px side margins.
pub const SEP_W: f32 = 1.0;
pub const SEP_MARGIN: f32 = 4.0;

/// Gaussian denominator from `Dock.jsx` (`exp(-d^2 / 4500)`).
pub const FALLOFF: f32 = 4500.0;
/// Peak growth (`1 + 0.42 * intensity`).
pub const GROWTH: f32 = 0.42;
/// Peak lift in px (`-12 * intensity`).
pub const LIFT: f32 = -12.0;

/// Pure magnification law: intensity in [0, 1] from cursor distance.
///
/// `centers` are the *resting* icon centers (measured before growth so the
/// animation never feeds back into itself — same rule as `Dock.jsx`).
pub fn intensity(cursor_x: f32, center_x: f32) -> f32 {
    let d = cursor_x - center_x;
    (-(d * d) / FALLOFF).exp()
}

/// (scale, lift_y) for one icon at the given cursor position.
pub fn icon_transform(cursor_x: Option<f32>, center_x: f32) -> (f32, f32) {
    match cursor_x {
        None => (1.0, 0.0),
        Some(x) => {
            let k = intensity(x, center_x);
            (1.0 + GROWTH * k, LIFT * k)
        }
    }
}
