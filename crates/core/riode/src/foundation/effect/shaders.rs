// Single responsibility: single source of truth for effect-stack WGSL.
//
// These sources are the exact modules the wgpu engine compiles into its
// pipelines (see `riode-engine-wgpu/src/pipeline.rs`) and the exact text
// shipped program → compositor for custom/builtin island effects. One copy,
// referenced by path from every consumer — never duplicated.

/// Shared `EffectCtx` + `apply` ABI all stages (builtin and custom) implement.
pub const EFFECT_ABI_WGSL: &str = include_str!("shaders/effect_abi.wgsl");

/// Separable Gaussian blur pass over a filter-layer offscreen.
pub const BLUR_WGSL: &str = include_str!("shaders/blur.wgsl");

/// Color-matrix composite of a (possibly blurred) layer onto its parent.
pub const COMPOSITE_WGSL: &str = include_str!("shaders/composite.wgsl");

/// Backdrop-blend composite (layer + parent snapshot → CSS composite).
pub const COMPOSITE_BLEND_WGSL: &str = include_str!("shaders/composite_blend.wgsl");

/// `blendMode` dispatcher (concatenated after the separable/non-separable libs).
pub const BLEND_WGSL: &str = include_str!("shaders/blend.wgsl");

/// Separable blend-mode functions (PDF 1.4 / CSS Compositing Level 1).
pub const BLEND_SEPARABLE_WGSL: &str = include_str!("shaders/blend_separable.wgsl");

/// Non-separable blend-mode functions (hue/saturation/color/luminosity).
pub const BLEND_NONSEPARABLE_WGSL: &str = include_str!("shaders/blend_nonseparable.wgsl");

/// Liquid-glass optics module, exactly as the wgpu engine compiles it:
/// the blend library (dispatcher + libs), then the glass modules in
/// `shaders/glass/` — one file per effect, in the order substrate ->
/// shape -> refract -> frost -> spectral -> the Tier C colour chain ->
/// the Tier D lighting -> card blend -> orchestrator. This is the single
/// module list; there is no second glass source to keep in sync (WGSL
/// has no includes; order is free — module-scope declarations are
/// hoisted, see `pipeline.rs` — so this order documents dependencies
/// rather than enforcing them).
pub const GLASS_COMBINED_WGSL: &str = concat!(
    include_str!("shaders/blend_nonseparable.wgsl"),
    "\n",
    include_str!("shaders/blend_separable.wgsl"),
    "\n",
    include_str!("shaders/blend.wgsl"),
    "\n",
    // Substrate: uniform, bindings, vertex, backdrop read, `GlassCtx`.
    include_str!("shaders/glass/core.wgsl"),
    "\n",
    // Tier B1 — Shape (08.5): geometry, silhouette, lens normal.
    include_str!("shaders/glass/shape.wgsl"),
    "\n",
    // Tier B2 — Refract (08.5): lens displacement -> `sample_point`.
    include_str!("shaders/glass/refract.wgsl"),
    "\n",
    // Tier B3 — Frost (08.5): core / post-lens / extra / wide taps.
    include_str!("shaders/glass/frost.wgsl"),
    "\n",
    // Tier B4 — Spectral (08.5): per-wavelength widening of the sample.
    include_str!("shaders/glass/spectral.wgsl"),
    "\n",
    // Tier C2 — Saturation (08.6): luma-keyed chroma grade.
    include_str!("shaders/glass/saturation.wgsl"),
    "\n",
    // Tier C3 — ToneCurve (08.6): tone LUT (+ luma-keyed, lift,
    // small-shape tone curve).
    include_str!("shaders/glass/tone.wgsl"),
    "\n",
    // Tier C4 — Wash (08.6): fill wash.
    include_str!("shaders/glass/wash.wgsl"),
    "\n",
    // Tier C5 — Dim (08.6): post-tone multiply.
    include_str!("shaders/glass/dim.wgsl"),
    "\n",
    // Tier C6 — Tint (08.6): flat tint mix.
    include_str!("shaders/glass/tint.wgsl"),
    "\n",
    // Tier C1 — Ambient (08.6): backdrop hue pickup.
    include_str!("shaders/glass/ambient.wgsl"),
    "\n",
    // Tier D — Glow (08.7): emissive voice-glow wash.
    include_str!("shaders/glass/glow.wgsl"),
    "\n",
    // Tier D — Rim (08.7): specular streak + isotropic rim.
    include_str!("shaders/glass/rim.wgsl"),
    "\n",
    // Tier D — Fresnel (A1): Schlick edge glint off the widened `n3`.
    include_str!("shaders/glass/fresnel.wgsl"),
    "\n",
    // Tier D — Press (08.7): press lift + touch Gaussian.
    include_str!("shaders/glass/press.wgsl"),
    "\n",
    // Tier E — CardBlend (08.7): CSS mix-blend-mode composite.
    include_str!("shaders/glass/card_blend.wgsl"),
    "\n",
    // Orchestrator: `fs_main`'s canonical stage order.
    include_str!("shaders/glass/fs_main.wgsl"),
);

/// Backdrop-blend composite module: blend libs + dispatcher + composite.
pub const BLEND_COMPOSITE_COMBINED_WGSL: &str = concat!(
    include_str!("shaders/blend_nonseparable.wgsl"),
    "\n",
    include_str!("shaders/blend_separable.wgsl"),
    "\n",
    include_str!("shaders/blend.wgsl"),
    "\n",
    include_str!("shaders/composite_blend.wgsl"),
);
