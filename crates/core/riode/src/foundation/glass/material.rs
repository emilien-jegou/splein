// Single responsibility: the Liquid-Glass *lens* material the DSL authors
// with — geometry, optics and interaction. No variants, no fitted tables:
// every visible number is an explicit knob (userland presets like
// anodeui fill them in bulk); unset means neutral.
//
// The backdrop *stages* (core blur, wash, saturation, tone, ambient,
// fill) are NOT here. They live on `Backdrop` (`super::backdrop`), which
// they stay meaningful on with the lens removed entirely. `effective`
// takes the backdrop and merges its stages into the packed bank, so a
// caller that has both gets the same `GlassBank` a monolithic `Glass`
// used to produce.

use crate::foundation::Radius;

/// An immutable description of a Liquid-Glass lens: a bare SDF plus
/// explicit optics knobs. Start from `Glass::lens` (or a userland preset
/// such as anodeui's) and chain modifiers.
///
/// ```ignore
/// Glass::lens(1.0)                          // the reference lens
/// Glass::lens(4.0)                          // 4x the bend
/// Glass::lens(1.0).interactive()            // reacts to touch
/// ```
///
/// `glass={...}` carries the lens and its reference geometry
/// (strength, band, edge profile, rim shape, glass-cast shadow).
/// Every *stage* (blur, wash, saturation, tone, ambient) rides a
/// separate `Backdrop` — see `super::backdrop`.
///
/// Two riode-only fields ride along because the scene graph stamps them
/// at emit time rather than at authoring time:
///
/// * [`Self::radius`] — the node's declared corners, written by
///   `emit_layer` so the glass silhouette follows the element's actual
///   shape (`Radius::Max` becomes a pill).
/// * [`Self::interactive`] — whether the lens reacts to touch.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Glass {
    /// Draws nothing (the frame still exists, the packer skips it).
    /// `Glass::identity()` sets this; everything else leaves it off.
    pub hidden: bool,
    /// `None` when never set (⇒ not interactive), `Some` after
    /// [`Self::interactive`] was called.
    pub interactive: Option<bool>,
    /// Corner radius of the glass silhouette. `ZERO` in presets; emit
    /// overwrites it with the node's declared radius.
    pub radius: Radius,
    /// Superellipse exponent for this shape, overriding the global
    /// `GlassConstants::corner_exponent`. `None` uses the global (this is
    /// what `uInfo[N].z == 0` means in the shader).
    pub corner_exponent: Option<f32>,
    /// Union identity for Phase 5 groups (`unionId`): members sharing one
    /// merge into their bounding rect before the pack. `None` draws
    /// standalone (merges with nothing).
    pub union_id: Option<u64>,
    /// Morph identity for Phase 5 groups (`glassId`): members sharing one
    /// across frames animate rect/radius between targets on the morph
    /// spring instead of snapping. `None` never morphs.
    pub glass_id: Option<u64>,
    /// Lens strength multiplier. `glass={4.0}` bends four times harder
    /// than the base bend; 0.0 is a flat (no-bend) lens. The base bend
    /// itself is [`Self::lens_strength`] (presets set it); without one
    /// the multiplier scales nothing.
    pub strength: f32,
    /// Absolute base bend (`lensStrength`, negative samples inward).
    /// Presets set this from the fitted rows; `None` keeps the bank's
    /// value pre-deletion and means flat post-deletion.
    pub lens_strength: Option<f32>,
    /// Lens band width in logical px, overriding the table's `lens_band`.
    /// `None` uses the fitted value.
    pub band: Option<f32>,
    /// Rim (specular + isotropic edge light) overall scale, applied to
    /// the absolute intensity and white-mix values below. `None` keeps
    /// them as set; `Some(0.0)` disables the optical edge so a flat
    /// `stroke` can supply the border instead.
    pub rim: Option<f32>,
    /// Specular rim intensity, absolute (`rimIntensity`).
    pub rim_intensity: Option<f32>,
    /// Isotropic white-mix amount, absolute (`rimMix`).
    pub rim_mix: Option<f32>,
    /// Anisotropic frost power (`blurAspectPower`, 0 = isotropic).
    /// Frost-model knob: the anisotropy scales the post-lens re-blur,
    /// so it stays with the lens even though it scales the core blur's
    /// aspect too.
    pub blur_aspect_power: Option<f32>,
    /// Falloff length of `lensEdge` (`lensEdgeDecay`).
    pub lens_edge_decay: Option<f32>,
    /// Touch-glow brightness at full press (`glowStrength`) and whole-
    /// shape press lift (`pressLift`). Interaction feedback, not a look.
    pub glow_strength: Option<f32>,
    pub press_lift: Option<f32>,
    /// Post-lens tap stretch cap (`postJacobianMax`).
    pub post_jacobian_max: Option<f32>,
    /// Fresnel edge-glint gain (`fresnel.wgsl`, stage id 14). `None`
    /// keeps it off — the stage no-ops, matching every existing preset.
    pub fresnel: Option<f32>,
    /// Schlick base reflectance (`fresnel_f0`; `None`/zero = shader
    /// default 0.2).
    pub fresnel_f0: Option<f32>,
    /// Schlick exponent (`fresnel_power`; `None`/zero = shader default
    /// 5).
    pub fresnel_power: Option<f32>,
    // --- Reference-geometry knobs ------------------------------------
    // Every one of these used to ride a fitted bank behind a variant
    // flag; now each is spelled explicitly (or left neutral) and
    // userland presets fill them in bulk.
    /// Lens falloff length in logical px (`lensDecay`).
    pub lens_decay: Option<f32>,
    /// Lens size reference: shapes smaller than this get a uniformly
    /// scaled-down lens (`lensSizeRef`, 0 disables).
    pub lens_size_ref: Option<f32>,
    /// Extra inward lens offset at the outline (`lensEdge`, 0 disables).
    pub lens_edge: Option<f32>,
    /// Lens normals follow a rounder outline: corner radius × this
    /// (`normalRadiusScale`).
    pub normal_radius_scale: Option<f32>,
    /// Specular rim width in logical px (`rimWidth`).
    pub rim_width: Option<f32>,
    /// Isotropic white-mix ramp width (`rimMixWidth`) and cutoff depth
    /// (`rimMixCut`).
    pub rim_mix_width: Option<f32>,
    pub rim_mix_cut: Option<f32>,
    /// Luminance floor scaling the white mix (`rimMixLumaFloor`).
    pub rim_mix_floor: Option<f32>,
    /// Share of the frost re-blurred after the lens (`postBlurShare`).
    pub post_blur_share: Option<f32>,
    /// Wide frost tail sigma and edge/centre mixes (`frostWideSigma`,
    /// `frostWideMixEdge/Centre`).
    pub frost_wide_sigma: Option<f32>,
    pub frost_wide_mix_edge: Option<f32>,
    pub frost_wide_mix_centre: Option<f32>,
    /// Wide-tail size gating (`frostWideSizeRef`, 0 disables;
    /// `frostWideSizeDrop`).
    pub frost_wide_size_ref: Option<f32>,
    pub frost_wide_size_drop: Option<f32>,
}

impl Glass {
    const fn base() -> Self {
        Self {
            hidden: false,
            interactive: None,
            radius: Radius::ZERO,
            corner_exponent: None,
            union_id: None,
            glass_id: None,
            strength: 1.0,
            lens_strength: None,
            band: None,
            rim: None,
            rim_intensity: None,
            rim_mix: None,
            blur_aspect_power: None,
            lens_edge_decay: None,
            glow_strength: None,
            press_lift: None,
            post_jacobian_max: None,
            fresnel: None,
            fresnel_f0: None,
            fresnel_power: None,
            lens_decay: None,
            lens_size_ref: None,
            lens_edge: None,
            normal_radius_scale: None,
            rim_width: None,
            rim_mix_width: None,
            rim_mix_cut: None,
            rim_mix_floor: None,
            post_blur_share: None,
            frost_wide_sigma: None,
            frost_wide_mix_edge: None,
            frost_wide_mix_centre: None,
            frost_wide_size_ref: None,
            frost_wide_size_drop: None,
        }
    }

    /// The bare lens: neutral stages, no bend until knobs (or a
    /// userland preset) set one. `glass={4.0}` scales whatever absolute
    /// bend is set; alone it draws nothing visible.
    pub const fn lens(strength: f32) -> Self {
        Self {
            strength,
            ..Self::base()
        }
    }

    /// No effect: the frame still runs but draws nothing (the packer
    /// skips it).
    pub const fn identity() -> Self {
        Self {
            hidden: true,
            ..Self::base()
        }
    }

    /// The isolation lens as an effect: a unit-strength bare lens over
    /// neutral stages (sharp, unwashed, monochrome, no glow, no halo).
    /// It refracts without tinting or frosting — the compositional
    /// primitive for stacking under/over other effects:
    /// `Blur::new(6.0).then(Glass::clear())`.
    ///
    /// Deliberately minimal, not a look: bare geometry over neutral
    /// stages (documented here, not fitted). For Apple-matched looks,
    /// use userland presets (e.g. anodeui's `GlassPreset`) and
    /// [`GlassStyle::into_effects`](super::style::GlassStyle::into_effects).
    pub fn clear() -> crate::foundation::effect::GlassEffect {
        use crate::foundation::effect::GlassEffect;
        use super::{backdrop::Backdrop, glow::Glow, refraction::Refraction, spectral::Spectral};
        GlassEffect::single(
            Self::lens(1.0),
            Backdrop::NONE,
            Spectral::NONE,
            Glow::NONE,
            Refraction::NONE,
        )
    }

    /// A copy that reacts to touch when `enabled` (stretch, glow, bounce).
    pub const fn interactive(mut self, enabled: bool) -> Self {
        self.interactive = Some(enabled);
        self
    }

    /// A copy merging with same-`id` members into one surface
    /// (`unionId`, Phase 5 groups).
    pub const fn union_id(mut self, id: u64) -> Self {
        self.union_id = Some(id);
        self
    }

    /// A copy morphing toward same-`id` members across frames
    /// (`glassId`, Phase 5 groups).
    pub const fn glass_id(mut self, id: u64) -> Self {
        self.glass_id = Some(id);
        self
    }

    /// Lens band width in logical px, overriding the fitted table.
    pub const fn band(mut self, band: f32) -> Self {
        self.band = Some(band);
        self
    }

    /// Absolute base bend (negative samples inward). Presets carry the
    /// fitted rows here; the `strength` multiplier scales it.
    pub const fn lens_strength(mut self, v: f32) -> Self {
        self.lens_strength = Some(v);
        self
    }

    const fn band_or(mut self, band: Option<f32>) -> Self {
        self.band = band;
        self
    }

    /// Rim (optical edge light) overall scale, applied to the absolute
    /// intensity and white-mix values (`rim_intensity` / `rim_mix`
    /// builders, or the selected bank when unset). `rim(0.0)` disables
    /// the edge light so a flat `stroke` can supply the border instead.
    pub const fn rim(mut self, scale: f32) -> Self {
        self.rim = Some(scale);
        self
    }

    /// Specular rim intensity, absolute.
    pub const fn rim_intensity(mut self, v: f32) -> Self { self.rim_intensity = Some(v.max(0.0)); self }
    /// Isotropic white-mix amount, absolute.
    pub const fn rim_mix(mut self, v: f32) -> Self { self.rim_mix = Some(v.max(0.0)); self }
    /// Anisotropic frost power (0 = isotropic).
    pub const fn blur_aspect_power(mut self, v: f32) -> Self { self.blur_aspect_power = Some(v.max(0.0)); self }
    /// Falloff length of `lens_edge`.
    pub const fn lens_edge_decay(mut self, v: f32) -> Self { self.lens_edge_decay = Some(v.max(0.0)); self }
    /// Touch-glow brightness and whole-shape press lift.
    pub const fn glow(mut self, v: f32) -> Self { self.glow_strength = Some(v.max(0.0)); self }
    pub const fn press_lift(mut self, v: f32) -> Self { self.press_lift = Some(v.max(0.0)); self }
    /// Post-lens tap stretch cap.
    pub const fn post_jacobian(mut self, v: f32) -> Self { self.post_jacobian_max = Some(v.max(0.0)); self }

    /// Fresnel edge glint (`fresnel.wgsl`): `fresnel(gain)` lights the
    /// bevel by Schlick off the 3-D normal; off (no-op) until called.
    pub const fn fresnel(mut self, strength: f32) -> Self { self.fresnel = Some(strength.max(0.0)); self }

    /// Fresnel Schlick base reflectance (absolute; shader default 0.2).
    pub const fn fresnel_f0(mut self, f0: f32) -> Self { self.fresnel_f0 = Some(f0.max(0.0)); self }

    /// Fresnel exponent (absolute; shader default 5).
    pub const fn fresnel_power(mut self, p: f32) -> Self { self.fresnel_power = Some(p.max(0.0)); self }

    /// Reference-geometry knobs (one line each; `None` keeps the neutral
    /// base — zero, off, untouched. See the field docs for units).
    pub const fn lens_decay(mut self, v: f32) -> Self { self.lens_decay = Some(v.max(0.0)); self }
    pub const fn lens_size_ref(mut self, v: f32) -> Self { self.lens_size_ref = Some(v.max(0.0)); self }
    pub const fn lens_edge(mut self, v: f32) -> Self { self.lens_edge = Some(v.max(0.0)); self }
    pub const fn normal_radius(mut self, v: f32) -> Self { self.normal_radius_scale = Some(v.max(0.0)); self }
    pub const fn rim_width(mut self, v: f32) -> Self { self.rim_width = Some(v.max(0.0)); self }
    pub const fn rim_mix_width(mut self, v: f32) -> Self { self.rim_mix_width = Some(v.max(0.0)); self }
    pub const fn rim_mix_cut(mut self, v: f32) -> Self { self.rim_mix_cut = Some(v.max(0.0)); self }
    pub const fn rim_mix_floor(mut self, v: f32) -> Self { self.rim_mix_floor = Some(v.clamp(0.0, 1.0)); self }
    pub const fn post_share(mut self, v: f32) -> Self { self.post_blur_share = Some(v.clamp(0.0, 0.9)); self }
    pub const fn frost_wide(mut self, v: f32) -> Self { self.frost_wide_sigma = Some(v.max(0.0)); self }
    pub const fn frost_wide_mix(mut self, edge: f32, centre: f32) -> Self { self.frost_wide_mix_edge = Some(edge); self.frost_wide_mix_centre = Some(centre); self }
    pub const fn frost_wide_size(mut self, ref_: f32, drop: f32) -> Self { self.frost_wide_size_ref = Some(ref_.max(0.0)); self.frost_wide_size_drop = Some(drop); self }

    /// The material as a pressable `LiquidGlass` uses it: interactive
    /// unless it opted out with `interactive(false)`. Mirrors
    /// `pressableGlass` in `glass.dart`.
    pub const fn pressable(self) -> Self {
        if self.interactive.is_none() {
            self.interactive(true)
        } else {
            self
        }
    }

    /// Whether the glass reacts to touch (stretch, glow, bounce).
    pub fn is_interactive(&self) -> bool {
        self.interactive.unwrap_or(false)
    }

    /// Packed constants with this lens's explicit knobs applied, merged
    /// with the given `Backdrop`'s stages.
    ///
    /// Starts from the neutral base (zero/off/untouched — no bank, no
    /// variant). The lens writes its optics first; the backdrop then
    /// overwrites the stage slots (blur, wash, saturation, tone,
    /// ambient, fill). The result is exactly the `GlassBank` a
    /// monolithic `Glass` used to produce — the split is data-modeling,
    /// not rendering.
    pub fn effective(&self, backdrop: &super::backdrop::Backdrop) -> crate::foundation::glass::GlassBank {
        use crate::foundation::glass::GlassBank;
        let mut v = GlassBank::ZERO;
        // Absolute base bend (presets set it); the user multiplier
        // scales it. Bare `strength` scales nothing.
        v.lens_strength = self.lens_strength.unwrap_or(0.0) * self.strength;
        if let Some(b) = self.band {
            v.lens_band = b.max(0.0);
        }
        if let Some(x) = self.rim_intensity {
            v.rim_intensity = x.max(0.0);
        }
        if let Some(x) = self.rim_mix {
            v.rim_mix = x.max(0.0);
        }
        if let Some(r) = self.rim {
            v.rim_intensity *= r;
            v.rim_mix *= r;
        }
        if let Some(x) = self.blur_aspect_power {
            v.blur_aspect_power = x.max(0.0);
        }
        if let Some(x) = self.fresnel {
            v.fresnel_strength = x.max(0.0);
        }
        if let Some(x) = self.fresnel_f0 {
            v.fresnel_f0 = x.max(0.0);
        }
        if let Some(x) = self.fresnel_power {
            v.fresnel_power = x.max(0.0);
        }
        if let Some(x) = self.lens_edge_decay {
            v.lens_edge_decay = x.max(0.0);
        }
        // Lens reference-geometry overrides: `Some` wins, `None` keeps
        // the neutral base.
        if let Some(x) = self.lens_decay { v.lens_decay = x.max(0.0); }
        if let Some(x) = self.lens_size_ref { v.lens_size_ref = x.max(0.0); }
        if let Some(x) = self.lens_edge { v.lens_edge = x.max(0.0); }
        if let Some(x) = self.normal_radius_scale { v.normal_radius_scale = x.max(0.0); }
        if let Some(x) = self.rim_width { v.rim_width = x.max(0.0); }
        if let Some(x) = self.rim_mix_width { v.rim_mix_width = x.max(0.0); }
        if let Some(x) = self.rim_mix_cut { v.rim_mix_cut = x.max(0.0); }
        if let Some(x) = self.rim_mix_floor { v.rim_mix_luma_floor = x.clamp(0.0, 1.0); }
        // Frost model (lens-coupled: the wide-tail weight and the
        // post-lens Jacobian re-blur depend on the lens profile).
        if let Some(x) = self.post_blur_share { v.post_blur_share = x.clamp(0.0, crate::foundation::glass::K_MAX_POST_BLUR_SHARE); }
        if let Some(x) = self.frost_wide_sigma { v.frost_wide_sigma = x.max(0.0); }
        if let Some(x) = self.frost_wide_mix_edge { v.frost_wide_mix_edge = x; }
        if let Some(x) = self.frost_wide_mix_centre { v.frost_wide_mix_centre = x; }
        if let Some(x) = self.frost_wide_size_ref { v.frost_wide_size_ref = x.max(0.0); }
        if let Some(x) = self.frost_wide_size_drop { v.frost_wide_size_drop = x; }
        if let Some(x) = self.glow_strength { v.glow_strength = x.max(0.0); }
        if let Some(x) = self.press_lift { v.press_lift = x.max(0.0); }
        if let Some(x) = self.post_jacobian_max { v.post_jacobian_max = x.max(0.0); }
        // Backdrop stages last: they own blur_sigma, the wash, the tone
        // curve, saturation, ambient, tint strength and fill gating.
        backdrop.apply_to(&mut v);
        v
    }

    /// The single radius the shader's `uInfo[N].x` carries for a
    /// `width × height` shape: the smallest resolved corner, capped at
    /// half the shorter side — the same rule `merge_unions` keeps. A
    /// pill (`Radius::Max`) resolves equal corners, so it is unaffected.
    pub fn shape_radius(&self, width: f32, height: f32) -> f32 {
        let c = self.radius.resolve_corners(width, height);
        c.top_left
            .min(c.top_right)
            .min(c.bottom_right)
            .min(c.bottom_left)
            .min(width.min(height) / 2.0)
    }

    /// Layout-px margin the glass output can reach past the element rect.
    ///
    /// The lens samples inward for the shipped (negative) `lensStrength`,
    /// but positive strengths are legal, and several passes reach *outward*
    /// regardless. This is the union of every outward reach in the
    /// reference shader, evaluated against the variant constants this
    /// material resolves to:
    ///
    /// * lens displacement (`|lensStrength| · lensBand`) — for positive
    ///   strengths; kept even for the shipped negatives so a retune cannot
    ///   silently underpaint,
    /// * the widest wide-tail ring (`2.62 · frostWideSigma`),
    /// * the post-lens Jacobian taps (`blurSigma·√postBlurShare ·
    ///   postJacobianMax`),
    /// * the per-shape extra frost ring (`~3σ`),
    /// * the ambient 5×5 backdrop window (`ambientReach`),
    /// * the backdrop-inset band (`backdropReach + backdropInset`),
    /// * the drop shadow (`shadowRadius + |shadowOffsetY|`),
    /// * the isotropic rim's mix cut (`rimMixCut`),
    ///
    /// plus a 2 px anti-aliasing fringe. Mirrors the 3σ blur pad in visual
    /// bounds so backdrop damage just outside the glass repaints it.
    pub fn damage_pad(&self, backdrop: &super::backdrop::Backdrop) -> f32 {
        use crate::foundation::glass::composed_blur_sigma;
        let v = self.effective(backdrop);
        let lens = v.lens_strength.abs() * v.lens_band;
        let tail = 2.62 * v.frost_wide_sigma;
        let half_min = f32::INFINITY;
        let share = v.post_blur_share.clamp(0.0, crate::foundation::glass::K_MAX_POST_BLUR_SHARE);
        let post = v.blur_sigma * share.sqrt() * v.post_jacobian_max;
        let extra_frost = 3.0 * composed_blur_sigma(v, half_min);
        let ambient = v.ambient_reach;
        let rim = v.rim_mix_cut;
        let reach = lens
            .max(tail)
            .max(post)
            .max(extra_frost)
            .max(ambient)
            .max(rim);
        reach + 2.0
    }

    /// [`Self::damage_pad`] scaled to device px.
    pub fn device_pad(&self, scale: f32, backdrop: &super::backdrop::Backdrop) -> f32 {
        self.damage_pad(backdrop) * scale
    }

    /// Bit-stable identity for chunk-cache hashing and content hashes.
    /// Field order is part of the contract; append, never reorder.
    /// (The compositor no longer reads this stream — it takes the packed
    /// frame from `pack_glass_wire` instead.)
    ///
    /// The backdrop stages are NOT in this stream — they live on
    /// [`super::backdrop::Backdrop::write_bits`], which the cache hashes
    /// *in addition* to this. A stage change that omitted that would
    /// leave stale frames.
    pub fn write_bits(&self, out: &mut impl FnMut(u32)) {
        out(u32::from(self.hidden));
        out(match self.interactive {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
        match self.radius {
            Radius::Scalar(v) => {
                out(0);
                out(v.to_bits());
            }
            Radius::Max => out(1),
            Radius::Corners {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            } => {
                out(2);
                out(top_left.to_bits());
                out(top_right.to_bits());
                out(bottom_right.to_bits());
                out(bottom_left.to_bits());
            }
        }
        match self.corner_exponent {
            None => out(0),
            Some(v) => {
                out(1);
                out(v.to_bits());
            }
        }
        // Appended after v2 (Phase 5): union/morph identity. `None` is a
        // single zero word; `Some` is tag + lo + hi.
        match self.union_id {
            None => out(0),
            Some(id) => {
                out(1);
                out(id as u32);
                out((id >> 32) as u32);
            }
        }
        match self.glass_id {
            None => out(0),
            Some(id) => {
                out(1);
                out(id as u32);
                out((id >> 32) as u32);
            }
        }
        // Appended for the lens/wash split: strength, band, rim. `None`
        // is a single zero word; `Some` is tag + payload.
        out(self.strength.to_bits());
        match self.band {
            None => out(0),
            Some(v) => {
                out(1);
                out(v.to_bits());
            }
        }
        match self.rim {
            None => out(0),
            Some(v) => {
                out(1);
                out(v.to_bits());
            }
        }
        // Reference-geometry and optics knobs, appended in declaration
        // order. (The stage knobs that used to be interleaved here moved
        // to `Backdrop::write_bits`.)
        for opt in [
            self.lens_strength,
            self.glow_strength,
            self.press_lift,
            self.post_jacobian_max,
            self.rim_intensity,
            self.rim_mix,
            self.blur_aspect_power,
            self.lens_edge_decay,
            self.lens_decay,
            self.lens_size_ref,
            self.lens_edge,
            self.normal_radius_scale,
            self.rim_width,
            self.rim_mix_width,
            self.rim_mix_cut,
            self.rim_mix_floor,
            self.post_blur_share,
            self.fresnel,
            self.fresnel_f0,
            self.fresnel_power,
            self.frost_wide_sigma,
            self.frost_wide_mix_edge,
            self.frost_wide_mix_centre,
            self.frost_wide_size_ref,
            self.frost_wide_size_drop,
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

/// Lerps 9 tone knots toward the identity ramp (`i / 8`) by `t`.
/// The shader's hat-sum LUT is linear in its knots, so this CPU-side
/// lerp is exactly a tone-amount mix in-shader — with no WGSL change.
pub(crate) fn lerp_knots(knots: [f32; 9], t: f32) -> [f32; 9] {
    let mut out = knots;
    for (i, k) in out.iter_mut().enumerate() {
        let id = i as f32 / 8.0;
        *k = id + (*k - id) * t;
    }
    out
}

impl From<f32> for Glass {
    /// `glass={4.0}` is `Glass::lens(4.0)`: lens strength only.
    fn from(strength: f32) -> Self {
        Self::lens(strength)
    }
}

impl From<&str> for Glass {
    /// `"4"`, `"4 5px"`: strength plus optional band width, extensible.
    fn from(s: &str) -> Self {
        parse_lens_shorthand(s)
    }
}

impl From<String> for Glass {
    fn from(s: String) -> Self {
        parse_lens_shorthand(&s)
    }
}

fn parse_lens_shorthand(s: &str) -> Glass {
    let mut parts = s.split_whitespace();
    let strength: f32 = parts
        .next()
        .unwrap_or("1")
        .trim_end_matches("px")
        .parse()
        .unwrap_or_else(|_| panic!("invalid glass shorthand `{s}`: want `\"strength [bandpx]\"`"));
    let band = parts.next().map(|p| {
        p.trim_end_matches("px").parse().unwrap_or_else(|_| {
            panic!("invalid glass shorthand `{s}`: band must be a number like `5px`")
        })
    });
    Glass::lens(strength).band_or(band)
}

impl Default for Glass {
    fn default() -> Self {
        Self::lens(1.0)
    }
}
