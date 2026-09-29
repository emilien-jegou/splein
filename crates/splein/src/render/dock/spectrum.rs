// Maps horizontal physical pixel coordinates to the continuous 6-tool color spectrum.

use tiny_skia::Color;

const STOPS: [(f32, (f32, f32, f32)); 6] = [
    (0.0,   (1.0, 0.627, 0.0)),    // Tool 0 (q): Orange #FFA000
    (56.0,  (0.341, 0.980, 0.345)), // Tool 1 (w): Green  #57FA58
    (108.0, (0.980, 1.0, 0.125)),   // Tool 2 (e): Yellow #FAFF20
    (160.0, (0.0, 0.682, 1.0)),     // Tool 3 (r): Blue   #00AEFF
    (216.0, (0.0, 0.871, 0.702)),   // Tool 4 (t): Teal   #00DEB3
    (268.0, (1.0, 0.392, 0.831)),   // Tool 5 (y): Pink   #FF64D4
];

pub fn sample_spectrum(x: f32, alpha: f32) -> Color {
    let clamped_x = x.clamp(0.0, 268.0);
    for window in STOPS.windows(2) {
        let (x1, c1) = window[0];
        let (x2, c2) = window[1];
        if clamped_x >= x1 && clamped_x <= x2 {
            let t = (clamped_x - x1) / (x2 - x1);
            let r = c1.0 + (c2.0 - c1.0) * t;
            let g = c1.1 + (c2.1 - c1.1) * t;
            let b = c1.2 + (c2.2 - c1.2) * t;
            return Color::from_rgba(r, g, b, alpha).unwrap_or(Color::WHITE);
        }
    }
    Color::WHITE
}

pub fn active_shortcut_color(idx: usize) -> Color {
    match idx {
        0 => Color::from_rgba8(0x8F, 0x59, 0x00, 255), // Dark Orange
        1 => Color::from_rgba8(0x22, 0x6F, 0x22, 255), // Paper #226F22
        2 => Color::from_rgba8(0x77, 0x75, 0x00, 255), // Dark Yellow
        3 => Color::from_rgba8(0x00, 0x4E, 0x75, 255), // Dark Blue
        4 => Color::from_rgba8(0x00, 0x63, 0x50, 255), // Dark Teal
        5 => Color::from_rgba8(0x7A, 0x20, 0x62, 255), // Dark Pink
        _ => Color::from_rgba8(0x62, 0x62, 0x62, 255),
    }
}
