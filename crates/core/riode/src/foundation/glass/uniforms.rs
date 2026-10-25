// Single responsibility: the glass frame uniform layout and its packer —
// a line-for-line port of
// `adaptive_liquid_glass/lib/src/shader/glass_frame_uniforms.dart`
// (offsets, per-shape records, composed-blur helpers, union merge, pack).
//
// The layout is transcribed, not derived: the WGSL in
// `shaders/liquid_glass.frag` documents the same table, so a diff against
// the reference stays mechanical. Every *length* is multiplied by the
// device scale at pack time; every dimensionless scalar goes through
// untouched.

use crate::foundation::glass::{GlassConstants, GlassLayer, GlassMember, GlassBank};

/// Default light direction, radians, y-down: the reference's
/// `LiquidGlassThemeData.lightAngle` default (`-3π/4`, upper-left key
/// light). riode has no theme object yet, so the pack takes it as a
/// constant and material-level light control arrives with the DSL phase.
pub const DEFAULT_LIGHT_ANGLE: f32 = -2.3561945; // -3π/4
use crate::foundation::{Color, Glow, Refraction, ResolvedRect, Spectral};

/// First float index after the engine-owned `uSize` vec2.
pub const K_FIRST_USER_FLOAT: usize = 2;

/// Shapes one shader pass can draw (`uRects[24]` in the shader).
pub const K_MAX_PASS_SHAPES: usize = 24;

/// First float of each per-shape array and of `uVar`, after `uSize`.
pub const K_RECTS_AT: usize = 16;

/// See [`K_RECTS_AT`].
pub const K_INFO_AT: usize = K_RECTS_AT + 4 * K_MAX_PASS_SHAPES;

/// See [`K_RECTS_AT`].
pub const K_TINTS_AT: usize = K_INFO_AT + 4 * K_MAX_PASS_SHAPES;

/// See [`K_RECTS_AT`].
pub const K_VAR_AT: usize = K_TINTS_AT + 4 * K_MAX_PASS_SHAPES;

/// See [`K_RECTS_AT`].
pub const K_EXTRA_AT: usize = K_VAR_AT + 4 * 36;

/// See [`K_RECTS_AT`].
pub const K_CLIP_AT: usize = K_EXTRA_AT + 4 * K_MAX_PASS_SHAPES;

/// See [`K_RECTS_AT`].
pub const K_BLUR_AT: usize = K_CLIP_AT + 4 * K_MAX_PASS_SHAPES;

/// First float after the blur taps: the glow tail (wgpu-only).
pub const K_GLOW_AT: usize = K_BLUR_AT + 4 * K_MAX_PASS_SHAPES;

/// Glow tail vec4s: 6 lobe (centre, sigma, amplitude, 0) + 6 colours
/// (rgb, 0) + geom (cross-edge line, sigma, axis, blend mode id — see
/// [`crate::foundation::BlendMode`]) + outer rule (halo reach, tap
/// count, boost, alpha cap — `Refraction`).
pub const K_GLOW_VEC4S: usize = 14;

/// Floats up to and including the glow tail: the frozen compositor-wire
/// stream length (`GLASS_WIRE_PACKED_FLOATS`) — the plugin's GLSL port
/// reads exactly this and never sees the stage list appended after it.
pub const K_GLASS_WIRE_FLOATS: usize = K_GLOW_AT + 4 * K_GLOW_VEC4S;

/// Stage list (wgpu-only, appended after the glow tail): count in the
/// first float, then up to 15 author-ordered glass-stage ids (`fs_main`'s
/// dispatch), pad to the vec4 boundary. Count 0 = the canonical chain.
pub const K_STAGES_FLOATS: usize = 16;

/// GGX knobs (wgpu-only, appended after the stage list): roughness,
/// gain, light direction xyz — 2 vec4s, 5 used. Not in the v5 wire.
pub const K_GGX_FLOATS: usize = 8;

/// Number of user floats after `uSize`: the packed buffer length in `f32`.
pub const K_GLASS_UNIFORM_FLOATS: usize =
    K_GLASS_WIRE_FLOATS + K_STAGES_FLOATS + K_GGX_FLOATS;

/// Largest usable `postBlurShare` (the composed blur keeps `√0.1` of σ).
pub const K_MAX_POST_BLUR_SHARE: f32 = 0.9;

/// One shape as the shader draws it: a device-space rect + radius and
/// the per-shape overrides. Constants are single-bank: every value
/// arrives through explicit knobs (see `Glass::effective`), the
/// `uInfo[N].y` slot always ships 0, and hidden shapes are dropped by
/// the packer.
///
/// Multi-shape frames build a `Vec` of these (one per group after
/// [`merge_unions`]); the wgpu path ships one per layer for now.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlassShapeUniform {
    /// Shape rect in device px (`uRects[N]`).
    pub rect: ResolvedRect,
    /// Corner radius in device px (`uInfo[N].x`). Follows the smaller axis
    /// scale of any applied press deformation.
    pub radius: f32,
    /// Draw-nothing marker (`Glass::identity()`); dropped by the packer.
    pub hidden: bool,
    /// Material tint (`uTints[N]`: rgb, `tintStrength · a`). `None` packs
    /// zeroes.
    pub tint: Option<Color>,
    /// Shapes sharing a `unionId` merge into their bounding rect before the
    /// pack (not sent to the shader).
    pub union_id: Option<u64>,
    /// Superellipse exponent, overriding the global (`uInfo[N].z`; 0 =
    /// use the global `cornerExponent` / `cornerZone`).
    pub corner_exponent: Option<f32>,
    /// Smooth-union group index (`uExtra[N].x`). Same-group shapes smin
    /// together.
    pub group: i32,
    /// Per-shape smooth-min radius in device px, overriding the frame's
    /// (`uExtra[N].y`).
    pub smoothing: Option<f32>,
    /// Layer opacity folded into the glass (`uExtra[N].z`).
    pub opacity: f32,
    /// Backdrop clip rect in device px (`uClip[N]`; `±1e9` when absent).
    pub clip: Option<ResolvedRect>,
    /// Press light 0..1 (`uExtra[N].w`).
    pub light: f32,
    /// Composed frost sigma this shape wants, logical px (`uBlur[N].z`'s
    /// source; the packer stores the *extra* beyond the input's blur).
    pub blur_sigma: f32,
    /// sigmaX / sigma of [`Self::blur_sigma`] (aspect term of
    /// `blurAspectPower`).
    pub blur_aspect: f32,
    /// Lens strength multiplier (from `Glass::strength`).
    pub strength: f32,
    /// Absolute base bend (from `Glass::lens_strength`).
    pub lens_strength: Option<f32>,
    /// Lens band width in logical px (`None` = no band: flat lens).
    pub band: Option<f32>,
    /// Rim overall scale (`None` = as set).
    pub rim: Option<f32>,
    /// Absolute rim intensity / white mix (`None` = no edge light).
    pub rim_intensity: Option<f32>,
    pub rim_mix: Option<f32>,
    /// Aspect power, lens-edge decay (`None` = disabled).
    pub blur_aspect_power: Option<f32>,
    pub lens_edge_decay: Option<f32>,
    /// Frost sigma in logical px (`None` = sharp, unblurred).
    pub frost_sigma: Option<f32>,
    /// Wash (`None` = no wash).
    pub wash: Option<Option<Color>>,
    /// Saturation (`None` = 1.0, untouched).
    pub saturation: Option<f32>,
    /// Tone-curve amount (`None` = reference curve at full strength).
    pub tone: Option<f32>,
    /// Full tone curve (`None` = engine reference curve).
    pub tone_curve: Option<crate::foundation::glass::ToneCurve>,
    /// Interaction feedback, dim calibration, tap cap, ambient reach
    /// (`None` = fitted bank value).
    pub glow_strength: Option<f32>,
    pub press_lift: Option<f32>,
    pub dim: Option<f32>,
    pub post_jacobian_max: Option<f32>,
    pub ambient_reach: Option<f32>,
    /// Reference-geometry knobs (`None` = neutral: 0, off).
    /// See `Glass` field docs for units.
    pub lens_decay: Option<f32>,
    pub lens_size_ref: Option<f32>,
    pub lens_edge: Option<f32>,
    pub normal_radius_scale: Option<f32>,
    pub rim_width: Option<f32>,
    pub rim_mix_width: Option<f32>,
    pub rim_mix_cut: Option<f32>,
    pub rim_mix_floor: Option<f32>,
    pub tint_strength: Option<f32>,
    pub ambient_mix: Option<f32>,
    pub post_blur_share: Option<f32>,
    pub frost_wide_sigma: Option<f32>,
    pub frost_wide_mix_edge: Option<f32>,
    pub frost_wide_mix_centre: Option<f32>,
    pub frost_wide_size_ref: Option<f32>,
    pub frost_wide_size_drop: Option<f32>,
    pub blur_size_ref: Option<f32>,
    pub fill_size_ref: Option<f32>,
    pub fill_size_drop: Option<f32>,
    /// Fresnel edge-glint knobs (`fresnel.wgsl`, stage 14): gain, F0
    /// base reflectance, Schlick exponent. `None` = shader defaults
    /// (gain 0 = no-op, F0 0.2, power 5).
    pub fresnel_strength: Option<f32>,
    pub fresnel_f0: Option<f32>,
    pub fresnel_power: Option<f32>,
    /// GGX glint knobs (`ggx.wgsl`, stage 15): roughness α, gain, light
    /// direction (surface→light, normalized on the CPU). `None` =
    /// shader defaults (gain 0 = no-op, roughness 0.2, key light).
    pub ggx_roughness: Option<f32>,
    pub ggx_gain: Option<f32>,
    pub ggx_light_dir: Option<[f32; 3]>,
}

impl GlassShapeUniform {
    /// A plain, undeformed shape: full opacity, no tint, no clip, no
    /// press light, no extra blur. `rect`/`radius` in device px.
    pub const fn new(rect: ResolvedRect, radius: f32, hidden: bool) -> Self {
        Self {
            rect,
            radius,
            hidden,
            tint: None,
            union_id: None,
            corner_exponent: None,
            group: 0,
            smoothing: None,
            opacity: 1.0,
            clip: None,
            light: 0.0,
            blur_sigma: 0.0,
            blur_aspect: 1.0,
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
            dim: None,
            post_jacobian_max: None,
            ambient_reach: None,
            frost_sigma: None,
            wash: None,
            saturation: None,
            tone: None,
            tone_curve: None,
            lens_decay: None,
            lens_size_ref: None,
            lens_edge: None,
            normal_radius_scale: None,
            rim_width: None,
            rim_mix_width: None,
            rim_mix_cut: None,
            rim_mix_floor: None,
            tint_strength: None,
            ambient_mix: None,
            post_blur_share: None,
            frost_wide_sigma: None,
            frost_wide_mix_edge: None,
            frost_wide_mix_centre: None,
            frost_wide_size_ref: None,
            frost_wide_size_drop: None,
            blur_size_ref: None,
            fill_size_ref: None,
            fill_size_drop: None,
            fresnel_strength: None,
            fresnel_f0: None,
            fresnel_power: None,
            ggx_roughness: None,
            ggx_gain: None,
            ggx_light_dir: None,
        }
    }
    /// Packed constants with this shape's explicit knobs applied.
    /// Starts from the neutral base (zero/off/untouched — no bank, no
    /// variant): unset means nothing renders for that stage.
    ///
    /// Known limitation: wash *color* is bank-global (`D.rgb`); only
    /// opacity gets a per-shape scale (`fillScale`). Shapes with
    /// different explicit wash colors in one group render the first
    /// shape's color. Same for the tone curve and every other bank
    /// slot: a group shares one bank, taken from its first shape.
    pub fn effective(&self) -> GlassBank {
        let mut v = GlassBank::ZERO;
        v.lens_strength = self.lens_strength.unwrap_or(0.0);
        v.lens_strength *= self.strength;
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
        if let Some(x) = self.lens_edge_decay {
            v.lens_edge_decay = x.max(0.0);
        }
        // No fallbacks: absent stages resolve to neutral. See
        // `Glass::effective` for the contract.
        v.blur_sigma = self.frost_sigma.map(|s| s.max(0.0)).unwrap_or(0.0);
        match self.wash {
            None | Some(None) => v.fill_opacity = 0.0,
            Some(Some(c)) => {
                v.fill_color = Color::rgba(c.r, c.g, c.b, 1.0);
                v.fill_opacity = c.a.clamp(0.0, 1.0);
            }
        }
        v.saturation = self.saturation.map(|s| s.max(0.0)).unwrap_or(1.0);
        {
            use crate::foundation::glass::material::lerp_knots;
            use crate::foundation::glass::FIXED_REFERENCE_CURVE;
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
                v.tone_knots = lerp_knots(v.tone_knots, t);
                v.small_tone_knots = lerp_knots(v.small_tone_knots, t);
                v.tone_luma_mix *= t;
                v.tone_lift *= t;
            }
        }
        if let Some(x) = self.lens_decay { v.lens_decay = x.max(0.0); }
        if let Some(x) = self.lens_size_ref { v.lens_size_ref = x.max(0.0); }
        if let Some(x) = self.lens_edge { v.lens_edge = x.max(0.0); }
        if let Some(x) = self.normal_radius_scale { v.normal_radius_scale = x.max(0.0); }
        if let Some(x) = self.rim_width { v.rim_width = x.max(0.0); }
        if let Some(x) = self.rim_mix_width { v.rim_mix_width = x.max(0.0); }
        if let Some(x) = self.rim_mix_cut { v.rim_mix_cut = x.max(0.0); }
        if let Some(x) = self.rim_mix_floor { v.rim_mix_luma_floor = x.clamp(0.0, 1.0); }
        if let Some(x) = self.tint_strength { v.tint_strength = x.max(0.0); }
        if let Some(x) = self.ambient_mix { v.ambient_mix = x.clamp(0.0, 1.0); }
        if let Some(x) = self.post_blur_share {
            v.post_blur_share = x.clamp(0.0, crate::foundation::glass::K_MAX_POST_BLUR_SHARE);
        }
        if let Some(x) = self.frost_wide_sigma { v.frost_wide_sigma = x.max(0.0); }
        if let Some(x) = self.frost_wide_mix_edge { v.frost_wide_mix_edge = x; }
        if let Some(x) = self.frost_wide_mix_centre { v.frost_wide_mix_centre = x; }
        if let Some(x) = self.frost_wide_size_ref { v.frost_wide_size_ref = x.max(0.0); }
        if let Some(x) = self.frost_wide_size_drop { v.frost_wide_size_drop = x; }
        if let Some(x) = self.blur_size_ref { v.blur_size_ref = x.max(0.0); }
        if let Some(x) = self.fill_size_ref { v.fill_size_ref = x.max(0.0); }
        if let Some(x) = self.fill_size_drop { v.fill_size_drop = x; }
        if let Some(x) = self.glow_strength { v.glow_strength = x.max(0.0); }
        if let Some(x) = self.press_lift { v.press_lift = x.max(0.0); }
        if let Some(x) = self.dim { v.dim = x.clamp(0.0, 1.0); }
        if let Some(x) = self.post_jacobian_max { v.post_jacobian_max = x.max(0.0); }
        if let Some(x) = self.ambient_reach { v.ambient_reach = x.max(0.0); }
        if let Some(x) = self.fresnel_strength { v.fresnel_strength = x.max(0.0); }
        if let Some(x) = self.fresnel_f0 { v.fresnel_f0 = x.max(0.0); }
        if let Some(x) = self.fresnel_power { v.fresnel_power = x.max(0.0); }
        if let Some(x) = self.ggx_roughness { v.ggx_roughness = x.clamp(0.001, 1.0); }
        if let Some(x) = self.ggx_gain { v.ggx_gain = x.max(0.0); }
        if let Some(x) = self.ggx_light_dir { v.ggx_light = x; }
        v
    }
}

/// Everything the shader needs for one frame. Mirrors
/// `GlassFrameUniforms`; lifetimes keep it allocation-free for the engine
/// that builds it each frame.
#[derive(Copy, Clone, Debug)]
pub struct GlassFrameUniforms<'a> {
    /// Shapes to draw (already union-merged, already device space).
    pub shapes: &'a [GlassShapeUniform],
    /// Physical px per logical px.
    pub device_pixel_ratio: f32,
    /// Light direction, radians, y-down.
    pub light_angle: f32,
    /// Smooth-min radius in device px (`mergeFactor × group spacing ×
    /// dpr`).
    pub smoothing: f32,
    /// Rendering constants (motion constants ride along inside).
    pub constants: &'a GlassConstants,
    /// Increase Contrast is on.
    pub high_contrast: bool,
    /// Non-null when Reduce Transparency forces an opaque surface.
    pub opaque_color: Option<Color>,
    /// Touch point in device px, texture space.
    pub touch: Option<[f32; 2]>,
    /// Touch glow strength 0..1 (the pressed member's press light).
    pub glow: f32,
    /// The blur already applied to the input (logical px): each shape's
    /// [`GlassShapeUniform::blur_sigma`] beyond it is added in the shader.
    pub blur_sigma: f32,
    /// sigmaX / [`Self::blur_sigma`].
    pub blur_aspect: f32,
    /// Independent spectral refraction for the whole frame. The lens stays
    /// monochromatic; this optional sampler widens its neutral refraction
    /// offset once per wavelength.
    pub spectral: Spectral,
    /// Emissive bottom glow shared by the whole frame (the voice-glow
    /// wash). Off by default; unlike spectral it needs no backdrop.
    pub voice_glow: Glow,
    /// Outer refraction halo shared by the whole frame (theatre-mode
    /// blur of the card). Independent of the glow. Off by default.
    pub refraction: Refraction,
    /// Author-ordered glass-stage ids for the explicit fold (`fs_main`
    /// dispatch); empty = the canonical chain (count 0 in the pack).
    pub stages: &'a [u32],
}

/// Composed frost sigma (logical px) for a shape of variant constants `v`
/// whose half shorter side is `half_min`: `v.blur_sigma` scaled down below
/// `blur_size_ref` and by `√(1 − postBlurShare)`. A group runs one blur
/// with the largest of its members' values.
pub fn composed_blur_sigma(v: GlassBank, half_min: f32) -> f32 {
    let ref_ = v.blur_size_ref;
    let k = if ref_ > 0.0 {
        (half_min / ref_).min(1.0)
    } else {
        1.0
    };
    let share = v.post_blur_share.clamp(0.0, K_MAX_POST_BLUR_SHARE);
    v.blur_sigma * k * (1.0 - share).sqrt()
}

/// sigmaX / sigma of the composed blur for a shape of `size` (logical px):
/// `(height / width) ^ blurAspectPower`, so a wide shape blurs less along
/// its long axis and more across it (`sigmaY = sigma / aspect`). 1 when the
/// power is 0 or the shape is square.
pub fn composed_blur_aspect(v: GlassBank, width: f32, height: f32) -> f32 {
    if v.blur_aspect_power == 0.0 || width <= 0.0 || height <= 0.0 {
        return 1.0;
    }
    (height / width).powf(v.blur_aspect_power)
}

/// The frost `s` wants beyond the input's blur (`u`'s), per axis in
/// logical px, and the taps the shader spends on it; `None` when there is
/// none worth drawing (under 0.1 px). Gaussians compose by their variances.
///
/// Returns `(sigma_x, sigma_y, taps)`.
pub fn extra_blur_of(
    s: &GlassShapeUniform,
    u: &GlassFrameUniforms<'_>,
) -> Option<(f32, f32, f32)> {
    if s.blur_sigma <= 0.0 {
        return None;
    }
    fn extra(want: f32, have: f32) -> f32 {
        (want * want - have * have).max(0.0).sqrt()
    }
    let ia = if u.blur_sigma > 0.0 { u.blur_aspect } else { 1.0 };
    let have_x = u.blur_sigma * ia;
    let have_y = u.blur_sigma / ia;
    let x = extra(s.blur_sigma * s.blur_aspect, have_x);
    let y = extra(s.blur_sigma / s.blur_aspect, have_y);
    if x.max(y) < 0.1 {
        return None;
    }
    // Sparse taps alias less on a smoother input; measured on text and
    // photo backdrops (error under about 1.5 of 255 mean up to 4x, 2.6 at
    // 7x).
    let ratio = x.max(y) / have_x.min(have_y).max(0.25);
    let taps = if ratio <= 1.5 {
        12.0
    } else if ratio <= 2.5 {
        16.0
    } else if ratio <= 4.0 {
        24.0
    } else {
        32.0
    };
    Some((x, y, taps))
}

/// Whether the packer draws this shape (mirrors `_drawable`): not
/// hidden, finite and positive rect, finite radius.
pub fn drawable(s: &GlassShapeUniform) -> bool {
    !s.hidden
        && s.rect.x.is_finite()
        && s.rect.y.is_finite()
        && s.rect.width.is_finite()
        && s.rect.height.is_finite()
        && s.rect.width > 0.0
        && s.rect.height > 0.0
        && s.radius.is_finite()
}

/// Merges shapes that share a non-null `union_id` into their bounding rect,
/// keeping the smallest radius and the first shape's hidden flag, tint and
/// corner exponent. Mirrors `mergeUnions`.
pub fn merge_unions(shapes: &[GlassShapeUniform]) -> Vec<GlassShapeUniform> {
    let mut out: Vec<GlassShapeUniform> = Vec::with_capacity(shapes.len());
    let mut by_id: Vec<(u64, usize)> = Vec::new();
    for s in shapes {
        let at = s.union_id.and_then(|id| {
            by_id
                .iter()
                .find(|(k, _)| *k == id)
                .map(|&(_, idx)| (id, idx))
        });
        match at {
            None => {
                if let Some(id) = s.union_id {
                    by_id.push((id, out.len()));
                }
                out.push(*s);
            }
            Some((id, idx)) => {
                let prev = out[idx];
                let rect = union_rect(prev.rect, s.rect);
                let radius = prev
                    .radius
                    .min(s.radius)
                    .min(rect.width.min(rect.height) / 2.0);
                let merged = GlassShapeUniform {
                    rect,
                    radius,
                    hidden: prev.hidden,
                    tint: prev.tint,
                    union_id: Some(id),
                    corner_exponent: prev.corner_exponent,
                    group: prev.group,
                    strength: prev.strength,
                    lens_strength: prev.lens_strength.or(s.lens_strength),
                    smoothing: prev.smoothing,
                    opacity: prev.opacity,
                    clip: prev.clip,
                    band: prev.band.or(s.band),
                    rim: prev.rim.or(s.rim),
                    frost_sigma: prev.frost_sigma.or(s.frost_sigma),
                    rim_intensity: prev.rim_intensity.or(s.rim_intensity),
                    rim_mix: prev.rim_mix.or(s.rim_mix),
                    blur_aspect_power: prev.blur_aspect_power.or(s.blur_aspect_power),
                    lens_edge_decay: prev.lens_edge_decay.or(s.lens_edge_decay),
                    wash: prev.wash.or(s.wash),
                    saturation: prev.saturation.or(s.saturation),
                    tone: prev.tone.or(s.tone),
                    tone_curve: prev.tone_curve.or(s.tone_curve),
                    lens_decay: prev.lens_decay.or(s.lens_decay),
                    lens_size_ref: prev.lens_size_ref.or(s.lens_size_ref),
                    lens_edge: prev.lens_edge.or(s.lens_edge),
                    normal_radius_scale: prev.normal_radius_scale.or(s.normal_radius_scale),
                    rim_width: prev.rim_width.or(s.rim_width),
                    rim_mix_width: prev.rim_mix_width.or(s.rim_mix_width),
                    rim_mix_cut: prev.rim_mix_cut.or(s.rim_mix_cut),
                    rim_mix_floor: prev.rim_mix_floor.or(s.rim_mix_floor),
                    tint_strength: prev.tint_strength.or(s.tint_strength),
                    ambient_mix: prev.ambient_mix.or(s.ambient_mix),
                    post_blur_share: prev.post_blur_share.or(s.post_blur_share),
                    frost_wide_sigma: prev.frost_wide_sigma.or(s.frost_wide_sigma),
                    frost_wide_mix_edge: prev.frost_wide_mix_edge.or(s.frost_wide_mix_edge),
                    frost_wide_mix_centre: prev.frost_wide_mix_centre.or(s.frost_wide_mix_centre),
                    frost_wide_size_ref: prev.frost_wide_size_ref.or(s.frost_wide_size_ref),                    frost_wide_size_drop: prev.frost_wide_size_drop.or(s.frost_wide_size_drop),
                    blur_size_ref: prev.blur_size_ref.or(s.blur_size_ref),
                    fill_size_ref: prev.fill_size_ref.or(s.fill_size_ref),
                    fill_size_drop: prev.fill_size_drop.or(s.fill_size_drop),
                    fresnel_strength: prev.fresnel_strength.or(s.fresnel_strength),
                    fresnel_f0: prev.fresnel_f0.or(s.fresnel_f0),
                    fresnel_power: prev.fresnel_power.or(s.fresnel_power),
                    ggx_roughness: prev.ggx_roughness.or(s.ggx_roughness),
                    ggx_gain: prev.ggx_gain.or(s.ggx_gain),
                    ggx_light_dir: prev.ggx_light_dir.or(s.ggx_light_dir),
                    glow_strength: prev.glow_strength.or(s.glow_strength),
                    press_lift: prev.press_lift.or(s.press_lift),
                    dim: prev.dim.or(s.dim),
                    post_jacobian_max: prev.post_jacobian_max.or(s.post_jacobian_max),
                    ambient_reach: prev.ambient_reach.or(s.ambient_reach),
                    light: prev.light.max(s.light),
                    blur_sigma: if prev.blur_sigma >= s.blur_sigma {
                        prev.blur_sigma
                    } else {
                        s.blur_sigma
                    },
                    blur_aspect: if prev.blur_sigma >= s.blur_sigma {
                        prev.blur_aspect
                    } else {
                        s.blur_aspect
                    },
                };
                out[idx] = merged;
            }
        }
    }
    out
}

fn union_rect(a: ResolvedRect, b: ResolvedRect) -> ResolvedRect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    ResolvedRect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Packs `u` into the float layout documented in `shaders/liquid_glass.frag`
/// (N = [`K_MAX_PASS_SHAPES`] = 24). Returns exactly
/// [`K_GLASS_UNIFORM_FLOATS`] `f32`s, which ride after the engine-owned
/// `uSize` vec2.
///
/// Layout (floats, after `uSize`):
///
/// | floats  | uniform   | contents |
/// |---------|-----------|----------|
/// | 0–3     | uGlobal   | count, dpr, lightAngle, opaque |
/// | 4–7     | uGlobal2  | smoothing px, cornerExponent, highContrast, cornerZone |
/// | 8–11    | uOpaque   | rgb |
/// | 12–15   | uTouch    | x, y, glow, glowRadius px |
/// | 16–111  | uRects[N] | x, y, w, h px |
/// | 112–207 | uInfo[N]  | radius px, reserved (always 0), cornerExponent, fill scale |
/// | 208–303 | uTints[N] | rgb, strength |
/// | 304–447 | uVar[0..35] | 36 vec4s of packed constants (see below) |
/// | 448–543 | uExtra[N] | group index, smoothing px, opacity, light |
/// | 544–639 | uClip[N]  | clip x0, y0, x1, y1 px |
/// | 640–735 | uBlur[N]  | extra sigma x px, extra sigma y px, taps, 0 |
/// | 736–791 | uGlow/uOuter | 6 lobes (centre, σ, amp, 0) + 6 colours + geom + outer rule |
///
/// The glow tail (floats up to `K_GLASS_WIRE_FLOATS`) rides the
/// compositor wire too since wire v5 (the `riode-glass` plugin's GLSL
/// optics is a line-for-line port of the wgpu shader, glow and blend
/// library included — see `GLASS_WIRE_PACKED_FLOATS`); the stage list
/// appended after it is wgpu-only.
pub fn pack_glass_uniforms(u: &GlassFrameUniforms<'_>) -> Vec<f32> {
    let dpr = u.device_pixel_ratio;
    let k = u.constants;
    let shapes: Vec<&GlassShapeUniform> =
        u.shapes.iter().filter(|s| drawable(s)).take(K_MAX_PASS_SHAPES).collect();

    let mut f = vec![0.0f32; K_GLASS_UNIFORM_FLOATS];

    f[0] = shapes.len() as f32;
    f[1] = dpr;
    f[2] = u.light_angle;
    f[3] = if u.opaque_color.is_some() { 1.0 } else { 0.0 };

    f[4] = u.smoothing;
    f[5] = k.corner_exponent;
    f[6] = if u.high_contrast { 1.0 } else { 0.0 };
    f[7] = k.corner_zone;

    if let Some(o) = u.opaque_color {
        f[8] = o.r;
        f[9] = o.g;
        f[10] = o.b;
    }

    if let Some(t) = u.touch {
        f[12] = t[0];
        f[13] = t[1];
        f[14] = u.glow;
    }
    f[15] = u.constants.motion.glow_radius * dpr;

    for (i, s) in shapes.iter().enumerate() {
        let v = s.effective();
        let dst = K_RECTS_AT + i * 4;
        f[dst] = s.rect.x;
        f[dst + 1] = s.rect.y;
        f[dst + 2] = s.rect.width;
        f[dst + 3] = s.rect.height;

        // Fill scale for this shape's size (fillSizeRef / fillSizeDrop).
        let half_min = s.rect.width.min(s.rect.height) / 2.0 / dpr;
        let fill_scale = if v.fill_size_ref > 0.0 {
            1.0 - v.fill_size_drop * (1.0 - (half_min / v.fill_size_ref).min(1.0))
        } else {
            1.0
        };
        let dst = K_INFO_AT + i * 4;
        f[dst] = s.radius;
        // Single bank: no variant flag. The slot ships 0 (layout kept
        // stable for the compositor wire).
        f[dst + 1] = 0.0;
        // 0: the global exponent and continuous-corner zone (uGlobal2.yw).
        f[dst + 2] = s.corner_exponent.unwrap_or(0.0);
        f[dst + 3] = fill_scale;

        let dst = K_EXTRA_AT + i * 4;
        f[dst] = s.group as f32;
        f[dst + 1] = s.smoothing.unwrap_or(u.smoothing);
        f[dst + 2] = s.opacity;
        f[dst + 3] = s.light;

        let dst = K_CLIP_AT + i * 4;
        match s.clip {
            None => {
                f[dst] = -1.0e9;
                f[dst + 1] = -1.0e9;
                f[dst + 2] = 1.0e9;
                f[dst + 3] = 1.0e9;
            }
            Some(c) => {
                f[dst] = c.x;
                f[dst + 1] = c.y;
                f[dst + 2] = c.x + c.width;
                f[dst + 3] = c.y + c.height;
            }
        }

        if let Some(extra) = extra_blur_of(s, u) {
            let dst = K_BLUR_AT + i * 4;
            f[dst] = extra.0 * dpr;
            f[dst + 1] = extra.1 * dpr;
            f[dst + 2] = extra.2;
            f[dst + 3] = 0.0;
        }

        if let Some(tint) = s.tint {
            let dst = K_TINTS_AT + i * 4;
            f[dst] = tint.r;
            f[dst + 1] = tint.g;
            f[dst + 2] = tint.b;
            f[dst + 3] = v.tint_strength * tint.a;
        }
    }

    // Single bank, shipped twice: the layout (and the compositor wire)
    // keeps both 48-float variant slots, but both carry the same set —
    // the first drawn shape's effective constants. No variant flags, no
    // clearMix: every value on every shape is explicit.
    let mut bank = shapes
        .first()
        .map(|s| s.effective())
        .unwrap_or_else(|| {
            crate::foundation::Glass::identity().effective(&crate::foundation::Backdrop::NONE)
        });
    // Layer-shared spectral sampling: it widens the shared lens bank's
    // refraction offset without touching any glass-material value.
    u.spectral.apply_to(&mut bank);
    u.voice_glow.apply_to(&mut bank);
    u.refraction.apply_to(&mut bank);
    let variant_sets = [bank, bank];
    let mut base = K_VAR_AT;
    for v in variant_sets {
        // The frost blur is the composed ImageFilter, not a uniform.
        pack_a_to_l(&mut f, base, v, dpr);
        base += 48;
    }
    // Small-shape tone curve (Task g13): M–O per bank slot.
    for v in variant_sets {
        let d = base;
        f[d..d + 4].copy_from_slice(&v.small_tone_knots[0..4]);
        f[d + 4..d + 8].copy_from_slice(&v.small_tone_knots[4..8]);
        f[d + 8] = v.small_tone_knots[8];
        f[d + 9] = v.small_size_lo * dpr;
        f[d + 10] = v.small_size_hi * dpr;
        f[d + 11] = v.post_jacobian_max;
        base += 12;
    }
    // Ambient colour (item 8): P per bank slot.
    for v in variant_sets {
        let d = base;
        f[d] = v.ambient_mix;
        f[d + 1] = v.ambient_reach * dpr;
        f[d + 2] = v.tone_luma_mix;
        f[d + 3] = 0.0;
        base += 4;
    }
    // Q per bank slot: spectral refraction (ior, spread, reach) in xyz;
    // w carries the voice-glow overall gain (0 = the shader takes the
    // no-glow path). The spectral comment history lives in the shader's
    // bank table; the slots were repurposed here (wire layout unchanged).
    for v in variant_sets {
        let d = base;
        f[d] = v.spectral_ior;
        f[d + 1] = v.spectral_spread;
        f[d + 2] = v.spectral_reach;
        f[d + 3] = v.glow_voice;
        base += 4;
    }
    // R per bank slot: x carries the glow inside gain, y the outer
    // refraction (halo) gain, z the press lift; w is retired and kept 0.
    for v in variant_sets {
        let d = base;
        f[d] = v.glow_inside;
        f[d + 1] = v.refraction_strength;
        f[d + 2] = v.press_lift;
        f[d + 3] = 0.0;
        base += 4;
    }

    debug_assert_eq!(base, K_EXTRA_AT, "uVar must end where uExtra begins");

    // Glow tail (wgpu-only, appended — all v4 offsets above are stable).
    // Resolved against the first drawn shape's device-space rect: the
    // shader's attribute-blended rect is that rect for single shapes and
    // close for groups. Lobes pack when the glow is active; the outer
    // rule packs when refraction is active (inactive lanes keep the zeros
    // the `vec!` initialised). The shader gates each path on its own
    // gain regardless.
    if u.voice_glow.is_active() || u.refraction.is_active() {
        if let Some(first) = shapes.first() {
            let r = first.rect;
            let g = u.voice_glow.resolve((r.x, r.y, r.width, r.height));
            let mut o = u.refraction.resolve_rule();
            o[0] *= dpr;
            let mut d = K_GLOW_AT;
            for i in 0..crate::foundation::glass::GLOW_LOBES {
                f[d] = g.lobes[i].0;
                f[d + 1] = g.lobes[i].1;
                f[d + 2] = g.lobes[i].2;
                f[d + 3] = 0.0;
                d += 4;
            }
            for i in 0..crate::foundation::glass::GLOW_LOBES {
                f[d] = g.colors[i][0];
                f[d + 1] = g.colors[i][1];
                f[d + 2] = g.colors[i][2];
                f[d + 3] = 0.0;
                d += 4;
            }
            f[d] = g.line;
            f[d + 1] = g.line_sigma;
            f[d + 2] = g.axis;
            // Blend mode id in the geom vec4's free w slot (`Normal` = 0:
            // byte-identical to the pre-blend wire). Appended — every slot
            // above (and the outer rule at +52/53) keeps its offset.
            f[d + 3] = g.blend.as_uniform_id();
            d += 4;
            f[d] = o[0];
            f[d + 1] = o[1];
            f[d + 2] = o[2];
            f[d + 3] = o[3];
            d += 4;
            debug_assert_eq!(d, K_GLASS_WIRE_FLOATS, "glow tail must fill the wire stream");
        }
    }
    // Stage list (wgpu-only, appended after the glow tail — the wire
    // stops at `K_GLASS_WIRE_FLOATS`): count, then the author-ordered
    // glass-stage ids in stack order (`fs_main` dispatch). Count 0 is
    // the canonical chain, byte-identical to a stack with no units.
    debug_assert!(u.stages.len() <= 15, "at most 15 glass-stage ids");
    let d = K_GLASS_WIRE_FLOATS;
    f[d] = u.stages.len() as f32;
    for (i, id) in u.stages.iter().enumerate() {
        f[d + 1 + i] = *id as f32;
    }
    // GGX knobs (wgpu-only, appended after the stage list): x roughness,
    // y gain, zw + next vec4's x = the light direction xyz. Single-bank
    // discipline like the constants: the first drawn shape's effective
    // values; the gain-0 default keeps the stage a no-op.
    let d = K_GLASS_WIRE_FLOATS + K_STAGES_FLOATS;
    if let Some(first) = shapes.first() {
        let v = first.effective();
        f[d] = v.ggx_roughness;
        f[d + 1] = v.ggx_gain;
        f[d + 2] = v.ggx_light[0];
        f[d + 3] = v.ggx_light[1];
        f[d + 4] = v.ggx_light[2];
    }
    f
}

/// Light-spill replay params read back from packed frame vec4s (the
/// renderer's half of `LightSpill`): `(gain, blur σ in device px,
/// H+V iteration count, downward length in device px)`.
/// `None` = no spill this frame.
///
/// Layout: first bank's `R4.y` gain; tail outer rule length device px +
/// tap count 1/3/5. Taps map to iterations 1→1, 3→2, 5→3.
/// σ ≈ length/6 clamped to [2, 8] device px: tight enough to keep the hue
/// and the edge attached — the vertex-alpha gradient carries the falloff.
/// Gain is dimmed (×0.6 boost in `resolve_rule`) so the spill stays less
/// bright than the card.
pub fn light_spill_replay(data: &[[f32; 4]]) -> Option<(f32, f32, usize, f32)> {
    let f = |i: usize| data[i / 4][i % 4];
    let strength = f(K_VAR_AT + 136 + 1);
    if strength <= 0.0 {
        return None;
    }
    let length = f(K_GLOW_AT + 52).max(0.0);
    let taps = f(K_GLOW_AT + 53) as usize;
    if length <= 0.5 {
        return None;
    }
    let iters = (taps + 1) / 2;
    if iters == 0 {
        return None;
    }
    // Tight kernel: the vertex-alpha gradient (not the blur) carries the
    // downward falloff, so the blur only softens — never washes out — the
    // card hue. σ ≈ length/6 keeps the edge attached and the colour true.
    let sigma = (length / 6.0).clamp(2.0, 8.0);
    Some((strength, sigma, iters, length))
}

/// Deprecated: theatre-halo entry point. Reinterpreted as a downward
/// light spill — same wire, new composite (below-edge strip only).
/// Layout (must match the packer above and `shaders/glass/core.wgsl`):
/// banks are
/// section-major (A–L×2, M–O×2, P×2, Q×2, R×2), so the first bank's `R4.y`
/// is `K_VAR_AT + 136 + 1`; the tail's outer rule (`K_GLOW_AT + 52/53`:
/// reach in edge-σ, tap count 1/3/5) and the lobe geom (`K_GLOW_AT + 49`:
/// cross-edge σ in device px) size the blur. Taps map to iterations
/// 1→1, 3→2, 5→3 (effective σ ≈ σ√n).
pub fn refraction_replay(data: &[[f32; 4]]) -> Option<(f32, f32, usize)> {
    light_spill_replay(data).map(|(s, sigma, iters, _)| (s, sigma, iters))
}

/// The A–L block (48 floats) the single bank contributes to `uVar`.
fn pack_a_to_l(f: &mut [f32], base: usize, v: GlassBank, dpr: f32) {
    let mut k = base;
    f[k] = v.lens_decay * dpr;
    f[k + 1] = v.lens_band * dpr;
    f[k + 2] = v.lens_strength;
    f[k + 3] = 0.0;
    k += 4;

    f[k] = v.rim_width * dpr;
    f[k + 1] = v.rim_intensity;
    f[k + 2] = v.fill_opacity;
    f[k + 3] = v.dim;
    k += 4;

    f[k] = 0.0;
    f[k + 1] = 0.0;
    f[k + 2] = v.tint_strength;
    f[k + 3] = v.lens_size_ref * dpr;
    k += 4;

    f[k] = v.fill_color.r;
    f[k + 1] = v.fill_color.g;
    f[k + 2] = v.fill_color.b;
    f[k + 3] = v.saturation;
    k += 4;

    f[k] = v.frost_wide_sigma * dpr;
    f[k + 1] = v.frost_wide_mix_edge;
    f[k + 2] = v.frost_wide_mix_centre;
    f[k + 3] = v.frost_wide_size_ref * dpr;
    k += 4;

    f[k] = v.frost_wide_size_drop;
    f[k + 1] = v.glow_strength;
    f[k + 2] = v.post_blur_share.clamp(0.0, K_MAX_POST_BLUR_SHARE);
    f[k + 3] = v.normal_radius_scale;
    k += 4;

    f[k..k + 4].copy_from_slice(&v.tone_knots[0..4]);
    k += 4;
    f[k..k + 4].copy_from_slice(&v.tone_knots[4..8]);
    k += 4;

    f[k] = v.tone_knots[8];
    f[k + 1] = v.lens_edge * dpr;
    f[k + 2] = v.lens_edge_decay * dpr;
    f[k + 3] = v.tone_lift;
    k += 4;

    f[k] = v.rim_mix;
    f[k + 1] = v.rim_mix_width * dpr;
    f[k + 2] = v.rim_mix_cut * dpr;
    f[k + 3] = v.rim_mix_luma_floor;
    k += 4;

    // Post-lens sigma, so a mixed group does not inherit the strongest
    // member's frost (the shader applies the size scale).
    f[k] = v.tone_lift_knee;
    f[k + 1] = v.tone_lift_size_ref * dpr;
    f[k + 2] = v.blur_sigma
        * v.post_blur_share.clamp(0.0, K_MAX_POST_BLUR_SHARE).sqrt()
        * dpr;
    f[k + 3] = v.blur_size_ref * dpr;
    k += 4;

    f[k] = v.rim_back;
    // L's retired slots: the Fresnel stage's knobs (`fresnel.wgsl`:
    // w the gain, y F0, z the exponent — the shader defaults F0 0.2,
    // power 5 when zeroed).
    f[k + 1] = v.fresnel_f0;
    f[k + 2] = v.fresnel_power;
    f[k + 3] = v.fresnel_strength;
}

/// Packed-frame wire version for the compositor (`set_glass` payload).
/// v1 was the P1 30-float material (retired with the P1 port); v2/v3
/// were engine-internal material iterations no helper ever consumed.
/// v4 was the 736-float prefix (no glow tail). v5 is the stream through
/// the glow tail (frozen at `GLASS_WIRE_PACKED_FLOATS`) — the only
/// layout the current helper decodes.
pub const GLASS_WIRE_PACKED_VERSION: u32 = 5;
/// Floats in one packed wire frame: the stream through the glow tail
/// (frozen — see `K_GLASS_WIRE_FLOATS`); the stage list after it is
/// wgpu-only until the plugin's port grows a dispatch.
pub const GLASS_WIRE_PACKED_FLOATS: usize = K_GLASS_WIRE_FLOATS;
/// u32 words in one `set_glass` payload: version + frame σx/σy + integer
/// sample box (x0, y0, w, h, device px) + the packed floats. All shape
/// rects, clip rects and the touch point in the packed floats are
/// box-relative (sample-box origin subtracted); the plugin uses the header
/// box verbatim for blit/scissor/uSize — no recomputation, no rounding
/// drift (x0/y0 are whole numbers so the offset is exact).
pub const GLASS_WIRE_PACKED_WORDS: usize = 7 + GLASS_WIRE_PACKED_FLOATS;

/// Packs one island for the compositor wire (v5): the 7-word frame
/// header followed by the full packed uniform stream (glow tail
/// included), as little-endian u32 words.
///
/// `rect` is the island union in surface-local logical px; `layer`
/// members, clip and press_touch must already be in the same
/// surface-local space (see `scene::islands::Collector`, which maps all
/// three through the layer transform). `scale` is the output device
/// scale (surface logical → device px).
///
/// Box-relative layout: shapes are built in device px, union-merged, then
/// every merged shape rect, clip rect and the press touch are offset by
/// -(x0,y0), the integer sample-box origin below. The compositor shader
/// samples with box-relative px (`gl_FragCoord - boxMin`) against
/// box-sized uSize, so surface-space coords would evaluate every SDF
/// outside and render the lens invisible.
///
/// Header: [version][frame σx dev][frame σy dev][boxX][boxY][boxW][boxH],
/// where the box is the exact integer sample box the plugin uses verbatim
/// for blit/scissor/uSize.
///
/// Single islands pack as a group of one (smoothing 0, opacity 1, no
/// clip, light 0 — the wgpu single path's hardcodes), so the compositor
/// needs no special case. Groups merge unions, take the smallest drawn
/// σ as the frame blur (members wanting more get extra frost taps),
/// and carry the first member's set — exactly the wgpu group
/// emission. Press/morph ride along: collection already baked the
/// deformed rects, light and touch into the members.
pub fn pack_glass_wire(rect: ResolvedRect, layer: &GlassLayer, scale: f32) -> Vec<u32> {
    use crate::foundation::glass::{GLASS_STANDARD, DEFAULT_LIGHT_ANGLE};
    let k = GLASS_STANDARD;
    let (members, spectral, glow, refraction, smoothing, opacity, clip, press_touch) = match layer {
        GlassLayer::Single(g, b, s, gl, r) => {
            let member = GlassMember {
                rect,
                glass: *g,
                backdrop: *b,
                light: 0.0,
                radius_scale: 1.0,
            };
            (vec![member], *s, *gl, *r, 0.0, 1.0, None, None)
        }
        GlassLayer::Group(group) => (
            group.members.clone(),
            group.spectral,
            group.glow,
            group.refraction,
            group.smoothing,
            group.opacity,
            group.clip,
            group.press_touch,
        ),
    };
    let clip_dev = clip.map(|c| {
        ResolvedRect::new(
            c.x * scale,
            c.y * scale,
            (c.width * scale).max(0.0),
            (c.height * scale).max(0.0),
        )
    });
    let mut shapes = Vec::with_capacity(members.len().min(K_MAX_PASS_SHAPES));
    for m in &members {
        let v = m.glass.effective(&m.backdrop);
        let rw = (m.rect.width * scale).max(0.0);
        let rh = (m.rect.height * scale).max(0.0);
        let shape_rect = ResolvedRect::new(m.rect.x * scale, m.rect.y * scale, rw, rh);
        let radius = (m.radius() * m.radius_scale * scale).min(rw.min(rh) / 2.0);
        let half_min = rw.min(rh) / 2.0 / scale;
        let sigma = composed_blur_sigma(v, half_min);
        let aspect = composed_blur_aspect(v, rw / scale, rh / scale);
        let mut shape = GlassShapeUniform::new(shape_rect, radius, m.glass.hidden);
        shape.tint = m.backdrop.tint;
        shape.corner_exponent = m.glass.corner_exponent;
        shape.union_id = m.glass.union_id;
        shape.group = 0;
        shape.smoothing = Some(smoothing * scale);
        shape.opacity = opacity;
        shape.clip = clip_dev;
        shape.light = m.light;
        shape.blur_sigma = sigma;
        shape.blur_aspect = aspect;
        shape.hidden = m.glass.hidden;
        shape.band = m.glass.band;
        shape.rim = m.glass.rim;
        shape.frost_sigma = m.backdrop.blur;
        shape.wash = m.backdrop.wash;
        shape.saturation = m.backdrop.saturation;
        shape.rim_intensity = m.glass.rim_intensity;
        shape.rim_mix = m.glass.rim_mix;
        shape.blur_aspect_power = m.glass.blur_aspect_power;
        shape.lens_edge_decay = m.glass.lens_edge_decay;
        shape.tone = m.backdrop.tone;
        shape.tone_curve = m.backdrop.tone_curve;
        shape.lens_decay = m.glass.lens_decay;
        shape.lens_size_ref = m.glass.lens_size_ref;
        shape.lens_edge = m.glass.lens_edge;
        shape.normal_radius_scale = m.glass.normal_radius_scale;
        shape.rim_width = m.glass.rim_width;
        shape.rim_mix_width = m.glass.rim_mix_width;
        shape.rim_mix_cut = m.glass.rim_mix_cut;
        shape.rim_mix_floor = m.glass.rim_mix_floor;
        shape.tint_strength = m.backdrop.tint_strength;
        shape.ambient_mix = m.backdrop.ambient_mix;
        shape.post_blur_share = m.glass.post_blur_share;
        shape.frost_wide_sigma = m.glass.frost_wide_sigma;
        shape.frost_wide_mix_edge = m.glass.frost_wide_mix_edge;
        shape.frost_wide_mix_centre = m.glass.frost_wide_mix_centre;
        shape.frost_wide_size_ref = m.glass.frost_wide_size_ref;
        shape.frost_wide_size_drop = m.glass.frost_wide_size_drop;
        shape.blur_size_ref = m.backdrop.blur_size_ref;
        shape.fill_size_ref = m.backdrop.fill_size_ref;
        shape.fill_size_drop = m.backdrop.fill_size_drop;
        shape.glow_strength = m.glass.glow_strength;
        shape.press_lift = m.glass.press_lift;
        shape.dim = m.backdrop.dim;
        shape.post_jacobian_max = m.glass.post_jacobian_max;
        shape.ambient_reach = m.backdrop.ambient_reach;
        shape.strength = m.glass.strength;
        shape.lens_strength = m.glass.lens_strength;
        shapes.push(shape);
    }
    let mut shapes = merge_unions(&shapes);
    let (frame_sigma, frame_aspect) = shapes
        .iter()
        .map(|s| (s.blur_sigma, s.blur_aspect))
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0.0, 1.0));
    // Integer sample box from the island union (surface-logical px),
    // padded by damage + 3x the frame core-blur kernel (device px).
    let pad_total =
        layer.damage_pad() * scale
            + 3.0 * (frame_sigma * frame_aspect).max(frame_sigma / frame_aspect) * scale;
    let x0 = (rect.x * scale - pad_total).floor();
    let y0 = (rect.y * scale - pad_total).floor();
    let x1 = ((rect.x + rect.width) * scale + pad_total).ceil();
    let y1 = ((rect.y + rect.height) * scale + pad_total).ceil();
    let box_w = ((x1 - x0).max(1.0)) as f32;
    let box_h = ((y1 - y0).max(1.0)) as f32;
    // Box-relative: x0/y0 are whole numbers so the offset is exact.
    for s in &mut shapes {
        s.rect.x -= x0;
        s.rect.y -= y0;
        if let Some(c) = s.clip.as_mut() {
            c.x -= x0;
            c.y -= y0;
        }
    }
    let (touch, touch_glow) = press_touch
        .map(|(t, g)| ([t.x * scale - x0, t.y * scale - y0], g))
        .unwrap_or(([0.0, 0.0], 0.0));
    let frame = GlassFrameUniforms {
        shapes: &shapes,
        device_pixel_ratio: scale,
        light_angle: DEFAULT_LIGHT_ANGLE,
        smoothing: 0.0,
        constants: &k,
        high_contrast: false,
        opaque_color: None,
        touch: if press_touch.is_some() {
            Some([touch[0], touch[1]])
        } else {
            None
        },
        glow: touch_glow,
        blur_sigma: frame_sigma,
        blur_aspect: frame_aspect,
        spectral,
        voice_glow: glow,
        refraction,
        // The wire carries no dispatch: islands run the canonical chain
        // (count 0) until the plugin's port grows a stage list.
        stages: &[],
    };
    let packed = pack_glass_uniforms(&frame);
    debug_assert_eq!(packed.len(), K_GLASS_UNIFORM_FLOATS);
    let mut words = Vec::with_capacity(GLASS_WIRE_PACKED_WORDS);
    words.push(GLASS_WIRE_PACKED_VERSION);
    words.push((frame_sigma * frame_aspect * scale).to_bits());
    words.push((frame_sigma / frame_aspect * scale).to_bits());
    words.push(x0.to_bits());
    words.push(y0.to_bits());
    words.push(box_w.to_bits());
    words.push(box_h.to_bits());
    // Full stream through the glow tail: it rides the wire since v5 (the
    // plugin's GLSL optics is a line-for-line port of the wgpu twin).
    // The stage list after the tail is wgpu-only — frozen wire length.
    words.extend(packed[..GLASS_WIRE_PACKED_FLOATS].iter().map(|f| f.to_bits()));
    debug_assert_eq!(words.len(), GLASS_WIRE_PACKED_WORDS);
    words
}

#[cfg(test)]
mod override_tests {
    use super::*;
    use crate::foundation::glass::IDENTITY_TONE_KNOTS;
    use crate::foundation::{Backdrop, Glass};

    fn shape_with(g: Glass, b: Backdrop) -> GlassShapeUniform {
        let mut s = GlassShapeUniform::new(ResolvedRect::new(0.0, 0.0, 200.0, 200.0), 28.0, g.hidden);
        s.strength = g.strength;
        s.lens_strength = g.lens_strength;
        s.band = g.band;
        s.rim = g.rim;
        s.tint = b.tint;
        s.frost_sigma = b.blur;
        s.wash = b.wash;
        s.saturation = b.saturation;
        s.tone = b.tone;
        s.tone_curve = b.tone_curve;
        s
    }

    #[test]
    fn refraction_replay_roundtrip() {
        use crate::foundation::{Glow, Refraction, Spectral, GLASS_STANDARD};
        let shape = GlassShapeUniform::new(
            ResolvedRect::new(0.0, 0.0, 200.0, 150.0),
            24.0,
            false,
        );
        let frame = GlassFrameUniforms {
            shapes: std::slice::from_ref(&shape),
            device_pixel_ratio: 1.0,
            light_angle: 0.0,
            smoothing: 0.0,
            constants: &GLASS_STANDARD,
            high_contrast: false,
            opaque_color: None,
            touch: None,
            glow: 0.0,
            blur_sigma: 0.0,
            blur_aspect: 1.0,
            spectral: Spectral::NONE,
            voice_glow: Glow::new().strength(0.6).inside(0.5),
            refraction: Refraction::new(0.9).reach(1.5).softness(5.0),
            stages: &[],
        };
        let packed = pack_glass_uniforms(&frame);
        let data: Vec<[f32; 4]> = packed
            .chunks_exact(4)
            .map(|c| [c[0], c[1], c[2], c[3]])
            .collect();
        let (s, sigma, iters) =
            refraction_replay(&data).expect("halo params must survive the pack");
        assert!((s - 0.9).abs() < 1e-6, "gain, got {s}");
        assert!((sigma - 8.0).abs() < 1e-6, "tight sigma length/6, got {sigma}");
        assert_eq!(iters, 3, "5 taps → 3 H+V iterations");
    }

    #[test]
    fn neutral_when_absent() {
        let v = Glass::lens(1.0).effective(&Backdrop::NONE);
        assert_eq!(v.blur_sigma, 0.0, "frost falls back to nothing");
        assert_eq!(v.fill_opacity, 0.0, "wash falls back to off");
        assert_eq!(v.saturation, 1.0, "saturation falls back to untouched");
        // Tone is the deliberate exception: absent keeps the reference curve.
        assert_eq!(
            v.tone_knots,
            crate::foundation::glass::FIXED_REFERENCE_CURVE.knots
        );
    }

    #[test]
    fn explicit_stages_reach_constants() {
        use crate::foundation::Color;
        let g = Glass::lens(1.0).rim(0.0);
        let b = Backdrop::new()
            .blur(1.28)
            .wash(Color::hex_alpha(0xFEFCFC, 0.1705))
            .saturation(0.9726)
            .tone(0.0);
        let v = g.effective(&b);
        assert_eq!(v.blur_sigma, 1.28);
        assert_eq!(v.fill_opacity, 0.1705);
        assert_eq!(v.saturation, 0.9726);
        assert_eq!(v.tone_knots, IDENTITY_TONE_KNOTS, "tone 0 must be identity");
        assert_eq!(v.rim_intensity, 0.0);
        assert_eq!(v.rim_mix, 0.0);
    }

    #[test]
    fn geometry_knobs_flow() {
        let g = Glass::lens(1.0)
            .lens_edge(10.0)
            .post_share(0.31)
            .rim_mix_cut(3.97)
            .frost_wide(1.7973);
        let b = Backdrop::new()
            .ambient(0.18)
            .tint_strength(0.35)
            .blur_size_ref(0.0);
        let v = g.effective(&b);
        assert_eq!(v.lens_edge, 10.0);
        assert_eq!(v.ambient_mix, 0.18);
        assert_eq!(v.post_blur_share, 0.31);
        assert_eq!(v.rim_mix_cut, 3.97);
        assert_eq!(v.tint_strength, 0.35);
        assert_eq!(v.frost_wide_sigma, 1.7973);
        assert_eq!(v.blur_size_ref, 0.0);
    }

    #[test]
    fn shape_effective_matches_material() {
        let g = Glass::lens(2.0).rim(0.0);
        let b = Backdrop::new().tone(0.0);
        let s = shape_with(g, b);
        let a = s.effective();
        let c = g.effective(&b);
        assert_eq!(a, c, "shape and material must agree");
    }

    #[test]
    fn hidden_shapes_do_not_draw() {
        let g = Glass::identity();
        let s = shape_with(g, Backdrop::NONE);
        assert!(s.hidden);
        assert!(!super::drawable(&s));
    }

    #[test]
    fn stage_tail_roundtrip() {
        use crate::foundation::{Glow, Refraction, Spectral, GLASS_STANDARD};
        let shape = GlassShapeUniform::new(
            ResolvedRect::new(0.0, 0.0, 200.0, 150.0),
            24.0,
            false,
        );
        let frame = GlassFrameUniforms {
            shapes: std::slice::from_ref(&shape),
            device_pixel_ratio: 1.0,
            light_angle: 0.0,
            smoothing: 0.0,
            constants: &GLASS_STANDARD,
            high_contrast: false,
            opaque_color: None,
            touch: None,
            glow: 0.0,
            blur_sigma: 0.0,
            blur_aspect: 1.0,
            spectral: Spectral::NONE,
            voice_glow: Glow::NONE,
            refraction: Refraction::NONE,
            stages: &[1, 5, 8],
        };
        let packed = pack_glass_uniforms(&frame);
        assert_eq!(packed.len(), K_GLASS_UNIFORM_FLOATS);
        let d = K_GLASS_WIRE_FLOATS;
        assert_eq!(packed[d], 3.0, "count first");
        assert_eq!(packed[d + 1], 1.0);
        assert_eq!(packed[d + 2], 5.0);
        assert_eq!(packed[d + 3], 8.0);
        assert_eq!(packed[d + 4], 0.0, "rest of the first vec4 pads to zero");
        // Wire frozen at the glow tail: the stage + GGX tails must not
        // ride it (both wgpu-only, appended after the frozen v5 prefix).
        assert_eq!(
            K_GLASS_WIRE_FLOATS,
            K_GLOW_AT + 4 * K_GLOW_VEC4S,
            "wire stream must end after the glow tail"
        );
        assert_eq!(
            K_GLASS_UNIFORM_FLOATS,
            K_GLASS_WIRE_FLOATS + K_STAGES_FLOATS + K_GGX_FLOATS,
            "uniform stream is wire prefix + stage tail + GGX tail"
        );
        assert_eq!(
            GLASS_WIRE_PACKED_FLOATS,
            K_GLASS_WIRE_FLOATS,
            "wire length frozen at 792"
        );
        assert_eq!(GLASS_WIRE_PACKED_WORDS, 7 + K_GLASS_WIRE_FLOATS);
    }

    #[test]
    fn stage_tail_defaults_to_canonical() {
        use crate::foundation::{Glow, Refraction, Spectral, GLASS_STANDARD};
        let shape = GlassShapeUniform::new(
            ResolvedRect::new(0.0, 0.0, 200.0, 150.0),
            24.0,
            false,
        );
        let frame = GlassFrameUniforms {
            shapes: std::slice::from_ref(&shape),
            device_pixel_ratio: 1.0,
            light_angle: 0.0,
            smoothing: 0.0,
            constants: &GLASS_STANDARD,
            high_contrast: false,
            opaque_color: None,
            touch: None,
            glow: 0.0,
            blur_sigma: 0.0,
            blur_aspect: 1.0,
            spectral: Spectral::NONE,
            voice_glow: Glow::NONE,
            refraction: Refraction::NONE,
            stages: &[],
        };
        let packed = pack_glass_uniforms(&frame);
        let d = K_GLASS_WIRE_FLOATS;
        assert_eq!(packed[d], 0.0, "empty list = count 0 = canonical chain");
        assert!(packed[d + 1..].iter().all(|f| *f == 0.0));
    }

    #[test]
    fn wire_frame_stops_at_the_glow_tail() {
        // The wire packer must never carry stage ids: islands run the
        // canonical chain until the plugin grows a dispatch.
        let wire = pack_glass_wire(
            ResolvedRect::new(0.0, 0.0, 100.0, 100.0),
            &crate::foundation::GlassLayer::Single(
                crate::foundation::Glass::lens(1.0),
                crate::foundation::Backdrop::NONE,
                crate::foundation::Spectral::NONE,
                crate::foundation::Glow::NONE,
                crate::foundation::Refraction::NONE,
            ),
            1.0,
        );
        assert_eq!(wire.len(), GLASS_WIRE_PACKED_WORDS);
        assert_eq!(wire[0], GLASS_WIRE_PACKED_VERSION);
        let floats = &wire[7..];
        let d = K_GLASS_WIRE_FLOATS;
        assert_eq!(floats.len(), d);
        // The wire's last float is the glow tail's final slot, not a count.
        assert_eq!(
            floats.len(),
            K_GLASS_UNIFORM_FLOATS - K_STAGES_FLOATS - K_GGX_FLOATS
        );
        // The floats before the stage tail belong to the glow-tail region;
        // with glow inactive it packs zeros (inactive lanes keep zeros).
        assert_eq!(
            floats[K_GLOW_AT..],
            [0u32; K_GLOW_VEC4S * 4],
            "glow tail inactive → zeros"
        );
    }

    #[test]
    fn glass_stage_ids_cover_the_dispatch() {
        use crate::foundation::effect::BuiltinStage;
        // Actual contract: units map to their dispatch ids.
        assert_eq!(BuiltinStage::Refract.glass_stage_ids(), &[1]);
        assert_eq!(BuiltinStage::Frost.glass_stage_ids(), &[2]);
        assert_eq!(BuiltinStage::Spectral.glass_stage_ids(), &[3]);
        assert_eq!(BuiltinStage::Saturation.glass_stage_ids(), &[4]);
        assert_eq!(BuiltinStage::Tone.glass_stage_ids(), &[5, 8]);
        assert_eq!(BuiltinStage::Wash.glass_stage_ids(), &[6]);
        assert_eq!(BuiltinStage::Dim.glass_stage_ids(), &[7]);
        assert_eq!(BuiltinStage::Tint.glass_stage_ids(), &[9]);
        assert_eq!(BuiltinStage::Ambient.glass_stage_ids(), &[10]);
        assert_eq!(BuiltinStage::Glow.glass_stage_ids(), &[11]);
        assert_eq!(BuiltinStage::Rim.glass_stage_ids(), &[12]);
        assert_eq!(BuiltinStage::Press.glass_stage_ids(), &[13]);
        assert_eq!(BuiltinStage::Fresnel.glass_stage_ids(), &[14]);
        assert_eq!(BuiltinStage::Ggx.glass_stage_ids(), &[15]);
        assert!(BuiltinStage::Blend { mode: crate::foundation::BlendMode::Normal }
            .glass_stage_ids()
            .is_empty());
        assert!(BuiltinStage::Blur { sigma: 4.0 }.glass_stage_ids().is_empty());
    }
}
