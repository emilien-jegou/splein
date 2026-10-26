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
    // Tier D — GGX (A2): microfacet directional glint off `n3`, gated to
    // the bevel band.
    include_str!("shaders/glass/ggx.wgsl"),
    "\n",
    // Tier D — EnvReflect (A10): backdrop mirror + sky, Fresnel-weighted.
    include_str!("shaders/glass/env_reflect.wgsl"),
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

/// Engine-side entry scaffold for custom (material-DSL) stages — the
/// twin of the plugin glue's `GLUE_ENTRY` (`plugins/riode-shader-glue/
/// src/lib.rs`). Both compose the *same* complete module the wire
/// carries: `EFFECT_ABI_WGSL` + the custom `apply` + this entry. The
/// glue's runs under naga→GLSL on the compositor plugin; this one
/// compiles directly on wgpu. **Keep the two in step** (D-M7):
///
/// Same by contract — binding numbering (0 `effect_sampler` from the
/// ABI, 1 uniforms, 2 input/card, 3 backdrop, 4 sharp), the
/// identity-`ctx` prelude, sampling the input into `ctx.color`, the
/// `apply(ctx, backdrop, sharp, u)` call, returning `out_ctx.color`.
///
/// Deliberate divergences — (1) this entry carries its own `vs_main`
/// (the plugin's GlassOptics supplies the vertex stage; wgpu needs one
/// in-module), and (2) every sample uses `textureSampleLevel(_, 0.0)`
/// rather than `textureSample`: `apply` may contain early returns, which
/// put `textureSample` in non-uniform control flow that naga rejects
/// (`glass/core.wgsl` decision 4).
pub const ENGINE_ENTRY_WGSL: &str = r#"
@group(0) @binding(1) var<uniform> engine_u: CustomUniforms;
@group(0) @binding(2) var engine_input: texture_2d<f32>;
@group(0) @binding(3) var engine_backdrop: texture_2d<f32>;
@group(0) @binding(4) var engine_sharp: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) ix: u32) -> @builtin(position) vec4<f32> {
    var p = vec2<f32>(-1.0, 1.0);
    switch ix {
        case 1u { p = vec2<f32>(-1.0, -1.0); }
        case 2u, 4u { p = vec2<f32>(1.0, -1.0); }
        case 5u { p = vec2<f32>(1.0, 1.0); }
        default {}
    }
    return vec4<f32>(p, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(engine_input));
    let uv = clamp(pos.xy / dims, vec2<f32>(0.0), vec2<f32>(1.0));
    var ctx = effect_ctx_identity(pos.xy);
    ctx.color = textureSampleLevel(engine_input, effect_sampler, uv, 0.0);
    let out_ctx = apply(ctx, engine_backdrop, engine_sharp, engine_u);
    return out_ctx.color;
}
"#;
