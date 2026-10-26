// Liquid-glass optics pass — substrate. Line-for-line WGSL port of
// `adaptive_liquid_glass/shaders/liquid_glass.frag` (the Flutter
// package's fragment shader), cut into one module per effect so the
// two files still diff by eye. Function names (`sdSuperellipseBox`,
// `shapeOf`, `smin`, `lensField`, `lensBlend`, `toneLut`) and stage
// order are kept. Deviations are the four design decisions of
// `ongoing-liquid-glass-upgrade.md`:
//
//  1. Display-referred. The backdrop texture holds premultiplied sRGB;
//     `tex` unpremultiplies and every stage runs on the encoded values.
//     No sRGB<->linear round trip, and no Fresnel / environment probe /
//     spec-band / KP-stroke stages (P1, absent from the reference).
//  2. The `dark` flag picks the *Dark constant set on the CPU
//     (`GlassConstants::of`); the shader still blends regular <-> clear
//     per shape through `u_info[i].y` exactly as the reference does.
//  3. The engine pre-blurs the backdrop once at the composed sigma
//     (anisotropic sx/sy, `engine`) before this pass. The wide tail, the
//     post-lens blur and per-shape extra frost are taps on that one
//     texture — the reference's `uTexture` is likewise "the backdrop
//     already blurred by ImageFilter.blur".
//  4. Every texture read uses `textureSampleLevel` (explicit LOD 0):
//     the early returns in `fs_main` put `textureSample` in non-uniform
//     control flow, which naga rejects.
//
// Module map (tiers of `liquid/writeup/08-our-implementation-glass-separation.md`;
// `GLASS_COMBINED_WGSL` concatenates substrate -> stages -> orchestrator,
// one file per effect):
//
//   substrate  core.wgsl        this file: uniform, bindings, vertex,
//                               backdrop read, `GlassCtx`
//   tier B     shape.wgsl       silhouette, SDF union, lens normal
//              refract.wgsl     lens displacement -> `sample_point`
//              frost.wgsl       core / post-lens / extra / wide taps
//              spectral.wgsl    per-wavelength widening of the sample
//   tier C     saturation.wgsl  luma-keyed chroma grade
//              tone.wgsl        tone LUT (+ luma-keyed, lift, small shape)
//              wash.wgsl        fill wash
//              dim.wgsl         post-tone multiply
//              tint.wgsl        flat tint mix
//              ambient.wgsl     backdrop hue pickup
//   tier D     glow.wgsl        emissive voice-glow wash
//              rim.wgsl         specular streak + isotropic rim
//              press.wgsl       press lift + touch Gaussian
//   tier E     card_blend.wgsl  CSS mix-blend-mode against the sharp
//                               snapshot (in-shader). `Shadow` and
//                               `LightSpill` are renderer-side draws,
//                               not stages of this pass.
//   pipe       fs_main.wgsl     the orchestrator: the canonical order
//
// One module is not one render pass: the tiers are strictly ordered
// inside a single fragment pass, and the glass pack travels the wire as
// one effect (Phase E: "glass rides as the pack, never as a stage").
// The division buys separable knobs and a place to put the layers we do
// not have yet (Absorb, Fresnel, GGX, Dither) — not a pass split.
//
// Group 0: binding 0 backdrop (surface-sized, premultiplied sRGB, same
// coordinates as the fragment position — so `px / u.size` samples the
// pixel under `px`; may be the core pre-blur), binding 1 a linear
// sampler, binding 2 the uniform, binding 3 the *sharp* parent
// snapshot (the card-blend backdrop — binding 0's blur must not leak
// into the mix-blend-mode composite).
// The pass renders a fullscreen triangle into a cleared offscreen
// (blend disabled); outside the silhouette the SDF stage returns
// transparent, inside it refracts the frosted backdrop and shades it.

struct GlassU {
    // Engine header (not in the reference): composed core blur sigma in
    // device px, sx horizontal and sy vertical. The shader does not read
    // it — the CPU used it to blur the backdrop — but it rides along so
    // one buffer carries the whole pass.
    engine: vec4<f32>,
    // Reference `uSize`: backdrop size in device px (xy).
    size: vec4<f32>,
    // count, dpr, lightAngle (radians), opaque flag.
    u_global: vec4<f32>,
    // default smoothing px (per shape: u_extra.y), cornerExponent,
    // highContrast, cornerZone.
    u_global2: vec4<f32>,
    // opaque fill rgb.
    u_opaque: vec4<f32>,
    // touch x, y, glow, glowRadius px.
    u_touch: vec4<f32>,
    u_rects: array<vec4<f32>, 24>,
    // radius px, reserved (always 0 — the second bank is a packing
    // duplicate, never selected), cornerExponent (0 = global),
    // fill scale.
    u_info: array<vec4<f32>, 24>,
    // rgb, tint strength.
    u_tints: array<vec4<f32>, 24>,
    // Single bank, shipped twice for wire stability (the second bank
    // at +12/+3/+1 offsets duplicates the first; the shader only ever
    // reads the first):
    //   A(lens decay px, band px, lens strength, retired) B(rimW, rimI,
    //   fillOpacity, dim) C(retired, retired, tintS, lens size ref px)
    //   D(fillR, fillG, fillB, saturation) E(frost wide sigma px, wide
    //   mix edge, wide mix centre, wide size ref px) F(wide size drop,
    //   glow strength, post-lens blur share, normal radius scale) G, H,
    //   I.x: tone LUT, 9 grey output knots at inputs i/8; I.yzw(lens
//   edge px, lens edge decay px, tone lift) J(rim mix, rim mix width
//   px, rim mix cut px, rim mix luma floor) K(tone lift knee, tone
//   lift size ref px, post-lens sigma px, blur size ref px) L(rim
//   back strength, fresnel F0, fresnel power, fresnel gain) M, N,
//   O.x: small-shape
    //   tone LUT; O.yz(smallSizeLo px,
    //   smallSizeHi px); O.w post-lens Jacobian clamp P(ambient mix,
    //   ambient reach px, luma-keyed tone mix, retired)
    //   Q(spectral base IOR, spread, reach, voice-glow gain — see
    //   spectralWeight: the refraction sampler's independent spectrum,
    //   not a lens stage; Q.w = 0 takes the no-glow path)
    //   R(glow inside gain, refraction halo gain, press lift, -)
    u_var: array<vec4<f32>, 36>,
    // group index, smoothing px, opacity, press light 0..1.
    u_extra: array<vec4<f32>, 24>,
    // clip x0, y0, x1, y1 px (±1e9 when there is no clip).
    u_clip: array<vec4<f32>, 24>,
    // extra sigma x px, extra sigma y px, taps, - (per-shape frost the
    // backdrop's blur lacks; the Gaussians compose as sigma² sums).
    u_blur: array<vec4<f32>, 24>,
    // Glow tail (wgpu-only, appended — every v4 offset above is stable;
    // the v4 compositor wire carries the 736-float prefix and never sees
    // these, its shader has no glow port yet):
    // lobes (centre, sigma, amplitude px along x), colours,
    // geom (cross-edge line px, sigma px, refracted flag 0/1, blend mode
    // id 0..15 — see `blend.wgsl`), outer rule (spill length device px,
    // tap count, boost, alpha cap — LightSpill).
    u_glow_lobes: array<vec4<f32>, 6>,
    u_glow_cols: array<vec4<f32>, 6>,
    u_glow_geom: vec4<f32>,
    u_outer: vec4<f32>,
    // Stage list (wgpu-only, appended after the glow tail — the v5 wire
    // stops at the tail and its shader has no dispatch): count in [0].x,
    // then up to 16 author-ordered stage ids across the five vec4s
    // (ids 1..16 — see `fs_main`'s switch). Count 0 = run the canonical
    // chain, byte-identical to a stack with no unit items.
    u_stages: array<vec4<f32>, 5>,
    // GGX knobs (wgpu-only, appended after the stage list — the v5 wire
    // stops at the tail): [0].x roughness α, [0].y gain (0 = the stage
    // no-ops), [0].zw + [1].x the light direction xyz.
    u_ggx: array<vec4<f32>, 2>,
    // EnvReflect knobs (wgpu-only, appended after the GGX tail — the v5
    // wire stops at the tail): [0].x gain (0 = the stage no-ops),
    // [0].y sky strength, [0].z the mirror travel cap in px, [0].w the
    // sky horizon lift, [1].x the sky zenith gain.
    u_env: array<vec4<f32>, 2>,
};

@group(0) @binding(0)
var backdrop: texture_2d<f32>;
@group(0) @binding(1)
var samp: sampler;
@group(0) @binding(2)
var<uniform> u: GlassU;
@group(0) @binding(3)
var sharp: texture_2d<f32>;

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

// Backdrop sample at device px `px`, clamped to the texture, returned
// unpremultiplied (the reference reads straight `tex().rgb`; our
// offscreens store premultiplied sRGB). `textureSampleLevel`, not
// `textureSample`: the early returns in `fs_main` make the control flow
// non-uniform.
fn tex(px: vec2<f32>) -> vec4<f32> {
    let uv = clamp(px / u.size.xy, vec2<f32>(0.0), vec2<f32>(1.0));
    let s = textureSampleLevel(backdrop, samp, uv, 0.0);
    let a = max(s.a, 1.0 / 255.0);
    return vec4<f32>(s.rgb / a, s.a);
}

/// Per-pixel state threaded through the glass effect stages. The
/// `EffectCtx`-shared names mean what they mean in `effect_abi.wgsl`
/// (`px`, `normal`, `coverage`), so a stack stage and a glass stage read
/// the context the same way. `d` is the `EffectCtx`-shaped signed
/// distance (negative inside); glass's own `depth` is `-d` (positive
/// inside) and must NOT be read as `EffectCtx.depth` — that field is
/// "distance to the clip silhouette, `1e6` when no clip stage has run",
/// a different quantity. The rest are the optics intermediates the
/// stages hand to each other. `sample_point` is the position every
/// backdrop read agrees on: `px` until a stage refracts it.
struct GlassCtx {
    px: vec2<f32>,
    d: f32,
    depth: f32,
    coverage: f32,
    keep: f32,
    ownK: f32,
    ownGrp: f32,
    count: i32,
    halfMin: f32,
    fillScale: f32,
    light: f32,
    tint: vec4<f32>,
    rcB: vec4<f32>,
    xb: vec4<f32>,
    fxp: f32,
    fxm: f32,
    fyp: f32,
    fym: f32,
    normal: vec2<f32>,
    /// 3-D surface normal from the height field (`stageShape` publishes
    /// the flat baseline `(normal.xy, 1)`; `stageRefract` bends it with
    /// the lens profile slope via the Snell lift). Enables `dot(N, V)`
    /// shading effects (`Fresnel`, GGX). Mirrors `EffectCtx.normal`.
    n3: vec3<f32>,
    /// Optical path through the glass in device px, written by
    /// `stageRefract` (our glass is a half-space: the refracted travel
    /// distance is the compressed read depth). 0 when Refract did not
    /// run — that is the `Absorb`/`Slab` flat-mix degeneration. Mirrors
    /// `EffectCtx.path_length`.
    path_length: f32,
    gradLen: f32,
    lensAmt: f32,
    dLens: f32,
    vv: f32,
    sample_point: vec2<f32>,
    base: vec3<f32>,
    col: vec3<f32>,
    sw: f32,
};
