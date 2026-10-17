// Single responsibility: the non-glass backdrop stages.
//
// `Glass` is the lens material — the SDF, band, rim and optics. The
// stages below are what the lens *samples*: the core blur, the wash,
// saturation, tone, ambient pickup and fill. Every one of them stays
// meaningful if the lens is removed entirely (a plain blurred,
// desaturated panel), so they live here rather than on `Glass`.
//
// There is a deliberate split between the **core blur** and the **frost
// model**. *Blur* is the single Gaussian pass the engine runs over the
// snapshot (`run_blur_pass` at `composed_blur_sigma`). *Frost* is the
// multi-scale treatment built on top: the wide 16-tap tail, the
// post-lens Jacobian re-blur and the per-shape extra taps, whose
// weight is lens-coupled. The core blur moves up here; the frost model
// stays on `Glass`.

use super::constants::GlassBank;
use super::material::lerp_knots;
use super::tone::{ToneCurve, FIXED_REFERENCE_CURVE};
use crate::foundation::Color;

/// The backdrop stages: what a glass lens samples, and what an
/// un-lensed panel would still show.
///
/// `None` on any field means "neutral" — the value that leaves the
/// backdrop untouched (sharp, unsaturated, unwashed, un-toned). There
/// is no table fallback; every visible number is spelled at the call
/// site or by a userland preset.
///
/// ```ignore
/// Backdrop::new().blur(12.0).saturation(1.2).tint(Color::hex(0x112233))
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Backdrop {
    /// Core backdrop-blur sigma in logical px. `None` (absent prop)
    /// means neutral: a sharp, unblurred backdrop.
    pub blur: Option<f32>,
    /// Shapes whose half shorter side is below this get a proportionally
    /// smaller core blur (`blurSizeRef`, 0 disables).
    pub blur_size_ref: Option<f32>,
    /// Backdrop saturation at the shader's saturate stage (`1.0` =
    /// untouched, `0.0` = grayscale). `None` means neutral.
    pub saturation: Option<f32>,
    /// Backdrop tone-curve amount. `None` keeps the reference curve at
    /// full strength; `Some(t)` lerps every knot toward identity,
    /// `Some(0.0)` rendering the backdrop linearly.
    pub tone: Option<f32>,
    /// Full tone curve, replacing the engine's fixed reference curve
    /// wholesale. Wins over `tone` (which then scales this curve).
    pub tone_curve: Option<ToneCurve>,
    /// Backdrop wash. `None` (absent prop) means neutral: no wash.
    /// `Some(None)` also disables it; `Some(Some(c))` uses `c.rgb` with
    /// opacity `c.a`.
    pub wash: Option<Option<Color>>,
    /// Colour blended over the glass body (alpha is the tint strength,
    /// scaled by `tint_strength` at pack time). `None` = no tint.
    pub tint: Option<Color>,
    /// How strongly the tint colours the glass (`tintStrength`).
    pub tint_strength: Option<f32>,
    /// Post-tone multiply (`dim`, ~0.005: sub-visible calibration).
    pub dim: Option<f32>,
    /// Backdrop hue pickup (`ambientMix`, 0–1).
    pub ambient_mix: Option<f32>,
    /// Backdrop area the ambient hue averages over (`ambientReach`).
    pub ambient_reach: Option<f32>,
    /// Fill size gating (`fillSizeRef`, 0 disables). Inert in every
    /// shipped preset (`fill_size_drop` is 0 everywhere) — carried for
    /// exactness so no bank value hides.
    pub fill_size_ref: Option<f32>,
    /// Fraction of the fill opacity lost on a vanishingly small shape
    /// (`fillSizeDrop`).
    pub fill_size_drop: Option<f32>,
}

impl Default for Backdrop {
    fn default() -> Self {
        Self::NONE
    }
}

impl Backdrop {
    /// Every stage neutral: a sharp, unsaturated, untoned backdrop.
    pub const NONE: Self = Self {
        blur: None,
        blur_size_ref: None,
        saturation: None,
        tone: None,
        tone_curve: None,
        wash: None,
        tint: None,
        tint_strength: None,
        dim: None,
        ambient_mix: None,
        ambient_reach: None,
        fill_size_ref: None,
        fill_size_drop: None,
    };

    /// The neutral starting point. `Backdrop::new().blur(12.0)` blurs
    /// and leaves every other stage untouched.
    pub const fn new() -> Self {
        Self::NONE
    }

    /// Core backdrop-blur sigma in logical px. 0.0 is a sharp backdrop.
    pub const fn blur(mut self, sigma: f32) -> Self {
        self.blur = Some(sigma.max(0.0));
        self
    }

    /// Shapes below `ref_` lose core blur in proportion (`blurSizeRef`,
    /// 0 disables).
    pub const fn blur_size_ref(mut self, ref_: f32) -> Self {
        self.blur_size_ref = Some(ref_.max(0.0));
        self
    }

    /// Backdrop saturation (1.0 = untouched, 0.0 = grayscale).
    pub const fn saturation(mut self, s: f32) -> Self {
        self.saturation = Some(s.max(0.0));
        self
    }

    /// Backdrop tone-curve amount (1.0 = reference curve, 0.0 = linear
    /// passthrough).
    pub const fn tone(mut self, t: f32) -> Self {
        self.tone = Some(t.clamp(0.0, 1.0));
        self
    }

    /// Full tone curve, replacing the engine's reference curve.
    pub const fn tone_curve(mut self, c: ToneCurve) -> Self {
        self.tone_curve = Some(c);
        self
    }

    /// Backdrop wash toward `color` (opacity from the color's alpha).
    pub const fn wash(mut self, color: Color) -> Self {
        self.wash = Some(Some(color));
        self
    }

    /// No backdrop wash (the backdrop passes through un-tinted).
    pub const fn no_wash(mut self) -> Self {
        self.wash = Some(None);
        self
    }

    /// Colour blended over the glass body. Alpha is the tint strength
    /// (further scaled by [`Self::tint_strength`]).
    pub const fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }

    /// How strongly the tint colours the glass.
    pub const fn tint_strength(mut self, v: f32) -> Self {
        self.tint_strength = Some(v.max(0.0));
        self
    }

    /// Post-tone multiply (sub-visible calibration).
    pub const fn dim(mut self, v: f32) -> Self {
        self.dim = Some(v.clamp(0.0, 1.0));
        self
    }

    /// Backdrop hue pickup (0–1).
    pub const fn ambient(mut self, v: f32) -> Self {
        self.ambient_mix = Some(v.clamp(0.0, 1.0));
        self
    }

    /// Backdrop area the ambient hue averages over.
    pub const fn ambient_reach(mut self, v: f32) -> Self {
        self.ambient_reach = Some(v.max(0.0));
        self
    }

    /// Fill size gating (`fillSizeRef`, 0 disables; `fillSizeDrop`).
    pub const fn fill_size(mut self, ref_: f32, drop: f32) -> Self {
        self.fill_size_ref = Some(ref_.max(0.0));
        self.fill_size_drop = Some(drop);
        self
    }

    /// Writes this backdrop's stage knobs into a neutral `GlassBank`.
    ///
    /// `None` fields resolve to the neutral stage value (`blur_sigma` 0,
    /// `saturation` 1, `fill_opacity` 0, the fixed reference tone curve,
    /// `dim`/`ambient`/`fill_size_*` off). `tint` is *not* applied here
    /// — it is a per-shape uniform, packed separately, not a bank slot.
    pub fn apply_to(&self, v: &mut GlassBank) {
        // Core blur (the single Gaussian pass the engine runs).
        v.blur_sigma = self.blur.map(|s| s.max(0.0)).unwrap_or(0.0);
        if let Some(x) = self.blur_size_ref {
            v.blur_size_ref = x.max(0.0);
        }
        // Wash → fill colour/opacity.
        match self.wash {
            None | Some(None) => v.fill_opacity = 0.0,
            Some(Some(c)) => {
                v.fill_color = Color::rgba(c.r, c.g, c.b, 1.0);
                v.fill_opacity = c.a.clamp(0.0, 1.0);
            }
        }
        v.saturation = self.saturation.map(|s| s.max(0.0)).unwrap_or(1.0);
        // Tone curve: an explicit userland curve wins; otherwise the
        // engine's fixed reference curve. The amount lerps every knot
        // toward identity and scales the luma blend and lift.
        let base = self.tone_curve.unwrap_or(FIXED_REFERENCE_CURVE);
        let t = self.tone.map(|t| t.clamp(0.0, 1.0)).unwrap_or(1.0);
        v.tone_knots = base.knots;
        v.small_tone_knots = base.small_knots;
        v.small_size_lo = base.small_lo;
        v.small_size_hi = base.small_hi;
        v.tone_luma_mix = base.luma_mix;
        v.tone_lift = base.lift;
        v.tone_lift_knee = base.lift_knee;
        v.tone_lift_size_ref = base.lift_size_ref;
        if t < 1.0 {
            // `id + (k - id)` may not round-trip bit-identically, hence
            // the exact-1.0 skip: a fitted curve must survive untouched.
            v.tone_knots = lerp_knots(v.tone_knots, t);
            v.small_tone_knots = lerp_knots(v.small_tone_knots, t);
            v.tone_luma_mix *= t;
            v.tone_lift *= t;
        }
        // Remaining stages.
        v.tint_strength = self.tint_strength.map(|x| x.max(0.0)).unwrap_or(0.0);
        v.dim = self.dim.map(|d| d.clamp(0.0, 1.0)).unwrap_or(0.0);
        v.ambient_mix = self.ambient_mix.map(|a| a.clamp(0.0, 1.0)).unwrap_or(0.0);
        v.ambient_reach = self.ambient_reach.map(|a| a.max(0.0)).unwrap_or(0.0);
        v.fill_size_ref = self.fill_size_ref.map(|r| r.max(0.0)).unwrap_or(0.0);
        v.fill_size_drop = self.fill_size_drop.unwrap_or(0.0);
    }

    /// Bit-stable identity for chunk-cache hashing and content hashes.
    /// Field order is part of the contract; append, never reorder.
    ///
    /// This is the stream the cache hashes *in addition to*
    /// `Glass::write_bits` — a stage change that omitted it would leave
    /// stale frames.
    pub fn write_bits(&self, out: &mut impl FnMut(u32)) {
        for opt in [
            self.blur,
            self.blur_size_ref,
            self.saturation,
            self.tone,
        ] {
            match opt {
                None => out(0),
                Some(v) => {
                    out(1);
                    out(v.to_bits());
                }
            }
        }
        match self.tone_curve {
            None => out(0),
            Some(c) => {
                out(1);
                for k in c.knots {
                    out(k.to_bits());
                }
                for k in c.small_knots {
                    out(k.to_bits());
                }
                out(c.small_lo.to_bits());
                out(c.small_hi.to_bits());
                out(c.luma_mix.to_bits());
                out(c.lift.to_bits());
                out(c.lift_knee.to_bits());
                out(c.lift_size_ref.to_bits());
            }
        }
        match self.wash {
            None => out(0),
            Some(None) => out(1),
            Some(Some(c)) => {
                out(2);
                out(c.r.to_bits());
                out(c.g.to_bits());
                out(c.b.to_bits());
                out(c.a.to_bits());
            }
        }
        match self.tint {
            None => out(0),
            Some(c) => {
                out(1);
                out(c.r.to_bits());
                out(c.g.to_bits());
                out(c.b.to_bits());
                out(c.a.to_bits());
            }
        }
        for opt in [
            self.tint_strength,
            self.dim,
            self.ambient_mix,
            self.ambient_reach,
            self.fill_size_ref,
            self.fill_size_drop,
        ] {
            match opt {
                None => out(0),
                Some(v) => {
                    out(1);
                    out(v.to_bits());
                }
            }
        }
    }
}
