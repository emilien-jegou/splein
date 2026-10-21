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

/// Liquid-glass optics pass (SDF lens, frost, tone, glow, rim, card blend).
/// Phase C decomposes this into per-stage functions over `EffectCtx`;
/// until then it ships verbatim.
pub const GLASS_WGSL: &str = include_str!("shaders/glass.wgsl");

/// Glass optics module: blend libs + dispatcher + optics, exactly as the
/// wgpu engine compiles it (WGSL has no includes; order is free).
pub const GLASS_COMBINED_WGSL: &str = concat!(
    include_str!("shaders/blend_nonseparable.wgsl"),
    "\n",
    include_str!("shaders/blend_separable.wgsl"),
    "\n",
    include_str!("shaders/blend.wgsl"),
    "\n",
    include_str!("shaders/glass.wgsl"),
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
