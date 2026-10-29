// Single responsibility: Anode liquid-glass looks — complete explicit
// bundles of engine-atomic values.
//
// Fitted reference data below (every literal): transcribed verbatim from
// the engine's former per-variant constant rows. Do not hand-tune; tune
// the atomic props at the call site instead, and only promote a value
// here once eyes approve it on the demos.

use riode::foundation::glass::{GlassStyle, ToneCurve};
use riode_shader::shadow::Shadow;
use riode::foundation::{Backdrop, Color, Glass, Glow, Refraction, Spectral};

/// Clear tone curve: the fitted shadow crush
/// (`0.125 → 0.0289`, …). Brand-free name; Apple-matched numbers.
/// (Uses the engine's `ToneCurve` type — one definition.)
pub const CLEAR_TONE: ToneCurve = ToneCurve {
    knots: [0.0, 0.0289, 0.1865, 0.3476, 0.5063, 0.6651, 0.8265, 0.9772, 1.0],
    // Unset in the fitted row (= identity, and gated off by lo/hi = 0).
    small_knots: [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0],
    small_lo: 0.0,
    small_hi: 0.0,
    luma_mix: 0.0,
    lift: 0.0,
    lift_knee: 0.5,
    lift_size_ref: 0.0,
};

/// Dark tone curve: brighter mids, lifted shadow end for small shapes.
pub const DARK_TONE: ToneCurve = ToneCurve {
    knots: [0.0, 0.1917, 0.2754, 0.3813, 0.4936, 0.6408, 0.7513, 0.8537, 0.9861],
    small_knots: [0.0, 0.0675, 0.3273, 0.4709, 0.8295, 0.92, 0.96, 0.985, 1.0],
    small_lo: 32.0,
    small_hi: 34.0,
    luma_mix: 0.0,
    lift: 0.7708,
    lift_knee: 0.9995,
    lift_size_ref: 48.0387,
};

/// Clear-dark tone curve.
pub const CLEAR_DARK_TONE: ToneCurve = ToneCurve {
    knots: [
        0.0,
        0.051_909_158_652_979_7,
        0.213_090_511_337_508,
        0.363_096_272_728_077_9,
        0.519_814_575_290_755_1,
        0.679_180_921_179_765,
        0.834_769_696_623_644_4,
        0.991_006_535_026_090_4,
        1.0,
    ],
    small_knots: [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0],
    small_lo: 0.0,
    small_hi: 0.0,
    luma_mix: 0.0,
    lift: 0.0,
    lift_knee: 0.5,
    lift_size_ref: 0.0,
};

/// A complete liquid-glass look: the lens geometry plus the backdrop
/// stages it samples, all explicit. No table, no variant flag — what
/// you see is this struct. Expand onto a group with the engine's
/// `preset={...}` (or read the fields and set the atomic props by hand).
#[derive(Clone, Debug, PartialEq)]
pub struct GlassPreset {
    /// Lens + rim + frost-model geometry (stage fields ride on the
    /// backdrop below).
    pub glass: Glass,
    /// The backdrop stages the lens samples: core blur, wash,
    /// saturation, tone, ambient pickup, fill and tint.
    pub backdrop: Backdrop,
    /// The spectral (rainbow) response layered over the lens refraction.
    /// `Spectral::NONE` keeps every preset exactly monochromatic.
    pub spectral: Spectral,
    /// The emissive bottom glow (the voice-glow wash). `Glow::NONE`
    /// keeps every preset exactly as before (off).
    pub glow: Glow,
    /// The outer refraction halo (theatre-mode blur of the card).
    /// `Refraction::NONE` keeps every preset exactly as before (off).
    pub refraction: Refraction,
    /// The cast shadow for this look (v2 Phase 1: a plain `Shadow` owned
    /// by the general API, painted behind the glass — no longer a lens
    /// knob). Applied via `preset={...}` unless an explicit `shadow=`
    /// prop wins.
    pub shadow: Shadow,
}

impl GlassPreset {
    /// Fully knobbed lens: every value absolute except `rim_scale`
    /// (overall edge-light multiplier, 1.0 = as given). Returns the
    /// lens, the backdrop stages that were lens-coupled (tint strength,
    /// blur/fill size gating, dim, ambient reach), and the cast shadow
    /// (v2 Phase 1: a plain `Shadow`, no longer a lens knob).
    #[allow(clippy::too_many_arguments)]
    fn lens(
        strength: f32,
        lens_strength: f32,
        band: f32,
        decay: f32,
        size_ref: f32,
        edge: f32,
        edge_decay: f32,
        normal_radius: f32,
        aspect_power: f32,
        rim_width: f32,
        rim_intensity: f32,
        rim_mix: f32,
        rim_mix_width: f32,
        rim_mix_cut: f32,
        rim_mix_floor: f32,
        shadow_radius: f32,
        shadow_opacity: f32,
        tint_strength: f32,
        post_share: f32,
        wide_sigma: f32,
        wide_edge: f32,
        wide_centre: f32,
        wide_size_ref: f32,
        wide_size_drop: f32,
        frost_size_ref: f32,
        fill_size_ref: f32,
        fill_size_drop: f32,
        glow: f32,
        press: f32,
        dim: f32,
        jacobian: f32,
        ambient_reach: f32,
    ) -> (Glass, Backdrop, Shadow) {
        let glass = Glass::lens(strength)
            .lens_strength(lens_strength)
            .band(band)
            .lens_decay(decay)
            .lens_size_ref(size_ref)
            .lens_edge(edge)
            .lens_edge_decay(edge_decay)
            .normal_radius(normal_radius)
            .blur_aspect_power(aspect_power)
            .rim_width(rim_width)
            .rim_intensity(rim_intensity)
            .rim_mix(rim_mix)
            .rim_mix_width(rim_mix_width)
            .rim_mix_cut(rim_mix_cut)
            .rim_mix_floor(rim_mix_floor)
            .post_share(post_share)
            .frost_wide(wide_sigma)
            .frost_wide_mix(wide_edge, wide_centre)
            .frost_wide_size(wide_size_ref, wide_size_drop)
            .glow(glow)
            .press_lift(press)
            .post_jacobian(jacobian);
        let backdrop = Backdrop::NONE
            .tint_strength(tint_strength)
            .blur_size_ref(frost_size_ref)
            .fill_size(fill_size_ref, fill_size_drop)
            .dim(dim)
            .ambient_reach(ambient_reach);
        // The cast shadow (v2 Phase 1): black, softness = half the old
        // in-shader sigma (≥1.0 px), extent = radius, opacity = edge peak.
        // A plain Box drop shadow — but note it must be painted to the
        // *parent* target behind the glass layer (see `enter_node_frame`):
        // inside the glass offscreen it would be masked by the lens.
        let shadow = Shadow::layered(Color::BLACK)
            .ease_out((shadow_radius * 0.5).max(1.0), shadow_radius, shadow_opacity);
        (glass, backdrop, shadow)
    }

    /// Standard frosted glass: adapts to the content behind it.
    /// (Pre-deletion: selects the fitted regular bank for the tone
    /// knots and size heuristics; every other number below wins.)
    pub fn regular() -> Self {
        let (glass, stages, shadow) = Self::lens(
            1.0, -2.6351, 18.2615, 6.1712, 38.2208, 0.0, 0.6, 1.0, 0.0, 1.2454,
            0.2315, 0.0, 1.5, 1.0, 1.0, 20.8906, 0.0494, 1.024, 0.0, 7.0367, 0.4052,
            0.9298, 75.4199, 1.1423, 59.9762, 43.9952, 0.0, 0.016, 0.047, 0.0046, 4.0,
            60.0,
        );
        Self {
            glass,
            backdrop: stages
                .blur(5.9236)
                .wash(Color::hex_alpha(0xFEFDFD, 0.6839))
                .saturation(1.4901)
                .tone(1.0)
                .ambient(0.18),
            spectral: Spectral::NONE,
            glow: Glow::NONE,
            refraction: Refraction::NONE,
            shadow,
        }
    }

    /// Sharp glass for rich media backgrounds: minimal frost and veil,
    /// strong edge capture. (Pre-deletion: selects the clear bank.)
    pub fn clear() -> Self {
        let (glass, stages, shadow) = Self::lens(
            1.0, -2.49, 18.442, 6.8073, 28.8, 10.0, 0.6, 1.55, 0.0, 1.2475, 0.15,
            0.48, 1.5, 1.0, 1.0, 29.31, 0.0015, 0.35, 0.31, 1.7973, -0.0117, 0.6789,
            85.2463, 0.0002, 0.0, 0.0, 0.0, 0.016, 0.047, 0.0013, 1.15, 0.0,
        );
        Self {
            glass,
            backdrop: stages
                .blur(1.28)
                .wash(Color::hex_alpha(0xFEFCFC, 0.1705))
                .saturation(0.9726)
                .tone(1.0)
                .tone_curve(CLEAR_TONE)
                .ambient(0.0),
            spectral: Spectral::NONE,
            glow: Glow::NONE,
            refraction: Refraction::NONE,
            shadow,
        }
    }

    /// Standard glass against a dark appearance.
    /// (Pre-deletion: selects the regular-dark bank.)
    pub fn regular_dark() -> Self {
        let (glass, stages, shadow) = Self::lens(
            1.0, -2.6267, 18.8261, 5.9001, 38.791, 0.0, 0.6, 1.0, 0.28, 1.3623,
            0.3293, 0.0, 1.5, 1.0, 1.0, 20.3036, 0.0264, 1.0054, 0.0, 7.556, 1.0836,
            0.908, 79.2109, 2.7378, 78.0334, 36.0728, 0.0, 0.073, 0.056, 0.001, 4.0, 0.0,
        );
        Self {
            glass,
            backdrop: stages
                .blur(8.2468)
                .wash(Color::hex_alpha(0x191818, 0.6672))
                .saturation(1.967)
                .tone(1.0)
                .tone_curve(DARK_TONE)
                .ambient(0.0),
            spectral: Spectral::NONE,
            glow: Glow::NONE,
            refraction: Refraction::NONE,
            shadow,
        }
    }

    /// The preset as one engine value: expand onto a group with
    /// `preset={...}` (static or computed). Explicit props win
    /// field-by-field.
    pub fn style(&self) -> GlassStyle {
        GlassStyle {
            glass: self.glass,
            backdrop: self.backdrop,
            spectral: self.spectral,
            glow: self.glow,
            refraction: self.refraction,
            shadow: Some(self.shadow.clone()),
        }
    }

    /// The preset tinted: a backdrop colour blended into the glass body
    /// (alpha is the tint strength).
    pub fn style_tinted(&self, tint: Color) -> GlassStyle {
        GlassStyle {
            glass: self.glass,
            backdrop: self.backdrop.tint(tint),
            spectral: self.spectral,
            glow: self.glow,
            refraction: self.refraction,
            shadow: Some(self.shadow.clone()),
        }
    }

    /// The preset as a Phase-5 union member (`union_id`): members
    /// sharing one merge into their bounding rect before the pack.
    pub fn style_union(&self, id: u64) -> GlassStyle {
        GlassStyle {
            glass: self.glass.union_id(id),
            backdrop: self.backdrop,
            spectral: self.spectral,
            glow: self.glow,
            refraction: self.refraction,
            shadow: Some(self.shadow.clone()),
        }
    }

    /// The preset as a touch-reactive morph member (`glass_id`): members
    /// sharing one animate rect/radius between targets on the morph
    /// spring instead of snapping.
    pub fn style_morph(&self, id: u64) -> GlassStyle {
        GlassStyle {
            glass: self.glass.interactive(true).glass_id(id),
            backdrop: self.backdrop,
            spectral: self.spectral,
            glow: self.glow,
            refraction: self.refraction,
            shadow: Some(self.shadow.clone()),
        }
    }

    /// Isolation lens: clear geometry, neutral stages, linear tone.
    /// The debugging primitive — what remains when every look is
    /// switched off. Any visible ring/crush with this preset names a
    /// stage that still hides numbers.
    pub fn clear_bare() -> Self {
        let (glass, stages, shadow) = Self::lens(
            1.0, -2.49, 18.442, 6.8073, 28.8, 10.0, 0.6, 1.55, 0.0, 1.2475, 0.0,
            0.0, 1.5, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        );
        Self {
            glass,
            backdrop: stages
                .blur(0.0)
                .wash(Color::rgba(0.0, 0.0, 0.0, 0.0))
                .saturation(1.0)
                .tone(0.0)
                .ambient(0.0),
            spectral: Spectral::NONE,
            glow: Glow::NONE,
            refraction: Refraction::NONE,
            shadow,
        }
    }

    /// Sharp glass against a dark appearance.
    /// (Pre-deletion: selects the clear-dark bank.)
    pub fn clear_dark() -> Self {
        let (glass, stages, shadow) = Self::lens(
            1.0, -2.49, 18.3411, 6.8762, 28.7, 11.5854, 0.4954, 1.5358, 0.0, 1.2475,
            0.15, 0.3747, 1.1782, 3.9722, 0.05, 13.0732, 0.0017, 0.35, 0.1482, 1.9152,
            0.0296, 0.7009, 84.2326, 0.0038, 0.0, 0.0, 0.0, 0.073, 0.056, 0.0006, 1.15,
            0.0,
        );
        Self {
            glass,
            backdrop: stages
                .blur(1.2334)
                .wash(Color::hex_alpha(0xFDFAFB, 0.1581))
                .saturation(0.9633)
                .tone(1.0)
                .tone_curve(CLEAR_DARK_TONE)
                .ambient(0.0),
            spectral: Spectral::NONE,
            glow: Glow::NONE,
            refraction: Refraction::NONE,
            shadow,
        }
    }
}

#[cfg(test)]
mod transcription_tests {
    use super::*;

    fn check(preset: GlassPreset, want: riode::foundation::glass::GlassBank) {
        let v = preset.glass.effective(&preset.backdrop);
        assert_eq!(v, want, "preset must expand to exactly its fitted row");
    }

    #[test]
    fn regular_matches_row() {
        check(GlassPreset::regular(), fitted_rows::regular());
    }

    #[test]
    fn clear_matches_row() {
        check(GlassPreset::clear(), fitted_rows::clear());
    }

    #[test]
    fn regular_dark_matches_row() {
        check(GlassPreset::regular_dark(), fitted_rows::regular_dark());
    }

    #[test]
    fn clear_dark_matches_row() {
        check(GlassPreset::clear_dark(), fitted_rows::clear_dark());
    }
}

// Frozen fitted rows (deleted engine table, verbatim + `..ZERO` spread).
// If a preset intentionally changes look, update the matching row here.
#[cfg(test)]
mod fitted_rows {
    use riode::foundation::glass::{GlassBank, IDENTITY_TONE_KNOTS};
    use riode::foundation::Color;

    #[allow(dead_code)]
    pub fn regular() -> GlassBank {
        GlassBank {
                // Task g13: fitted tone LUT plus a small-shape tone curve;
                // item 8 rounds 2-3: ambient backdrop chroma and a
                // luma-keyed tone LUT.
                blur_sigma: 5.9236,
                blur_size_ref: 59.9762,
                frost_wide_sigma: 7.0367,
                frost_wide_mix_edge: 0.4052,
                frost_wide_mix_centre: 0.9298,
                frost_wide_size_ref: 75.4199,
                frost_wide_size_drop: 1.1423,
                tone_knots: [
                    0.0, 0.113, 0.25, 0.363, 0.4973, 0.6263, 0.7534, 0.8839, 1.0,
                ],
                tone_luma_mix: 0.6,
                ambient_mix: 0.18,
                ambient_reach: 60.0,
                small_tone_knots: [
                    0.0, 0.1, 0.2, 0.3, 0.5, 0.611, 0.7142, 0.8538, 0.974,
                ],
                small_size_lo: 32.0,
                small_size_hi: 34.0,
                lens_band: 18.2615,
                lens_strength: -2.6351,
                lens_decay: 6.1712,
                lens_size_ref: 38.2208,
                rim_width: 1.2454,
                rim_intensity: 0.2315,
                fill_color: Color::hex(0xFEFDFD),
                fill_opacity: 0.6839,
                fill_size_ref: 43.9952,
                fill_size_drop: 0.0,
                saturation: 1.4901,
                dim: 0.0046,
                tint_strength: 1.024,
                // Task 17d: pressed glass light + Gaussian under the finger.
                glow_strength: 0.016,
                press_lift: 0.047,
            ..GlassBank::ZERO
        }
    }

    #[allow(dead_code)]
    pub fn clear() -> GlassBank {
        GlassBank {
                post_jacobian_max: 1.15,
                blur_sigma: 1.28,
                blur_size_ref: 0.0,
                frost_wide_sigma: 1.7973,
                frost_wide_mix_edge: -0.0117,
                frost_wide_mix_centre: 0.6789,
                frost_wide_size_ref: 85.2463,
                frost_wide_size_drop: 0.0002,
                tone_knots: [
                    0.0, 0.0289, 0.1865, 0.3476, 0.5063, 0.6651, 0.8265, 0.9772, 1.0,
                ],
                post_blur_share: 0.31,
                normal_radius_scale: 1.55,
                lens_edge: 10.0,
                rim_mix: 0.48,
                lens_band: 18.442,
                lens_strength: -2.49,
                lens_decay: 6.8073,
                lens_size_ref: 28.8,
                rim_width: 1.2475,
                rim_intensity: 0.15,
                fill_color: Color::hex(0xFEFCFC),
                fill_opacity: 0.1705,
                fill_size_ref: 0.0,
                fill_size_drop: 0.0,
                saturation: 0.9726,
                dim: 0.0013,
                tint_strength: 0.35,
                glow_strength: 0.016,
                press_lift: 0.047,
            ..GlassBank::ZERO
        }
    }

    #[allow(dead_code)]
    pub fn regular_dark() -> GlassBank {
        GlassBank {
                blur_sigma: 8.2468,
                blur_size_ref: 78.0334,
                frost_wide_sigma: 7.556,
                frost_wide_mix_edge: 1.0836,
                frost_wide_mix_centre: 0.908,
                frost_wide_size_ref: 79.2109,
                frost_wide_size_drop: 2.7378,
                tone_knots: [
                    0.0, 0.1917, 0.2754, 0.3813, 0.4936, 0.6408, 0.7513, 0.8537, 0.9861,
                ],
                small_tone_knots: [
                    0.0, 0.0675, 0.3273, 0.4709, 0.8295, 0.92, 0.96, 0.985, 1.0,
                ],
                small_size_lo: 32.0,
                small_size_hi: 34.0,
                tone_lift: 0.7708,
                tone_lift_knee: 0.9995,
                tone_lift_size_ref: 48.0387,
                blur_aspect_power: 0.28,
                lens_band: 18.8261,
                lens_strength: -2.6267,
                lens_decay: 5.9001,
                lens_size_ref: 38.791,
                rim_width: 1.3623,
                rim_intensity: 0.3293,
                fill_color: Color::hex(0x191818),
                fill_opacity: 0.6672,
                fill_size_ref: 36.0728,
                fill_size_drop: 0.0,
                saturation: 1.967,
                dim: 0.001,
                tint_strength: 1.0054,
                glow_strength: 0.073,
                press_lift: 0.056,
            ..GlassBank::ZERO
        }
    }

    #[allow(dead_code)]
    pub fn clear_dark() -> GlassBank {
        GlassBank {
                post_jacobian_max: 1.15,
                blur_sigma: 1.2334,
                blur_size_ref: 0.0,
                frost_wide_sigma: 1.9152,
                frost_wide_mix_edge: 0.0296,
                frost_wide_mix_centre: 0.7009,
                frost_wide_size_ref: 84.2326,
                frost_wide_size_drop: 0.0038,
                tone_knots: [
                    0.0, 0.0519091586529797, 0.213090511337508, 0.3630962727280779,
                    0.5198145752907551, 0.679180921179765, 0.8347696966236444,
                    0.9910065350260904, 1.0,
                ],
                post_blur_share: 0.1482,
                normal_radius_scale: 1.5358,
                lens_edge: 11.5854,
                lens_edge_decay: 0.4954,
                rim_mix: 0.3747,
                rim_mix_width: 1.1782,
                rim_mix_cut: 3.9722,
                rim_mix_luma_floor: 0.05,
                lens_band: 18.3411,
                lens_strength: -2.49,
                lens_decay: 6.8762,
                lens_size_ref: 28.7,
                rim_width: 1.2475,
                rim_intensity: 0.15,
                fill_color: Color::hex(0xFDFAFB),
                fill_opacity: 0.1581,
                fill_size_ref: 0.0,
                fill_size_drop: 0.0,
                saturation: 0.9633,
                dim: 0.0006,
                tint_strength: 0.35,
                glow_strength: 0.073,
                press_lift: 0.056,
            ..GlassBank::ZERO
        }
    }

}