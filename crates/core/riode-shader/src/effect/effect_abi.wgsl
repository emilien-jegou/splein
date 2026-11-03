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
    /// 3-D surface normal (the widened field, substrate decision D1):
    /// xy is the lens/clip normal as before, z is the height-field lift
    /// — `(normal.xy, 1.0)` is the flat surface, the glass host bends z
    /// with the lens profile slope (see `glass/refract.wgsl`). Shading
    /// effects (`Fresnel`, GGX) form `dot(N, V)` from this. The
    /// composition contract: every stage that bends the surface writes
    /// it, everything downstream of it reads it — absent producers
    /// leave the flat baseline, so consumer stages degrade to grazing
    /// angle 0, not NaN.
    normal: vec3<f32>,
    /// Optical path through the glass in device px (the other widened
    /// field): the glass host writes it where a real depth exists (the
    /// inside depth, see `glass/refract.wgsl`). 0 when no `Refract`
    /// stage ran — `Absorb`/`Slab`-style consumers degenerate to the
    /// flat mix (the pre-widen look), so the field is backward-safe.
    path_length: f32,
    /// Clip coverage 0..1 (anti-aliased edge alpha from `ClipRound`).
    coverage: f32,
    /// Card origin in device px (top-left of the layer region being
    /// shaded). Phase 5a: seeds `geo.uv()` / `geo.local_px()` /
    /// `geo.edge_dist()` so patterns stick to cards while scrolling
    /// (retires the G1b device-space swim defect). The wgpu post-glass
    /// scaffold fills these from the layer `open.region`; the plugin
    /// fills from its island box; unwired producers leave (0,0,0,0)
    /// and card-local fields degrade to device space.
    card_rect: vec4<f32>,
};

/// Card-local helpers shared by every custom module. `ctx.card_rect`
/// packs (origin.x, origin.y, width, height) in device px — see the
/// `card_rect` field above.
fn ctx_local_px(ctx: EffectCtx) -> vec2<f32> {
    return ctx.px - ctx.card_rect.xy;
}
fn ctx_uv(ctx: EffectCtx) -> vec2<f32> {
    let size = max(ctx.card_rect.zw, vec2<f32>(1.0));
    return ctx_local_px(ctx) / size;
}
fn ctx_edge_dist(ctx: EffectCtx) -> f32 {
    // Sharp-rect proxy (documented): module-scope helpers cannot read the
    // uniform block, so the radius-aware field (`ctx_edge_dist_at`) is
    // called inline by the lowerings with the slot-8 corner radii. This
    // one is the Beer-travel/path bulk estimate only.
    return ctx_edge_dist_at(ctx_local_px(ctx), ctx.card_rect, vec4<f32>(0.0));
}

/// Rounded-rect signed distance, inside-positive: the card's actual
/// silhouette. `rc` = corner radii in device px (top-left, top-right,
/// bottom-right, bottom-left — slot 8, host-W3C-resolved and clamped);
/// zero radii degenerate to the plain rect distance exactly as the old
/// helper computed it.
fn ctx_edge_dist_at(lp: vec2<f32>, rect: vec4<f32>, rc: vec4<f32>) -> f32 {
    let size = max(rect.zw, vec2<f32>(1.0));
    let center = size * 0.5;
    let rel = lp - center;
    // Per-quadrant radius (rc = tl, tr, br, bl): the point's quadrant
    // selects its own corner.
    let r = max(
        select(select(rc.y, rc.x, rel.x <= 0.0), select(rc.z, rc.w, rel.x <= 0.0), rel.y > 0.0),
        0.0,
    );
    let q = abs(rel) - (center - vec2<f32>(r));
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    return -d;
}

/// Identity context: transparent, infinitely far from any clip.
fn effect_ctx_identity(px: vec2<f32>) -> EffectCtx {
    var ctx: EffectCtx;
    ctx.color = vec4<f32>(0.0);
    ctx.px = px;
    ctx.depth = 1e6;
    ctx.normal = vec3<f32>(0.0, 0.0, 1.0);
    ctx.path_length = 0.0;
    ctx.coverage = 1.0;
    ctx.card_rect = vec4<f32>(0.0);
    return ctx;
}

/// Uniform block for custom stages: whole vec4s, padded by the client
/// to the fixed 9-vec4 stream the wire carries (36 f32s). Reads index
/// `u.data[i]`; use `u.data` lengths the uniforms actually fill.
///
/// Host-reserved tail (Phase 5a + corner-radius follow): the DSL leaves
/// `data[7]` (card rect) and `data[8]` (corner radii) empty — the
/// executor appends them per pass. Unwired producers leave zeros:
/// `data[8]` zero means a sharp card and every consumer degenerates to
/// the pre-radius look, exactly as before.
struct CustomUniforms {
    data: array<vec4<f32>, 9>,
};

/// Shared sampler for custom effects (textureSample goes through this;
/// `apply` gets textures but no sampler). Binding 0 of group 0 — the
/// executor owns bindings 1..5 of the same group.
@group(0) @binding(0) var effect_sampler: sampler;
