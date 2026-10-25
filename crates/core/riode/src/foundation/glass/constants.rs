// Single responsibility: the packed glass-constant block.
//
// The layout is transcribed from
// `adaptive_liquid_glass/lib/src/core/glass_variant_constants.dart` so a
// diff against the reference stays reviewable. Every length is in
// logical px; the uniform pack scales by the device scale.
//
// This struct holds no looks: `ZERO` is the neutral base (all zeros,
// identity tone, untouched saturation) and every visible number arrives
// per shape through explicit knobs. The fitted per-look numbers used to
// live here as variant rows; they moved to userland
// (`riode-anodeui`), which is the only place looks exist.

use crate::foundation::Color;

use super::motion::GlassMotionConstants;

/// Grey tone LUT output knots at inputs `i / 8`, `i = 0..8`. Reproduces the
/// input exactly (the LUT becomes a no-op).
pub const IDENTITY_TONE_KNOTS: [f32; 9] = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];

/// One packed bank of rendering constants (lengths in logical px).
/// Transcribed from the reference's per-variant constants; see that
/// source for the measured provenance of each field. In riode exactly
/// one bank ships per frame, fully explicit — there are no variants.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlassBank {
    /// Gaussian sigma of the frost blur (logical px), applied as a backdrop
    /// blur before the optics pass.
    pub blur_sigma: f32,
    /// Shapes whose half shorter side is below this get a proportionally
    /// smaller frost blur: `σ = blurSigma × min(1, halfMin / blurSizeRef)`.
    /// 0 disables the scaling.
    pub blur_size_ref: f32,
    /// Sigma of the wide frost tail added in-shader on top of the core
    /// (logical px; 0 disables it). `(1−w)·G(core) + w·G(core ⊕ tail)`.
    pub frost_wide_sigma: f32,
    /// Wide-tail weight where the lens samples at the outline.
    pub frost_wide_mix_edge: f32,
    /// Wide-tail weight where the lens samples at the shape's centre line.
    pub frost_wide_mix_centre: f32,
    /// Shapes whose half shorter side is below this lose wide-tail weight.
    /// 0 disables the size term.
    pub frost_wide_size_ref: f32,
    /// Wide-tail weight lost per unit of `1 − halfMin / frostWideSizeRef`.
    pub frost_wide_size_drop: f32,
    /// Grey tone LUT output knots at inputs `i/8`. Applied after the
    /// saturation and before the fill wash.
    pub tone_knots: [f32; 9],
    /// Grey tone LUT for small shapes (half shorter side ≤ `smallSizeLo`),
    /// applied after the fill wash and dim.
    pub small_tone_knots: [f32; 9],
    /// Half shorter side (logical px) at and below which `smallToneKnots` is
    /// applied at full weight.
    pub small_size_lo: f32,
    /// Half shorter side at and above which the small-shape tone curve is
    /// off; linear in between. ≤ 0 disables it.
    pub small_size_hi: f32,
    /// Largest stretch of the post-lens blur taps through the lens Jacobian.
    pub post_jacobian_max: f32,
    /// Strength of the ambient colour (backdrop average chroma carried
    /// through). 0 = off.
    pub ambient_mix: f32,
    /// How far (logical px) the ambient colour's average reaches beyond the
    /// shape's rect on each side.
    pub ambient_reach: f32,
    /// How far the tone LUT is keyed on luma instead of applied per channel
    /// (0 = per channel).
    pub tone_luma_mix: f32,
    /// Brightness of the touch glow at full press.
    pub glow_strength: f32,
    /// White added over the whole of a pressed shape.
    pub press_lift: f32,
    /// Share of the frost blur's variance applied after the lens instead of
    /// before it (0…0.9).
    pub post_blur_share: f32,
    /// Lens normals follow a rounder outline: each corner radius × this,
    /// capped at half the shorter side. 1 uses the outline itself.
    pub normal_radius_scale: f32,
    /// Extra inward lens offset at the outline (logical px), falling off as
    /// `exp(−depth / lensEdgeDecay)`. 0 disables it.
    pub lens_edge: f32,
    /// Falloff length of `lensEdge` (logical px).
    pub lens_edge_decay: f32,
    /// Isotropic rim: outer pixels mix toward white by this alpha, ramping
    /// to 0 at `rimMixWidth`, cut at `rimMixCut`. 0 disables it.
    pub rim_mix: f32,
    /// Depth (logical px) where the `rimMix` ramp reaches 0.
    pub rim_mix_width: f32,
    /// Depth (logical px) where `rimMix` is cut off.
    pub rim_mix_cut: f32,
    /// `rimMix` is scaled by `floor + (1−floor)·min(1, luma / 0.5)` of the
    /// glass under it; 1 disables the scaling.
    pub rim_mix_luma_floor: f32,
    /// Small-shape shadow lift: after the tone LUT each channel gains
    /// `toneLift · size · max(0, 1 − c / toneLiftKnee)²`. 0 disables it.
    pub tone_lift: f32,
    /// Anisotropic frost: `sigmaX = σ·(h/w)^power`, `sigmaY = σ·(w/h)^power`.
    /// 0 = isotropic.
    pub blur_aspect_power: f32,
    /// Input level where `toneLift` fades to 0.
    pub tone_lift_knee: f32,
    /// Half shorter side (logical px) at and above which `toneLift` is 0;
    /// 0 disables the lift.
    pub tone_lift_size_ref: f32,
    /// Width of the specular rim.
    pub rim_width: f32,
    /// Brightness added at the rim.
    pub rim_intensity: f32,
    /// Rim strength on the side facing away from the light, relative to the
    /// lit side.
    pub rim_back: f32,
    /// Schlick base reflectance of the Fresnel edge glint (`fresnel.wgsl`
    /// defaults 0.2 when zero). Rider of the L bank's retired slot.
    pub fresnel_f0: f32,
    /// Fresnel exponent (`fresnel.wgsl` defaults 5 = Schlick when zero).
    pub fresnel_power: f32,
    /// Fresnel edge-glint gain. 0 = off (the stage no-ops) — the
    /// conservative default that keeps every preset unchanged.
    pub fresnel_strength: f32,
    /// Depth inside the outline where the edge lens ends (displacement 0).
    pub lens_band: f32,
    /// Edge displacement as a multiple of `lensBand`; negative samples
    /// inward (SwiftUI ≈ −2.6: outer pixels show content from ~47 pt inside,
    /// mirrored and compressed).
    pub lens_strength: f32,
    /// Exponential falloff length of the lens displacement.
    pub lens_decay: f32,
    /// Shapes whose half shorter side is below this get a uniformly scaled
    /// down lens; 0 disables it.
    pub lens_size_ref: f32,
    /// Base index of refraction the spectral spread deviates from (the
    /// `Spectral` source writes this; monochrome leaves it 0/unused).
    pub spectral_ior: f32,
    /// Dispersion strength across the spectrum (`Spectral::spread`). 0 =
    /// monochrome, no rainbow.
    pub spectral_spread: f32,
    /// How far the rainbow extends relative to the lens displacement
    /// (`Spectral::reach`).
    pub spectral_reach: f32,
    /// Overall gain of the voice-glow light inside the slab (`Glow`).
    /// 0 = off (the shader takes the no-glow path).
    pub glow_voice: f32,
    /// Gain of the glow added to the refracted ray inside the slab.
    pub glow_inside: f32,
    /// Gain of the outer refraction halo (`Refraction`). 0 = off (the
    /// shader takes the no-halo path).
    pub refraction_strength: f32,
    /// Colour the content behind the glass is washed toward.
    pub fill_color: Color,
    /// How far the content is mixed toward `fillColor` (0 = untouched).
    pub fill_opacity: f32,
    /// Shapes whose half shorter side is below this get a weaker fill. 0
    /// disables it.
    pub fill_size_ref: f32,
    /// Fraction of `fillOpacity` lost on a vanishingly small shape.
    pub fill_size_drop: f32,
    /// Saturation of the content behind the glass (1 = unchanged).
    pub saturation: f32,
    /// Darkening applied after the fill (used by `clear`).
    pub dim: f32,
    /// How strongly a material tint colours the glass.
    pub tint_strength: f32,
}

impl GlassBank {
    /// All-zero constants with identity tone knots: the neutral base the
    /// packer starts from. Every visible number arrives through explicit
    /// knobs; unset means zero/off/untouched.
    pub const ZERO: Self = Self {
        blur_sigma: 0.0,
        blur_size_ref: 0.0,
        frost_wide_sigma: 0.0,
        frost_wide_mix_edge: 0.0,
        frost_wide_mix_centre: 0.0,
        frost_wide_size_ref: 0.0,
        frost_wide_size_drop: 0.0,
        tone_knots: IDENTITY_TONE_KNOTS,
        small_tone_knots: IDENTITY_TONE_KNOTS,
        small_size_lo: 0.0,
        small_size_hi: 0.0,
        post_jacobian_max: 4.0,
        ambient_mix: 0.0,
        ambient_reach: 0.0,
        tone_luma_mix: 0.0,
        glow_strength: 0.0,
        press_lift: 0.0,
        post_blur_share: 0.0,
        normal_radius_scale: 1.0,
        lens_edge: 0.0,
        lens_edge_decay: 0.6,
        rim_mix: 0.0,
        rim_mix_width: 1.5,
        rim_mix_cut: 1.0,
        rim_mix_luma_floor: 1.0,
        tone_lift: 0.0,
        blur_aspect_power: 0.0,
        tone_lift_knee: 0.5,
        tone_lift_size_ref: 0.0,
        rim_width: 0.0,
        rim_intensity: 0.0,
        rim_back: 0.35,
        fresnel_f0: 0.0,
        fresnel_power: 0.0,
        fresnel_strength: 0.0,
        lens_band: 0.0,
        lens_strength: 0.0,
        lens_decay: 0.0,
        lens_size_ref: 0.0,
        spectral_ior: 0.0,
        spectral_spread: 0.0,
        spectral_reach: 0.0,
        glow_voice: 0.0,
        glow_inside: 0.0,
        refraction_strength: 0.0,
        fill_color: Color::WHITE,
        fill_opacity: 0.0,
        fill_size_ref: 0.0,
        fill_size_drop: 0.0,
        saturation: 1.0,
        dim: 0.0,
        tint_strength: 0.0,
    };
}

/// Engine-wide glass shape globals (no looks: no variant sets — the
/// fitted rows moved to userland, see `riode-anodeui`). Only outline
/// geometry and motion live here; every visual value arrives per shape
/// through explicit knobs.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlassConstants {
    /// Superellipse exponent approximating Apple's continuous corners
    /// (rectangles only; capsules and circles stay exact circular arcs).
    pub corner_exponent: f32,
    /// Continuous-corner zone as a multiple of the corner radius (outline
    /// only; the lens field keeps circular corners).
    pub corner_zone: f32,
    /// Smooth-union radius as a multiple of the group's spacing.
    pub merge_factor: f32,
    /// Motion constants for `.interactive()` and `glassId` morphs
    /// (`GlassMotionConstants` in the reference; part of `GlassConstants`).
    pub motion: GlassMotionConstants,
}

/// Engine-wide shape globals, evaluated once at compile time.
pub const GLASS_STANDARD: GlassConstants = GlassConstants::standard();

impl GlassConstants {
    /// Outline geometry + motion only. No looks ship here.
    pub const fn standard() -> Self {
        Self {
            // Task A1 (model sweep 1.0–1.6 against SwiftUI's `.continuous`
            // rects, then device): outline only; the lens normals keep
            // circular corners. Merge scenes (device): median SSIM 0.979.
            corner_exponent: 2.0,
            corner_zone: 1.2,
            merge_factor: 0.8,
            motion: GlassMotionConstants::standard(),
        }
    }
}
