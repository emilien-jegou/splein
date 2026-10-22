// Effect-stack ABI: the standard plug shape every stage implements,
// builtin or custom.
//
// A stage reads an `EffectCtx`, transforms it, and returns it. The
// executor chains stages in stack order (bottom → top); `depth` and
// `coverage` carry the clip state downstream (written by `ClipRound`,
// read by glass refraction, shadows, and glow).
//
// Custom shaders implement exactly one entry point with this signature:
//
//   fn apply(ctx: EffectCtx, backdrop: texture_2d<f32>, sharp: texture_2d<f32>, u: CustomUniforms) -> EffectCtx
//
// `backdrop` is the parent snapshot (meaningful only when the stage
// declared `BackdropNeed`); `sharp` is the unblurred parent snapshot for
// card-blend stages. Sampling goes through `effect_sampler` (below) —
// `apply` receives textures but no sampler. Builtin stages follow the
// same convention with their own uniform structs.
//
// On the compositor wire this header rides every custom module
// (client-prepended); the transpile entry (riode-shader-glue) appends
// `fs_main` and binds: 0 sampler (here), 1 uniforms, 2 input,
// 3 backdrop, 4 sharp.

/// Per-pixel state threaded through the effect stack.
struct EffectCtx {
    /// Premultiplied color accumulated so far.
    color: vec4<f32>,
    /// Device-px position of the fragment being shaded.
    px: vec2<f32>,
    /// Signed distance to the clip silhouette (negative inside).
    /// `1e6` when no clip stage has run.
    depth: f32,
    /// Lens normal from the SDF gradient (meaningful inside the shape).
    normal: vec2<f32>,
    /// Clip coverage 0..1 (anti-aliased edge alpha from `ClipRound`).
    coverage: f32,
};

/// Identity context: transparent, infinitely far from any clip.
fn effect_ctx_identity(px: vec2<f32>) -> EffectCtx {
    var ctx: EffectCtx;
    ctx.color = vec4<f32>(0.0);
    ctx.px = px;
    ctx.depth = 1e6;
    ctx.normal = vec2<f32>(0.0);
    ctx.coverage = 1.0;
    return ctx;
}

/// Uniform block for custom stages: whole vec4s, padded by the client
/// to the fixed 8-vec4 stream the wire carries (32 f32s). Reads index
/// `u.data[i]`; use `u.data` lengths the uniforms actually fill.
struct CustomUniforms {
    data: array<vec4<f32>, 8>,
};

/// Shared sampler for custom effects (textureSample goes through this;
/// `apply` gets textures but no sampler). Binding 0 of group 0 — the
/// executor owns bindings 1..5 of the same group.
@group(0) @binding(0) var effect_sampler: sampler;
