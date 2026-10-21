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
// card-blend stages. Builtin stages follow the same convention with
// their own uniform structs.

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
