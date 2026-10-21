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

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // Output is premultiplied colour composited srcOver onto the sharp
    // backdrop: transparent outside the shape.
    let px = pos.xy;
    let count = i32(u.u_global.x);
    if (count == 0) {
        return vec4<f32>(0.0);
    }

    // Per-shape attributes blended by proximity (for merged regions).
    // Weights are shifted by the nearest distance (online softmax) so the
    // nearest shape always weighs 1: raw exp(-dist/k) underflows to 0
    // beyond ~87 px and wsum = 0 turned every attribute into NaN. The
    // nearest shape's group owns the pixel (its smoothing, clip and
    // opacity); the outline is the union of every group's smooth union.
    var d = 1e6;
    var dg = 1e6;
    var g = -1.0;
    var dOwn = 1e6;
    var own = vec4<f32>(0.0, 0.0, 1.0, 0.0);
    var ownClip = vec4<f32>(-1e9, -1e9, 1e9, 1e9);
    for (var i = 0; i < 24; i = i + 1) {
        if (i >= count) {
            break;
        }
        let ex = u.u_extra[i];
        if (ex.x != g) {
            d = min(d, dg);
            dg = 1e6;
            g = ex.x;
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
    let ownK = own.y;
    let keep = own.z
        * step(ownClip.x, px.x) * step(ownClip.y, px.y)
        * step(px.x, ownClip.z) * step(px.y, ownClip.w);
    if (keep <= 0.0) {
        return vec4<f32>(0.0);
    }

    var wsum = 0.0;
    var halfMin = 0.0;  // half the shorter side, for the lens size factor
    var fillScale = 0.0;  // per-shape fillOpacity factor (fill size scaling)
    var light = 0.0;  // per-shape press light (the pressed lift)
    var tint = vec4<f32>(0.0);
    var rcB = vec4<f32>(0.0);  // attribute-blended rect, for the ambient colour
    var xb = vec4<f32>(0.0);  // extra frost (uBlur)
    let wk = ownK * 0.5 + 1.0;
    var dmin = 1e6;
    for (var i = 0; i < 24; i = i + 1) {
        if (i >= count) {
            break;
        }
        if (u.u_extra[i].x != own.x) {
            continue;
        }
        let info = u.u_info[i];
        let di = shapeOf(px, u.u_rects[i], info, 1.0, u.u_global2.w);
        let e = max(di, 0.0);
        if (e < dmin) {
            let s = exp(-(dmin - e) / wk);
            wsum *= s;
            halfMin *= s;
            fillScale *= s;
            light *= s;
            tint *= s;
            rcB *= s;
            xb *= s;
            dmin = e;
        }
        let w = exp(-(e - dmin) / wk);
        wsum += w;
        halfMin += w * 0.5 * min(u.u_rects[i].z, u.u_rects[i].w);
        fillScale += w * info.w;
        light += w * u.u_extra[i].w;
        tint += w * u.u_tints[i];
        rcB += w * u.u_rects[i];
        xb += w * u.u_blur[i];
    }
    halfMin /= wsum;
    fillScale /= wsum;
    light /= wsum;
    tint /= wsum;
    rcB /= wsum;
    xb /= wsum;

    // Single bank: both uniform slots carry the same set (see the
    // packer), so the first bank is read directly. No variant flags,
    // no clearMix — every value on every shape is explicit.
    let A = u.u_var[0];
    let B = u.u_var[1];
    let C = u.u_var[2];
    let D = u.u_var[3];
    let E = u.u_var[4];
    let F = u.u_var[5];
    let G = u.u_var[6];
    let H = u.u_var[7];
    let I = u.u_var[8];
    let J = u.u_var[9];
    let K = u.u_var[10];
    let L4 = u.u_var[11];
    let R4 = u.u_var[34];
    let M = u.u_var[24];
    let N = u.u_var[25];
    let O = u.u_var[26];
    let P = u.u_var[30];
    let Q = u.u_var[32];
    let hc = u.u_global2.z;

    let inside = 1.0 - smoothstep(-0.75, 0.75, d);

    // Outside the silhouette the optics pass emits nothing: the halo is
    // the renderer's theatre-mode replay (blurred finished card under the
    // sharp one, `refraction_replay`), not shading. A shader-side halo
    // could only re-evaluate the lobe field — a second gradient that
    // diverges from the card by construction — so it was deleted.
    let glowGain = Q.w;
    let glowIn = R4.x;
    if (inside <= 0.0) {
        return vec4<f32>(0.0);
    }

    // Opaque (Reduce Transparency).
    if (u.u_global.w > 0.5) {
        let solid = mix(u.u_opaque.rgb, tint.rgb, tint.a);
        return cardBlend(solid * (inside * keep), inside * keep, px);
    }

    let fxp = lensField(px + vec2<f32>(1.0, 0.0));
    let fxm = lensField(px - vec2<f32>(1.0, 0.0));
    let fyp = lensField(px + vec2<f32>(0.0, 1.0));
    let fym = lensField(px - vec2<f32>(0.0, 1.0));
    let grad = vec2<f32>(fxp - fxm, fyp - fym) + vec2<f32>(1e-6);
    let gradLen = length(grad);
    let nrm = grad / gradLen;

    // Merge necks: where the lens field's smooth union is active (smin
    // weight > 0) the lens treats the blob as one surface, after the
    // reference's blended geometry: depth is the field over its gradient
    // length (a true distance) and the displacement is scaled by that
    // length, so it fades to 0 at the neck's saddle instead of flipping
    // sides (the crease a unit normal draws). Outside merge necks neck = 1
    // and the lens is unchanged.
    var neck = 1.0;
    if (count > 1 && ownK > 0.0 && lensBlend(px, own.x) > 0.0) {
        neck = clamp(0.5 * gradLen, 0.05, 1.0);
    }

    // Lens v3: the displacement along the normal falls off exponentially
    // with depth (length `decay`) and is cut to 0 at `band`. At the edge
    // it is strength x band (about -47 pt: the outer pixels sample deep
    // inside, so the band shows a mirrored, compressed copy of the
    // interior). Shapes with a half shorter side below the size ref get a
    // uniformly scaled-down lens.
    let depth = -d;
    let ldepth = depth / neck;
    let band = max(A.y, 1.0);
    let decay = max(A.x, 1e-3);
    let sc = select(1.0, min(1.0, halfMin / C.w), C.w > 0.0);
    let cut = exp(-band / decay);
    let ls = max(decay * sc, 1e-3);
    let ex = exp(-max(ldepth, 0.0) / ls);
    let v = max(ex - cut, 0.0) / max(1.0 - cut, 1e-6);
    let lensK = A.z * (1.0 - 0.5 * hc) * band * sc;
    var lensAmt = lensK * v;
    // d(lensAmt)/d(depth), for the post-lens blur's Jacobian.
    var dLens = select(0.0, -lensK * ex / ls / max(1.0 - cut, 1e-6), v > 0.0);
    // Lens edge term: an extra inward offset I.y x exp(-depth / I.z).
    let le = I.y * exp(-max(ldepth, 0.0) / max(I.z, 1e-3));
    lensAmt -= le;
    dLens += le / max(I.z, 1e-3);
    // d/ddepth of neck x f(depth / neck) is f'(depth / neck): dLens stays.
    lensAmt *= neck;
    var sp = px + nrm * lensAmt;

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
    let base = tex(sp).rgb;
    var col = base;
    let sPost = K.z * select(1.0, min(1.0, halfMin / K.w), K.w > 0.0);
    if (sPost >= 0.25) {
        // The lens field equals the outline's field when no variant
        // rounds it and the outline has no continuous-corner zone.
        let fc = select(
            lensField(px),
            d,
            u.u_var[5].w == 1.0 && u.u_var[17].w == 1.0 && u.u_global2.w <= 1.0,
        );
        let lap = fxp + fxm + fyp + fym - 4.0 * fc;
        let kappa = lap / max(0.5 * gradLen, 1e-3);
        // The post-lens taps stretch at most O.w (postJacobianMax).
        let jm = max(O.w, 1e-3);
        let ja = clamp(1.0 - dLens, -jm, jm) * sPost;
        let jb = clamp(1.0 + lensAmt * kappa, -jm, jm) * sPost;
        let tng = vec2<f32>(-nrm.y, nrm.x);
        col = base * 0.44444445;
        for (var i = 0; i < 3; i = i + 1) {
            let on = f32(i - 1) * 1.7320508;
            let wn = select(0.1666667, 0.6666667, i == 1);
            for (var j = 0; j < 3; j = j + 1) {
                if (i == 1 && j == 1) {
                    continue;
                }
                let ot = f32(j - 1) * 1.7320508;
                let wt = select(0.1666667, 0.6666667, j == 1);
                col = col + (wn * wt) * tex(sp + nrm * (ja * on) + tng * (jb * ot)).rgb;
            }
        }
    }
    // Extra frost (uBlur): equal-weight samples of a 2-D Gaussian
    // (Rayleigh radii, golden-angle turns) around the lens sample, added
    // as a difference so the post-lens blur and dispersion keep their own
    // taps. The input is already blurred, so the sparse taps do not alias
    // much: 16-32 taps are within about 1-3 of 255 of an exact blur.
    var xr2 = 0.0;
    if (xb.z >= 1.0 && max(xb.x, xb.y) >= 0.25) {
        let n = i32(xb.z + 0.5);
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
            acc += tex(sp + dir * xb.xy * r).rgb;
            dir = turn * dir;
        }
        col = col + acc / f32(n) - base;
        xr2 = xb.x * xb.y;
    }
    let hmw = max(halfMin, 1.0);
    var wideW = E.y + (E.z - E.y) * (depth - lensAmt) / hmw;
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
            ring1 += tex(sp + (wsig * 1.0823922) * vec2<f32>(cos(a), sin(a))).rgb;
        }
        var ring2 = vec3<f32>(0.0);
        for (var i = 0; i < 10; i = i + 1) {
            let a = 0.31415927 + f32(i) * 0.62831853;
            ring2 += tex(sp + (wsig * 2.6131259) * vec2<f32>(cos(a), sin(a))).rgb;
        }
        let wide = ring1 * (0.8535534 / 6.0) + ring2 * (0.1464466 / 10.0);
        col = mix(col, wide, wideW);
    }
    // Spectral refraction widens the lens sample once per wavelength.
    // The spectral taps carry the core channel's full frost offset, and
    // the spectrum weights are normalized, so spread/reach 0 is an exact
    // no-op: the single-tap frosted path above is used unchanged, with no
    // extra backdrop taps. A grey backdrop stays grey because the offset
    // is measured after the wide mix, not before it.
    if (Q.y != 0.0 && Q.z != 0.0) {
        let baseLinear = spectralToLinear(base);
        let coreOff = spectralToLinear(col) - baseLinear;
        let iorBase = max(Q.x, 1.0);
        let bendBase = max(1.0 - 1.0 / iorBase, 1e-4);
        let reach = max(Q.z, 0.0);
        let phase = spectralPhase(px);
        var acc = vec3<f32>(0.0);
        var wsum = vec3<f32>(0.0);
        for (var k = 0; k < 8; k = k + 1) {
            let frac = (f32(k) + phase) / 8.0;
            let lambda = 405.0 + 290.0 * frac;
            let um = lambda * 0.001;
            let iorWave = iorBase + Q.y * (1.0 / (um * um) - 2.8962) / 1.91;
            let bendWave = 1.0 - 1.0 / max(iorWave, 1.0);
            let spoke = nrm * lensAmt * (reach * (bendWave / bendBase - 1.0));
            let w = spectralWeight(lambda);
            acc = acc + (spectralToLinear(tex(sp + spoke).rgb) + coreOff) * w;
            wsum = wsum + w;
        }
        col = mix(col, spectralFromLinear(acc / max(wsum, vec3<f32>(1e-4))), v);
    }

    let luma = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    col = mix(vec3<f32>(luma), col, D.w);

    // Tone LUT: 9 grey knots at inputs i/8, hat-sum form, on the frosted
    // backdrop before the fill wash; identity knots reproduce col.
    // Luma-keyed tone: P.z of the way toward moving luma through the LUT
    // and carrying the chroma along unchanged.
    var toned = toneLut(col, G, H, I.x);
    if (P.z != 0.0) {
        let y = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
        let keyed = col + vec3<f32>(toneLut(vec3<f32>(y), G, H, I.x).x - y);
        toned = mix(toned, keyed, P.z);
    }
    col = toned;

    // Small-shape shadow lift: the dark end lifts behind small dark glass.
    // + I.w x size x max(0, 1 - col / K.x)^2, size = max(0, 1 - halfMin /
    // K.y).
    let liftSz = select(
        0.0,
        max(0.0, 1.0 - halfMin / max(K.y, 1e-3)),
        K.y > 0.0,
    );
    let lk = max(vec3<f32>(1.0) - col / max(K.x, 1e-3), vec3<f32>(0.0));
    col += (I.w * liftSz) * lk * lk;

    col = mix(col, D.rgb, B.z * fillScale);
    col *= (1.0 - B.w);

    // Small-shape tone curve: small shapes (half shorter side <= ~32 pt)
    // switch to a steeper tone response. Applied after the fill wash and
    // dim, weighted from 1 at O.y (smallSizeLo) to 0 at O.z (smallSizeHi);
    // off when smallSizeHi <= 0.
    var sw = 0.0;
    if (O.z > 0.0) {
        sw = clamp((O.z - halfMin) / max(O.z - O.y, 1e-3), 0.0, 1.0);
        col = mix(col, toneLut(col, M, N, O.x), sw);
    }

    col = mix(col, tint.rgb, tint.a);

    // Ambient colour: the residual sat where our frosted backdrop is grey
    // but the glass carries the backdrop's average hue: + P.x x (avg -
    // luma(avg)), avg = 5 x 5 taps of the blurred texture over the
    // blended rect inflated by P.y px; off in the small-shape class (x (1
    // - sw)). After the tint mix: tinted glass keeps the full ambient
    // chroma. Skipped when 0.
    let am = P.x * (1.0 - sw);
    if (am != 0.0) {
        let a0 = rcB.xy - vec2<f32>(P.y);
        let asz = rcB.zw + vec2<f32>(2.0 * P.y);
        var acc = vec3<f32>(0.0);
        for (var i = 0; i < 5; i = i + 1) {
            for (var j = 0; j < 5; j = j + 1) {
                acc += tex(a0 + asz * (vec2<f32>(f32(i), f32(j)) + 0.5) * 0.2).rgb;
            }
        }
        acc *= 0.04;
        col += am * (acc - vec3<f32>(dot(acc, vec3<f32>(0.2126, 0.7152, 0.0722))));
    }

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
    if (glowGain > 0.0) {
        let gp = select(px, sp, u.u_glow_geom.z > 0.5);
        let ov = glowWash(gp, glowGain * glowIn);
        col = mix(col, blendMode(col, ov.rgb, i32(u.u_glow_geom.w)), ov.a);
    }

    let L = vec2<f32>(cos(u.u_global.z), sin(u.u_global.z));
    let rimW = max(B.x * (1.0 + hc), 0.5);
    let rim = 1.0 - smoothstep(0.0, rimW, depth);
    let spec = rim * (max(dot(nrm, L), 0.0) + L4.x * max(dot(nrm, -L), 0.0));
    col += B.y * (1.0 + hc) * spec;

    // Isotropic rim: a mix toward white, alpha J.x ramping to 0 at depth
    // J.y, cut at J.z; scaled by the glass luminance down to J.w (dark
    // mode).
    let ra = J.x * clamp(1.0 - depth / max(J.y, 1e-3), 0.0, 1.0)
        * (1.0 - smoothstep(J.z - 0.5, J.z + 0.5, depth));
    let rl = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    let ra0 = ra * (J.w + (1.0 - J.w) * clamp(rl / 0.5, 0.0, 1.0));
    col += (vec3<f32>(1.0) - col) * ra0;

    // Pressed glass lights the whole shape (R.z) and, more, the area
    // under the finger (the Gaussian below); both follow the press light,
    // not the press spring.
    col += R4.z * light;
    if (u.u_touch.z > 0.0) {
        let dt = px - u.u_touch.xy;
        let gr = max(u.u_touch.w, 1.0);
        col += F.y * u.u_touch.z * exp(-dot(dt, dt) / (2.0 * gr * gr));
    }

    return cardBlend(
        clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)) * (inside * keep),
        inside * keep,
        px,
    );
}
