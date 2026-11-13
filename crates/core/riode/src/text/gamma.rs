//! Display correction for glyph coverage: scalar twin of the WGSL chain.
//!
//! Ports GPUI's grayscale contrast + gamma correction (`shaders.wgsl`,
//! itself adapted from microsoft/terminal `dwrite.hlsl`, MIT). The
//! wgpu glyph shader (`engine-wgpu/src/shaders/glyph.wgsl`) is a
//! transliteration of this module — keep the two in sync.
//!
//! Applied at ALL sizes, ALWAYS ON: contrast and gamma correction is the
//! default and only behavior (the old `RIODE_TEXT_CORRECTION=on` env gate
//! and the passthrough fallback are gone). There is deliberately no
//! font-size threshold: the chain is coverage-proportional, so dense
//! hinted stems pass through nearly unchanged while faint fringes lift,
//! without a size heuristic.

/// Gamma-1.8 correction ratios, matching GPUI's
/// `GAMMA_INCORRECT_TARGET_RATIOS[8]` pre-scaled by `NORM13`/`NORM24`.
/// (Row `[0.1469, -0.8911, 1.4644, -0.3234]/4`,
/// `NORM13 = 0x10000/255²*4`, `NORM24 = 0x100/255*4`.)
pub const GAMMA_RATIOS_1_8: [f32; 4] = [
    0.14805442,
    -0.89459451,
    1.47590801,
    -0.32466824,
];

/// Default grayscale enhanced contrast, matching GPUI's
/// `ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST` default of `1.0`.
pub const GRAYSCALE_ENHANCED_CONTRAST: f32 = 1.0;

/// Perceived brightness, REC.601 coefficients (matches GPUI).
pub fn color_brightness(color: [f32; 3]) -> f32 {
    color[0] * 0.30 + color[1] * 0.59 + color[2] * 0.11
}

/// Contrast boost for light-on-dark text (matches GPUI).
pub fn light_on_dark_contrast(enhanced_contrast: f32, color: [f32; 3]) -> f32 {
    let brightness = color_brightness(color);
    let multiplier = (4.0 * (0.75 - brightness)).clamp(0.0, 1.0);
    enhanced_contrast * multiplier
}

/// Rational contrast curve (matches GPUI).
pub fn enhance_contrast(alpha: f32, k: f32) -> f32 {
    alpha * (k + 1.0) / (alpha * k + 1.0)
}

/// Quadratic alpha correction driven by the gamma ratio row (matches GPUI).
pub fn apply_alpha_correction(a: f32, b: f32, g: [f32; 4]) -> f32 {
    let brightness_adjustment = g[0] * b + g[1];
    let correction = brightness_adjustment * a + (g[2] * b + g[3]);
    a + a * (1.0 - a) * correction
}

/// Full chain: coverage sample (0..1) + sRGB-encoded text color →
/// corrected coverage (matches GPUI's
/// `apply_contrast_and_gamma_correction`).
pub fn correct_coverage(sample: f32, ink_rgb: [f32; 3], enhanced_contrast: f32) -> f32 {
    let k = light_on_dark_contrast(enhanced_contrast, ink_rgb);
    let brightness = color_brightness(ink_rgb);
    let contrasted = enhance_contrast(sample, k);
    apply_alpha_correction(contrasted, brightness, GAMMA_RATIOS_1_8)
}

/// Per-run alpha table: coverage byte → alpha byte with run opacity
/// folded in. The dwrite contrast+gamma chain is always active (the
/// correction is the engine's default and only glyph treatment).
pub fn corrected_alpha_table(ink_rgb: [f32; 3], opacity: f32) -> [u8; 256] {
    let opacity = opacity.clamp(0.0, 1.0);
    std::array::from_fn(|i| {
        let corrected = correct_coverage(i as f32 / 255.0, ink_rgb, GRAYSCALE_ENHANCED_CONTRAST);
        (corrected * opacity * 255.0).round().clamp(0.0, 255.0) as u8
    })
}
