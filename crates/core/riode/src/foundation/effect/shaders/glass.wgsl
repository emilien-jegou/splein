// Liquid-glass optics pass — line-for-line WGSL port of
// `adaptive_liquid_glass/shaders/liquid_glass.frag` (the Flutter
// package's fragment shader). Function names (`sdSuperellipseBox`,
// `shapeOf`, `smin`, `lensField`, `lensBlend`, `toneLut`) and
// stage order are kept so the two files diff by eye. Deviations are the
// four design decisions of `ongoing-liquid-glass-upgrade.md`:
//
//  1. Display-referred. The backdrop texture holds premultiplied sRGB;
//     `tex` unpremultiplies and every stage runs on the encoded values.
//     No sRGB↔linear round trip, and no Fresnel / environment probe /
//     spec-band / KP-stroke stages (P1, absent from the reference).
//  2. The `dark` flag picks the *Dark constant set on the CPU
//     (`GlassConstants::of`); the shader still blends regular ↔ clear
//     per shape through `u_info[i].y` exactly as the reference does.
//  3. The engine pre-blurs the backdrop once at the composed sigma
//     (anisotropic σx/σy, `engine`) before this pass. The wide tail, the
//     post-lens blur and per-shape extra frost are taps on that one
//     texture — the reference's `uTexture` is likewise "the backdrop
//     already blurred by ImageFilter.blur".
//  4. Every texture read uses `textureSampleLevel` (explicit LOD 0):
//     the early returns below put `textureSample` in non-uniform control
//     flow, which naga rejects.
//
// Group 0: binding 0 backdrop (surface-sized, premultiplied sRGB, same
// coordinates as the fragment position — so `px / uSize` samples the
// pixel under `px`; may be the core pre-blur), binding 1 a linear
// sampler, binding 2 the uniform, binding 3 the *sharp* parent
// snapshot (the card-blend backdrop — binding 0's blur must not leak
// into the mix-blend-mode composite).
// The pass renders a fullscreen triangle into a cleared offscreen
// (blend disabled); outside the silhouette the SDF stage returns
// transparent, inside it refracts the frosted backdrop and shades it.

struct GlassU {
    // Engine header (not in the reference): composed core blur sigma in
    // device px, σx horizontal and σy vertical. The shader does not read
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
    //   back strength, retired, retired, retired) M, N, O.x: small-shape
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

fn sdSuperellipseBox(p: vec2<f32>, halfSize: vec2<f32>, r0: f32, n0: f32) -> f32 {
    let r = min(r0, min(halfSize.x, halfSize.y));
    let q = abs(p) - halfSize + r;
    let m = max(q, vec2<f32>(0.0));
    let corner = pow(pow(m.x, n0) + pow(m.y, n0), 1.0 / n0);
    return corner + min(max(q.x, q.y), 0.0) - r;
}

// The uniform arrays are indexed only inside the constant-bounded loops.
// `rs` scales the corner radius (lens normals; 1 = the outline).
//
// Continuous corners: with a zone factor `zf` > 1 the corner curve
// starts zf x r from the corner (capped at half the shorter side), with
// the superellipse exponent that puts it through the circular arc's 45
// degree point, so it becomes the circle (n = 2) when the zone clamps to
// r (pills). A per-shape exponent (info.z > 0: capsules, circles) keeps
// the plain arc.
fn shapeOf(p: vec2<f32>, rc: vec4<f32>, info: vec4<f32>, rs: f32, zf: f32) -> f32 {
    let hs = rc.zw * 0.5;
    let hm = min(hs.x, hs.y);
    var r = min(info.x * rs, hm);
    var n = select(u.u_global2.y, info.z, info.z > 0.0);
    if (info.z <= 0.0 && zf > 1.0 && r > 0.0) {
        let zone = min(zf * r, hm);
        if (zone > r) {
            n = -1.0 / log2(clamp(1.0 - 0.29289322 * r / zone, 0.5, 0.9999));
            r = zone;
        } else {
            n = 2.0;
        }
    }
    return sdSuperellipseBox(p - (rc.xy + hs), hs, r, n);
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    if (k <= 0.0) {
        return min(a, b);
    }
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

// Lens field: the smooth union of the shapes with each corner radius x
// the normal radius scale (capped at half the shorter side by
// sdSuperellipseBox), with circular corners (no continuous-corner zone:
// the zone measured worse on the lens field). Its gradient gives the
// lens normals.
fn lensField(p: vec2<f32>) -> f32 {
    var d = 1e6;
    var dg = 1e6;
    var g = -1.0;
    for (var i = 0; i < 24; i = i + 1) {
        if (i >= i32(u.u_global.x)) {
            break;
        }
        let info = u.u_info[i];
        let ex = u.u_extra[i];
        if (ex.x != g) {
            d = min(d, dg);
            dg = 1e6;
            g = ex.x;
        }
        let rs = u.u_var[5].w;
        dg = smin(dg, shapeOf(p, u.u_rects[i], info, rs, 1.0), ex.y);
    }
    return min(d, dg);
}

// Tone LUT: 9 grey output knots at inputs i/8, hat-sum form (no dynamic
// array indexing, so it is safe everywhere: the knots travel as two
// vec4s and a float). Identity knots reproduce c. Used by the main tone
// stage (G, H, I.x) and the small-shape tone curve (M, N, O.x).
fn toneLut(c: vec3<f32>, a: vec4<f32>, b: vec4<f32>, k8: f32) -> vec3<f32> {
    let c8 = c * 8.0;
    return vec3<f32>(a.x)
        + (vec3<f32>(a.y) - vec3<f32>(a.x)) * clamp(c8, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(a.z) - vec3<f32>(a.y)) * clamp(c8 - 1.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(a.w) - vec3<f32>(a.z)) * clamp(c8 - 2.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.x) - vec3<f32>(a.w)) * clamp(c8 - 3.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.y) - vec3<f32>(b.x)) * clamp(c8 - 4.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.z) - vec3<f32>(b.y)) * clamp(c8 - 5.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.w) - vec3<f32>(b.z)) * clamp(c8 - 6.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(k8) - vec3<f32>(b.w)) * clamp(c8 - 7.0, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Spectral refraction is independent of the lens material: the lens
// supplies one neutral refraction offset; this optional sampler widens
// that offset once per wavelength and recombines the taps. The rainbow
// therefore modulates refraction, but is not a glass-material stage.
fn spectralLobe(x: f32, m: f32, s1: f32, s2: f32) -> f32 {
    let t = (x - m) * select(s1, s2, x < m);
    return exp(-0.5 * t * t);
}

fn spectralWeight(lambda: f32) -> vec3<f32> {
    let l = lambda;
    let x = 0.362 * spectralLobe(l, 442.0, 0.0624, 0.0374)
        + 1.056 * spectralLobe(l, 599.8, 0.0264, 0.0323)
        - 0.065 * spectralLobe(l, 501.1, 0.049, 0.0382);
    let y = 0.821 * spectralLobe(l, 568.8, 0.0213, 0.0247)
        + 0.286 * spectralLobe(l, 530.9, 0.0613, 0.0322);
    let z = 1.217 * spectralLobe(l, 437.0, 0.0845, 0.0278)
        + 0.681 * spectralLobe(l, 459.0, 0.0385, 0.0725);
    return max(
        vec3<f32>(
            3.2406 * x - 1.5372 * y - 0.4986 * z,
            -0.9689 * x + 1.8752 * y + 0.0415 * z,
            0.0557 * x - 0.2040 * y + 1.0570 * z
        ),
        vec3<f32>(0.0)
    );
}

fn spectralPhase(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// The CIE spectrum weights are fitted in linear light, matching the
// faithful reference. Convert this display-referred sampler's taps to
// linear before recombination, then return the blended refraction to the
// pipeline's encoded colour. Monochrome glass above and below is untouched.
fn spectralToLinear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn spectralFromLinear(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// Voice glow: the emissive wash along the anchor edge (faithful
// `glowField`: voice lobes, Gaussian in both axes, sampled at the
// geometric pixel unless the refracted flag asks for the lens-displaced
// point. The gradient lives
// in userland (`Glow`: stops, anchor, span, thickness); the CPU resolves
// it to these uniform lobes at pack time and the shader only evaluates
// them — no palette or geometry is hardcoded. Interior only: the halo
// outside the edge is `Refraction`, a separate layer-owned concept.
//
// Display-referred adaptation: the reference adds (`bgFetch + glow·inside`)
// and survives because its backdrops sit at midtones and a filmic tonemap
// compresses highlights downstream — neither holds here (white field,
// hard clamp), so addition clips to invisible white. Instead chromaticity
// and intensity are separated: the returned rgb is the normalized weighted
// lobe mean (≤1, unshaped — saturation as a glow knob is deleted) and the
// alpha is the envelope × gain.
//
// Geometry mirrors the reference: the lobe line sits just outside the
// edge (`u_glow_geom.x`), cross-sigma ~0.17× the cross extent, lobe
// sigma ~1.6× the lobe spacing, amplitudes carry the `exp(-nj²/0.32)`
// edge falloff (see `Glow::resolve`).
fn glowOverlay(p: vec2<f32>, gain: f32) -> vec4<f32> {
    let along = p.x;
    let cross = (p.y - u.u_glow_geom.x) / max(u.u_glow_geom.y, 1.0);
    let cross2 = cross * cross;
    var num = vec3<f32>(0.0);
    var den = 0.0;
    for (var j = 0; j < 6; j = j + 1) {
        let L = u.u_glow_lobes[j];
        let d = (along - L.x) / max(L.y, 1.0);
        let wgt = L.z * exp(-d * d - cross2);
        num += u.u_glow_cols[j].rgb * wgt;
        den += wgt;
    }
    let rgb = num / max(den, 1e-4);
    return vec4<f32>(rgb, clamp(den * gain, 0.0, 1.0));
}

// The interior wash: the lens-independent emissive evaluation — geometric
// pixel, no bent or mirrored sampling. Alpha-weighted vertical taps blend
// neighbouring lobes into one continuous wash instead of separated plumes.
fn glowWash(p: vec2<f32>, gain: f32) -> vec4<f32> {
    let off = max(u.u_glow_geom.y, 1.0) * 0.75;
    let s0 = glowOverlay(p, gain);
    let s1 = glowOverlay(p + vec2<f32>(0.0, off), gain);
    let s2 = glowOverlay(p - vec2<f32>(0.0, off), gain);
    let asum = max(s0.a + s1.a + s2.a, 1e-4);
    let rgb = (s0.rgb * s0.a + s1.rgb * s1.a + s2.rgb * s2.a) / asum;
    return vec4<f32>(rgb, asum / 3.0);
}

// The largest smin weight h = max(k - |a - b|, 0) / k met while folding
// the lens field at p: 0 where no shape's union is smoothed, i.e.
// outside every merge neck (the fold starts at 1e6, so a single shape
// always has blend 0). Only the shapes of group [grp] count.
fn lensBlend(p: vec2<f32>, grp: f32) -> f32 {
    var d = 1e6;
    var hmax = 0.0;
    for (var i = 0; i < 24; i = i + 1) {
        if (i >= i32(u.u_global.x)) {
            break;
        }
        let ex = u.u_extra[i];
        if (ex.x != grp) {
            continue;
        }
        let info = u.u_info[i];
        let rs = u.u_var[5].w;
        let di = shapeOf(p, u.u_rects[i], info, rs, 1.0);
        if (ex.y > 0.0) {
            hmax = max(hmax, max(ex.y - abs(d - di), 0.0) / ex.y);
        }
        d = smin(d, di, ex.y);
    }
    return hmax;
}

// Card blend (CSS `mix-blend-mode` on the whole glass card): the
// finished card — refracted backdrop and material baked in,
// premultiplied — composited against the *sharp* snapshot at this
// pixel through §"Compositing and Blending" of CSS Compositing Level 1
// (same formula as `composite_blend.wgsl`). The mode id rides the
// engine header's w slot (`GlassFrameGpu.core[3]`); 0 (`Normal`)
// returns the card unchanged — the exact pre-blend bytes.
fn cardBlend(cardPrem: vec3<f32>, cardA: f32, px: vec2<f32>) -> vec4<f32> {
    let mode = i32(u.engine.w);
    if (mode == 0) {
        return vec4<f32>(cardPrem, cardA);
    }
    var cs = vec3<f32>(0.0);
    if (cardA > 0.0) {
        cs = cardPrem * (1.0 / max(cardA, 1e-6));
    }
    let bd = textureSampleLevel(sharp, samp, px / u.size.xy, 0.0);
    let ab = bd.a;
    var cb = vec3<f32>(0.0);
    if (ab > 0.0) {
        cb = bd.rgb * (1.0 / ab);
    }
    let b = blendMode(cb, cs, mode);
    let co = cardA * (1.0 - ab) * cs + cardA * ab * b + (1.0 - cardA) * ab * cb;
    let ao = cardA + ab * (1.0 - cardA);
    return vec4<f32>(co, ao);
}

// ---------------------------------------------------------------------------
// Effect stages: `fs_main` below is purely an orchestrator — silhouette,
// then one function per material stage in pipeline order. Every stage
// takes a `GlassCtx` and returns it; the math inside each stage is
// verbatim what the monolithic `fs_main` did (same taps, same order),
// so reordering stages is a one-line change in `fs_main` and custom
// effects can replace a single stage without forking the file.
//
// Uniform banks are read directly from `u` inside each stage
// (`u.u_var[N]`); the context carries only per-pixel varyings.
// ---------------------------------------------------------------------------

/// Per-pixel state threaded through the glass effect stages. Mirrors the
/// shared `EffectCtx` ABI (`effect_abi.wgsl`): `col` is the working
/// colour, `px` the device pixel, `depth` the SDF distance, `nrm` the lens
/// normal, `coverage` the clip edge alpha. The remaining fields are the
/// lens/frost intermediates the stages hand to each other.
struct GlassCtx {
    px: vec2<f32>,
    d: f32,
    depth: f32,
    inside: f32,
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
    nrm: vec2<f32>,
    gradLen: f32,
    lensAmt: f32,
    dLens: f32,
    vv: f32,
    sp: vec2<f32>,
    base: vec3<f32>,
    col: vec3<f32>,
    sw: f32,
};

/// Silhouette + attribute blending: the SDF union, the owning group's
/// smoothing/clip/opacity, and the proximity-blended per-shape attributes.
/// Returns `keep <= 0` when the pixel is clipped out (caller early-outs).
fn glassSilhouette(px: vec2<f32>, count: i32) -> GlassCtx {
    var g: GlassCtx;
    g.px = px;
    g.count = count;
    // Per-shape attributes blended by proximity (for merged regions).
    // Weights are shifted by the nearest distance (online softmax) so the
    // nearest shape always weighs 1: raw exp(-dist/k) underflows to 0
    // beyond ~87 px and wsum = 0 turned every attribute into NaN. The
    // nearest shape's group owns the pixel (its smoothing, clip and
    // opacity); the outline is the union of every group's smooth union.
    var d = 1e6;
    var dg = 1e6;
    var grp = -1.0;
    var dOwn = 1e6;
    var own = vec4<f32>(0.0, 0.0, 1.0, 0.0);
    var ownClip = vec4<f32>(-1e9, -1e9, 1e9, 1e9);
    for (var i = 0; i < 24; i = i + 1) {
        if (i >= count) {
            break;
        }
        let ex = u.u_extra[i];
        if (ex.x != grp) {
            d = min(d, dg);
            dg = 1e6;
            grp = ex.x;
        }
        let di = shapeOf(px, u.u_rects[i], u.u_info[i], 1.0, u.u_global2.w);
        dg = smin(dg, di, ex.y);
        if (di < dOwn) {
            dOwn = di;
            own = ex;
            ownClip = u.u_clip[i];
        }
    }
    d = min(d, dg);
    g.d = d;
    g.depth = -d;
    g.ownK = own.y;
    g.ownGrp = own.x;
    g.keep = own.z
        * step(ownClip.x, px.x) * step(ownClip.y, px.y)
        * step(px.x, ownClip.z) * step(px.y, ownClip.w);
    g.inside = 1.0 - smoothstep(-0.75, 0.75, d);

    var wsum = 0.0;
    g.halfMin = 0.0;  // half the shorter side, for the lens size factor
    g.fillScale = 0.0;  // per-shape fillOpacity factor (fill size scaling)
    g.light = 0.0;  // per-shape press light (the pressed lift)
    g.tint = vec4<f32>(0.0);
    g.rcB = vec4<f32>(0.0);  // attribute-blended rect, for the ambient colour
    g.xb = vec4<f32>(0.0);  // extra frost (uBlur)
    let wk = g.ownK * 0.5 + 1.0;
    var dmin = 1e6;
    for (var i = 0; i < 24; i = i + 1) {
        if (i >= count) {
            break;
        }
        if (u.u_extra[i].x != g.ownGrp) {
            continue;
        }
        let info = u.u_info[i];
        let di = shapeOf(px, u.u_rects[i], info, 1.0, u.u_global2.w);
        let e = max(di, 0.0);
        if (e < dmin) {
            let s = exp(-(dmin - e) / wk);
            wsum *= s;
            g.halfMin *= s;
            g.fillScale *= s;
            g.light *= s;
            g.tint *= s;
            g.rcB *= s;
            g.xb *= s;
            dmin = e;
        }
        let w = exp(-(e - dmin) / wk);
        wsum += w;
        g.halfMin += w * 0.5 * min(u.u_rects[i].z, u.u_rects[i].w);
        g.fillScale += w * info.w;
        g.light += w * u.u_extra[i].w;
        g.tint += w * u.u_tints[i];
        g.rcB += w * u.u_rects[i];
        g.xb += w * u.u_blur[i];
    }
    g.halfMin /= wsum;
    g.fillScale /= wsum;
    g.light /= wsum;
    g.tint /= wsum;
    g.rcB /= wsum;
    g.xb /= wsum;
    g.sw = 0.0;
    g.col = vec3<f32>(0.0);
    return g;
}

/// Refraction: lens-field gradient → normals → merge-neck scaling →
/// exponential lens displacement + edge term. Sets `sp` (the displaced
/// sample point), `nrm`, `lensAmt`, `dLens`, `vv`.
fn stageRefract(g: GlassCtx) -> GlassCtx {
    var o = g;
    let A = u.u_var[0];
    let I = u.u_var[8];
    let C = u.u_var[2];
    let hc = u.u_global2.z;
    o.fxp = lensField(o.px + vec2<f32>(1.0, 0.0));
    o.fxm = lensField(o.px - vec2<f32>(1.0, 0.0));
    o.fyp = lensField(o.px + vec2<f32>(0.0, 1.0));
    o.fym = lensField(o.px - vec2<f32>(0.0, 1.0));
    let grad = vec2<f32>(o.fxp - o.fxm, o.fyp - o.fym) + vec2<f32>(1e-6);
    o.gradLen = length(grad);
    o.nrm = grad / o.gradLen;

    // Merge necks: where the lens field's smooth union is active (smin
    // weight > 0) the lens treats the blob as one surface, after the
    // reference's blended geometry: depth is the field over its gradient
    // length (a true distance) and the displacement is scaled by that
    // length, so it fades to 0 at the neck's saddle instead of flipping
    // sides (the crease a unit normal draws). Outside merge necks neck = 1
    // and the lens is unchanged.
    var neck = 1.0;
    if (o.count > 1 && o.ownK > 0.0 && lensBlend(o.px, o.ownGrp) > 0.0) {
        neck = clamp(0.5 * o.gradLen, 0.05, 1.0);
    }

    // Lens v3: the displacement along the normal falls off exponentially
    // with depth (length `decay`) and is cut to 0 at `band`. At the edge
    // it is strength x band (about -47 pt: the outer pixels sample deep
    // inside, so the band shows a mirrored, compressed copy of the
    // interior). Shapes with a half shorter side below the size ref get a
    // uniformly scaled-down lens.
    let ldepth = o.depth / neck;
    let band = max(A.y, 1.0);
    let decay = max(A.x, 1e-3);
    let sc = select(1.0, min(1.0, o.halfMin / C.w), C.w > 0.0);
    let cut = exp(-band / decay);
    let ls = max(decay * sc, 1e-3);
    let ex = exp(-max(ldepth, 0.0) / ls);
    let v = max(ex - cut, 0.0) / max(1.0 - cut, 1e-6);
    let lensK = A.z * (1.0 - 0.5 * hc) * band * sc;
    o.lensAmt = lensK * v;
    // d(lensAmt)/d(depth), for the post-lens blur's Jacobian.
    o.dLens = select(0.0, -lensK * ex / ls / max(1.0 - cut, 1e-6), v > 0.0);
    // Lens edge term: an extra inward offset I.y x exp(-depth / I.z).
    let le = I.y * exp(-max(ldepth, 0.0) / max(I.z, 1e-3));
    o.lensAmt -= le;
    o.dLens += le / max(I.z, 1e-3);
    // d/ddepth of neck x f(depth / neck) is f'(depth / neck): dLens stays.
    o.lensAmt *= neck;
    o.vv = v;
    o.sp = o.px + o.nrm * o.lensAmt;
    return o;
}

/// Frost: core lens sample, post-lens Jacobian blur, extra frost taps,
/// and the wide two-ring tail. Sets `base` and `col`.
fn stageFrost(g: GlassCtx) -> GlassCtx {
    var o = g;
    let E = u.u_var[4];
    let F = u.u_var[5];
    let K = u.u_var[10];
    let O = u.u_var[26];
    // Frost v2: a sharp core plus a wide tail, (1 - w) G(core) + w G(core
    // + wide). The texture is already blurred by the core sigma; the tail
    // is 16 taps of it on two rings (6 + 10; the 2-node Gauss-Laguerre
    // rule for a 2-D Gaussian of sigma E.x), so the input is low-passed
    // and the sparse kernel does not alias. w follows the depth of the
    // point the lens samples relative to the half shorter side, minus a
    // size term for small shapes.
    // Post-lens blur: part of the frost applies after refraction. The
    // composed blur is sigma x sqrt(1 - share); the rest, per variant
    // K.z = sigma x sqrt(share) (x the frost size scale when K.w > 0), is
    // a 3x3 Gauss-Hermite rule in screen space mapped through the lens's
    // Jacobian: 1 - dL/ddepth along the normal, 1 + L x curvature along
    // the tangent (curvature of the lens field's level set, Laplacian /
    // |gradient|). Skipped below 0.25 px. The centre tap is the plain
    // lens sample `base`, reused by dispersion.
    o.base = tex(o.sp).rgb;
    o.col = o.base;
    let sPost = K.z * select(1.0, min(1.0, o.halfMin / K.w), K.w > 0.0);
    if (sPost >= 0.25) {
        // The lens field equals the outline's field when no variant
        // rounds it and the outline has no continuous-corner zone.
        let fc = select(
            lensField(o.px),
            o.d,
            u.u_var[5].w == 1.0 && u.u_var[17].w == 1.0 && u.u_global2.w <= 1.0,
        );
        let lap = o.fxp + o.fxm + o.fyp + o.fym - 4.0 * fc;
        let kappa = lap / max(0.5 * o.gradLen, 1e-3);
        // The post-lens taps stretch at most O.w (postJacobianMax).
        let jm = max(O.w, 1e-3);
        let ja = clamp(1.0 - o.dLens, -jm, jm) * sPost;
        let jb = clamp(1.0 + o.lensAmt * kappa, -jm, jm) * sPost;
        let tng = vec2<f32>(-o.nrm.y, o.nrm.x);
        o.col = o.base * 0.44444445;
        for (var i = 0; i < 3; i = i + 1) {
            let on = f32(i - 1) * 1.7320508;
            let wn = select(0.1666667, 0.6666667, i == 1);
            for (var j = 0; j < 3; j = j + 1) {
                if (i == 1 && j == 1) {
                    continue;
                }
                let ot = f32(j - 1) * 1.7320508;
                let wt = select(0.1666667, 0.6666667, j == 1);
                o.col = o.col + (wn * wt) * tex(o.sp + o.nrm * (ja * on) + tng * (jb * ot)).rgb;
            }
        }
    }
    // Extra frost (uBlur): equal-weight samples of a 2-D Gaussian
    // (Rayleigh radii, golden-angle turns) around the lens sample, added
    // as a difference so the post-lens blur and dispersion keep their own
    // taps. The input is already blurred, so the sparse taps do not alias
    // much: 16-32 taps are within about 1-3 of 255 of an exact blur.
    var xr2 = 0.0;
    if (o.xb.z >= 1.0 && max(o.xb.x, o.xb.y) >= 0.25) {
        let n = i32(o.xb.z + 0.5);
        var acc = vec3<f32>(0.0);
        var dir = vec2<f32>(1.0, 0.0);
        // cos and sin of the golden angle.
        let turn = mat2x2<f32>(
            vec2<f32>(-0.7373688, 0.6754903),
            vec2<f32>(-0.6754903, -0.7373688),
        );
        for (var i = 0; i < 32; i = i + 1) {
            if (i >= n) {
                break;
            }
            let r = sqrt(-2.0 * log(1.0 - (f32(i) + 0.5) / f32(n)));
            acc += tex(o.sp + dir * o.xb.xy * r).rgb;
            dir = turn * dir;
        }
        o.col = o.col + acc / f32(n) - o.base;
        xr2 = o.xb.x * o.xb.y;
    }
    let hmw = max(o.halfMin, 1.0);
    var wideW = E.y + (E.z - E.y) * (o.depth - o.lensAmt) / hmw;
    if (E.w > 0.0) {
        wideW -= F.x * max(0.0, 1.0 - hmw / E.w);
    }
    wideW = select(0.0, clamp(wideW, 0.0, 1.0), E.x > 0.0);
    if (wideW > 0.0) {
        // The tail is core + wide; the input carries only part of the core.
        let wsig = sqrt(E.x * E.x + xr2);
        var ring1 = vec3<f32>(0.0);
        for (var i = 0; i < 6; i = i + 1) {
            let a = f32(i) * 1.0471976;
            ring1 += tex(o.sp + (wsig * 1.0823922) * vec2<f32>(cos(a), sin(a))).rgb;
        }
        var ring2 = vec3<f32>(0.0);
        for (var i = 0; i < 10; i = i + 1) {
            let a = 0.31415927 + f32(i) * 0.62831853;
            ring2 += tex(o.sp + (wsig * 2.6131259) * vec2<f32>(cos(a), sin(a))).rgb;
        }
        let wide = ring1 * (0.8535534 / 6.0) + ring2 * (0.1464466 / 10.0);
        o.col = mix(o.col, wide, wideW);
    }
    return o;
}

/// Spectral refraction: widens the lens sample once per wavelength.
/// Spread/reach 0 is an exact no-op.
fn stageSpectral(g: GlassCtx) -> GlassCtx {
    var o = g;
    // Spectral refraction widens the lens sample once per wavelength.
    // The spectral taps carry the core channel's full frost offset, and
    // the spectrum weights are normalized, so spread/reach 0 is an exact
    // no-op: the single-tap frosted path above is used unchanged, with no
    // extra backdrop taps. A grey backdrop stays grey because the offset
    // is measured after the wide mix, not before it.
    let Q = u.u_var[32];
    if (Q.y != 0.0 && Q.z != 0.0) {
        let baseLinear = spectralToLinear(o.base);
        let coreOff = spectralToLinear(o.col) - baseLinear;
        let iorBase = max(Q.x, 1.0);
        let bendBase = max(1.0 - 1.0 / iorBase, 1e-4);
        let reach = max(Q.z, 0.0);
        let phase = spectralPhase(o.px);
        var acc = vec3<f32>(0.0);
        var wsum = vec3<f32>(0.0);
        for (var k = 0; k < 8; k = k + 1) {
            let frac = (f32(k) + phase) / 8.0;
            let lambda = 405.0 + 290.0 * frac;
            let um = lambda * 0.001;
            let iorWave = iorBase + Q.y * (1.0 / (um * um) - 2.8962) / 1.91;
            let bendWave = 1.0 - 1.0 / max(iorWave, 1.0);
            let spoke = o.nrm * o.lensAmt * (reach * (bendWave / bendBase - 1.0));
            let w = spectralWeight(lambda);
            acc = acc + (spectralToLinear(tex(o.sp + spoke).rgb) + coreOff) * w;
            wsum = wsum + w;
        }
        o.col = mix(o.col, spectralFromLinear(acc / max(wsum, vec3<f32>(1e-4))), o.vv);
    }
    return o;
}

/// Tone: saturation, tone LUT (+ luma-keyed), small-shape shadow lift,
/// fill wash, dim, small-shape tone curve, tint.
fn stageTone(g: GlassCtx) -> GlassCtx {
    var o = g;
    let B = u.u_var[1];
    let D = u.u_var[3];
    let G = u.u_var[6];
    let H = u.u_var[7];
    let I = u.u_var[8];
    let K = u.u_var[10];
    let M = u.u_var[24];
    let N = u.u_var[25];
    let O = u.u_var[26];
    let P = u.u_var[30];
    let luma = dot(o.col, vec3<f32>(0.2126, 0.7152, 0.0722));
    o.col = mix(vec3<f32>(luma), o.col, D.w);

    // Tone LUT: 9 grey knots at inputs i/8, hat-sum form, on the frosted
    // backdrop before the fill wash; identity knots reproduce col.
    // Luma-keyed tone: P.z of the way toward moving luma through the LUT
    // and carrying the chroma along unchanged.
    var toned = toneLut(o.col, G, H, I.x);
    if (P.z != 0.0) {
        let y = dot(o.col, vec3<f32>(0.2126, 0.7152, 0.0722));
        let keyed = o.col + vec3<f32>(toneLut(vec3<f32>(y), G, H, I.x).x - y);
        toned = mix(toned, keyed, P.z);
    }
    o.col = toned;

    // Small-shape shadow lift: the dark end lifts behind small dark glass.
    // + I.w x size x max(0, 1 - col / K.x)^2, size = max(0, 1 - halfMin /
    // K.y).
    let liftSz = select(
        0.0,
        max(0.0, 1.0 - o.halfMin / max(K.y, 1e-3)),
        K.y > 0.0,
    );
    let lk = max(vec3<f32>(1.0) - o.col / max(K.x, 1e-3), vec3<f32>(0.0));
    o.col += (I.w * liftSz) * lk * lk;

    o.col = mix(o.col, D.rgb, B.z * o.fillScale);
    o.col *= (1.0 - B.w);

    // Small-shape tone curve: small shapes (half shorter side <= ~32 pt)
    // switch to a steeper tone response. Applied after the fill wash and
    // dim, weighted from 1 at O.y (smallSizeLo) to 0 at O.z (smallSizeHi);
    // off when smallSizeHi <= 0.
    o.sw = 0.0;
    if (O.z > 0.0) {
        o.sw = clamp((O.z - o.halfMin) / max(O.z - O.y, 1e-3), 0.0, 1.0);
        o.col = mix(o.col, toneLut(o.col, M, N, O.x), o.sw);
    }

    o.col = mix(o.col, o.tint.rgb, o.tint.a);
    return o;
}

/// Ambient: residual backdrop hue where the frosted sample went grey.
fn stageAmbient(g: GlassCtx) -> GlassCtx {
    var o = g;
    let P = u.u_var[30];
    // Ambient colour: the residual sat where our frosted backdrop is grey
    // but the glass carries the backdrop's average hue: + P.x x (avg -
    // luma(avg)), avg = 5 x 5 taps of the blurred texture over the
    // blended rect inflated by P.y px; off in the small-shape class (x (1
    // - sw)). After the tint mix: tinted glass keeps the full ambient
    // chroma. Skipped when 0.
    let am = P.x * (1.0 - o.sw);
    if (am != 0.0) {
        let a0 = o.rcB.xy - vec2<f32>(P.y);
        let asz = o.rcB.zw + vec2<f32>(2.0 * P.y);
        var acc = vec3<f32>(0.0);
        for (var i = 0; i < 5; i = i + 1) {
            for (var j = 0; j < 5; j = j + 1) {
                acc += tex(a0 + asz * (vec2<f32>(f32(i), f32(j)) + 0.5) * 0.2).rgb;
            }
        }
        acc *= 0.04;
        o.col += am * (acc - vec3<f32>(dot(acc, vec3<f32>(0.2126, 0.7152, 0.0722))));
    }
    return o;
}

/// Voice glow inside the slab, composited over the glass base.
fn stageGlow(g: GlassCtx) -> GlassCtx {
    var o = g;
    let Q = u.u_var[32];
    let R4 = u.u_var[34];
    // Voice glow inside the slab: the shared wash (`glowWash`) composited
    // over the glass base. Reference behaviour samples it at the
    // geometric pixel, unaffected by the lens; the opt-in deviation
    // (`Glow::refracted`, carried in `u_glow_geom.z`) samples it at the
    // lens-displaced point, so the wash bends with the refraction like
    // light inside the glass. The wash composites at `Glow::blend`
    // (`u_glow_geom.w`): Normal (0) is the plain source-over `mix`
    // (blendMode returns src → mix(col, src, ov.a)), the other modes
    // tone the base like ink on glass. Before the rim lights, which stay
    // white.
    let glowGain = Q.w;
    let glowIn = R4.x;
    if (glowGain > 0.0) {
        let gp = select(o.px, o.sp, u.u_glow_geom.z > 0.5);
        let ov = glowWash(gp, glowGain * glowIn);
        o.col = mix(o.col, blendMode(o.col, ov.rgb, i32(u.u_glow_geom.w)), ov.a);
    }
    return o;
}

/// Rim lights: specular streak plus isotropic white mix.
fn stageRim(g: GlassCtx) -> GlassCtx {
    var o = g;
    let B = u.u_var[1];
    let J = u.u_var[9];
    let L4 = u.u_var[11];
    let hc = u.u_global2.z;
    let L = vec2<f32>(cos(u.u_global.z), sin(u.u_global.z));
    let rimW = max(B.x * (1.0 + hc), 0.5);
    let rim = 1.0 - smoothstep(0.0, rimW, o.depth);
    let spec = rim * (max(dot(o.nrm, L), 0.0) + L4.x * max(dot(o.nrm, -L), 0.0));
    o.col += B.y * (1.0 + hc) * spec;

    // Isotropic rim: a mix toward white, alpha J.x ramping to 0 at depth
    // J.y, cut at J.z; scaled by the glass luminance down to J.w (dark
    // mode).
    let ra = J.x * clamp(1.0 - o.depth / max(J.y, 1e-3), 0.0, 1.0)
        * (1.0 - smoothstep(J.z - 0.5, J.z + 0.5, o.depth));
    let rl = dot(o.col, vec3<f32>(0.2126, 0.7152, 0.0722));
    let ra0 = ra * (J.w + (1.0 - J.w) * clamp(rl / 0.5, 0.0, 1.0));
    o.col += (vec3<f32>(1.0) - o.col) * ra0;
    return o;
}

/// Pressed-state lighting: whole-shape lift plus touch Gaussian.
fn stagePress(g: GlassCtx) -> GlassCtx {
    var o = g;
    let F = u.u_var[5];
    let R4 = u.u_var[34];
    // Pressed glass lights the whole shape (R.z) and, more, the area
    // under the finger (the Gaussian below); both follow the press light,
    // not the press spring.
    o.col += R4.z * o.light;
    if (u.u_touch.z > 0.0) {
        let dt = o.px - u.u_touch.xy;
        let gr = max(u.u_touch.w, 1.0);
        o.col += F.y * u.u_touch.z * exp(-dot(dt, dt) / (2.0 * gr * gr));
    }
    return o;
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // Output is premultiplied colour composited srcOver onto the sharp
    // backdrop: transparent outside the shape.
    let px = pos.xy;
    let count = i32(u.u_global.x);
    if (count == 0) {
        return vec4<f32>(0.0);
    }

    // Outside the silhouette the optics pass emits nothing: the halo is
    // the renderer's theatre-mode replay (blurred finished card under the
    // sharp one, `refraction_replay`), not shading. A shader-side halo
    // could only re-evaluate the lobe field — a second gradient that
    // diverges from the card by construction — so it was deleted.
    var g = glassSilhouette(px, count);
    if (g.keep <= 0.0) {
        return vec4<f32>(0.0);
    }
    if (g.inside <= 0.0) {
        return vec4<f32>(0.0);
    }

    // Opaque (Reduce Transparency).
    if (u.u_global.w > 0.5) {
        let solid = mix(u.u_opaque.rgb, g.tint.rgb, g.tint.a);
        return cardBlend(solid * (g.inside * g.keep), g.inside * g.keep, px);
    }

    g = stageRefract(g);
    g = stageFrost(g);
    g = stageSpectral(g);
    g = stageTone(g);
    g = stageAmbient(g);
    g = stageGlow(g);
    g = stageRim(g);
    g = stagePress(g);

    return cardBlend(
        clamp(g.col, vec3<f32>(0.0), vec3<f32>(1.0)) * (g.inside * g.keep),
        g.inside * g.keep,
        px,
    );
}
