// Modular glass — the Chain-spec single static shader (slice M1:
// Refract only; the remaining `stage*` functions land with their
// primitives). One card → one 256 B block → one pass, per
// plan-better-glass-setup.md Part 2 (P3) and H5. The orchestrator is a
// `stage_mask` branch — no stage list, no per-composition codegen; the
// WGSL source never changes at runtime. Self-contained: it shares
// nothing with the legacy glass modules (which stay byte-stable —
// this file is additive by design).
//
// The 256 B block mirrors `foundation/shading/primitives.rs` bank
// slots (W_* constants — keep in sync):
//   w0: stage_mask (f32), card w, card h (device px), dpr
//   w1: corner tl, tr, br, bl (device px)
//   w2: Refract A — strength, band px, decay px, ior
//   w3: Refract B — size ref px, lens edge px, edge decay px, normal
//        radius scale
//   w4: card origin x, y (device px), high-contrast, corner exponent
//        (0 = global 2.0)
//   w5–w15: reserved banks (future stages read their numeric slot).
//
// `ShadingCtx` (plan P3, verbatim field list): px, local_px,
// edge_dist, coverage, normal, path_length, sample_point, col — plus
// the Refract intermediates (lensAmt, dLens, vv, n3) the next stages
// consume. Bindings match the legacy glass layout exactly (0 backdrop,
// 1 sampler, 2 uniform, 3 sharp), so both pipelines share one
// `glass_layout` — see `pipeline.rs`.

struct ModularU {
    block: array<vec4<f32>, 16>, // 64 f32 = 256 B
};

@group(0) @binding(0)
var backdrop: texture_2d<f32>;
@group(0) @binding(1)
var samp: sampler;
@group(0) @binding(2)
var<uniform> u: ModularU;
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

/// Backdrop sample at device px, clamped, unpremultiplied — the legacy
/// `tex()` contract (`core.wgsl`), mirrored verbatim.
fn tex(px: vec2<f32>) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(backdrop));
    let uv = clamp(px / dims, vec2<f32>(0.0), vec2<f32>(1.0));
    let s = textureSampleLevel(backdrop, samp, uv, 0.0);
    let a = max(s.a, 1.0 / 255.0);
    return vec4<f32>(s.rgb / a, s.a);
}

// --- Geometry: the one card's SDF (port of `shape.wgsl`, single
// shape; merge necks / proximity blending / clips belong to later
// slices — one card is one SDF by construction) -----------------------

// Per-quadrant superellipse box: `r` carries tl / tr / br / bl
// (device px). Verbatim `sdSuperellipseBox4`.
fn sdSuperellipseBox4(p: vec2<f32>, halfSize: vec2<f32>, r: vec4<f32>, n0: f32) -> f32 {
    let rr = min(r, vec4<f32>(min(halfSize.x, halfSize.y)));
    var rq = rr.x;
    rq = select(rq, rr.y, p.x > 0.0);
    rq = select(rq, rr.z, p.x > 0.0 && p.y > 0.0);
    rq = select(rq, rr.w, p.x <= 0.0 && p.y > 0.0);
    let q = abs(p) - halfSize + rq;
    let m = max(q, vec2<f32>(0.0));
    let corner = pow(pow(m.x, n0) + pow(m.y, n0), 1.0 / n0);
    return corner + min(max(q.x, q.y), 0.0) - rq;
}

// The card resolved from the block: outline SDF with the
// continuous-corner zone (`zf` = the legacy zone factor, 1.2 for the
// outline, 1.0 for the lens field — the lens keeps circular corners
// per the legacy `lensField`), Lens normals scale radii by `rs`.
fn cardOutline(p: vec2<f32>, rs: f32, zf: f32) -> f32 {
    let centre = vec2<f32>(u.block[4].x, u.block[4].y)
        + vec2<f32>(u.block[0].y, u.block[0].z) * 0.5;
    let hs = vec2<f32>(u.block[0].y, u.block[0].z) * 0.5;
    let hm = min(hs.x, hs.y);
    let rv = min(u.block[1] * rs, vec4<f32>(hm));
    let authored_exponent = u.block[4].w;
    let rmin = min(min(rv.x, rv.y), min(rv.z, rv.w));
    let rmax = max(max(rv.x, rv.y), max(rv.z, rv.w));
    let uniform = rmax - rmin <= max(rmax * 1e-3, 1e-3);
    if (authored_exponent > 0.0) {
        // Per-shape exponent path (capsule/circle): one arc radius.
        return sdSuperellipseBox4(p - centre, hs, vec4<f32>(rmin), authored_exponent);
    }
    // Continuous-corner zone on genuinely uniform cards (outline only;
    // lens field zf <= 1 skips it, per legacy).
    var n = 2.0;
    var r = rmin;
    if (zf > 1.0 && r > 0.0 && uniform) {
        let zone = min(zf * r, hm);
        if (zone > r) {
            n = -1.0 / log2(clamp(1.0 - 0.29289322 * r / zone, 0.5, 0.9999));
            r = zone;
        } else {
            n = 2.0;
        }
        return sdSuperellipseBox4(p - centre, hs, vec4<f32>(r), n);
    }
    // Plain arc per corner (asymmetric cards keep the exact quadrant
    // SDF — the outline; the lens field with rs != 1 keeps it too).
    if (zf > 1.0 || rs != 1.0) {
        return sdSuperellipseBox4(p - centre, hs, rv, 2.0);
    }
    // lens field with rs == 1: scalar rmin arc (the legacy lensField
    // reads the corner min through the same shape call).
    return sdSuperellipseBox4(p - centre, hs, vec4<f32>(rmin), 2.0);
}

fn outlineDistance(px: vec2<f32>) -> f32 {
    cardOutline(px, 1.0, 1.2)
}

// Lens field: the card with its corners scaled by the normal radius
// (circular corners), gradient source for the lens normals.
fn lensField(p: vec2<f32>) -> f32 {
    cardOutline(p, u.block[3].w, 1.0)
}

/// Per-pixel state threaded through the stages. Plan P3's field list
/// verbatim where it applies (`edge_dist` is the legacy `depth`):
/// positive distance inside the silhouette.
struct ShadingCtx {
    px: vec2<f32>,
    local_px: vec2<f32>,
    edge_dist: f32,
    coverage: f32,
    normal: vec2<f32>,
    /// 3-D surface normal (flat baseline until Refract bends it).
    n3: vec3<f32>,
    /// Optical path through the glass in device px (0 absent Refract).
    path_length: f32,
    /// The displaced backdrop read position; `px` until Refract runs.
    sample_point: vec2<f32>,
    lensAmt: f32,
    dLens: f32,
    vv: f32,
    halfMin: f32,
    gradLen: f32,
    col: vec3<f32>,
};

// Silhouette of the one card + its baseline context (port of
// `glassSilhouette` degenerate to count = 1 — no groups, no blending,
// no proximity attributes in M1).
fn glassSilhouette(px: vec2<f32>) -> ShadingCtx {
    var g: ShadingCtx;
    g.px = px;
    let rc = vec4<f32>(u.block[4].x, u.block[4].y, u.block[0].y, u.block[0].z);
    g.local_px = px - rc.xy;
    g.halfMin = 0.5 * min(rc.z, rc.w);
    let d = outlineDistance(px);
    g.edge_dist = -d;
    g.coverage = 1.0 - smoothstep(-0.75, 0.75, d);
    g.n3 = vec3<f32>(0.0, 0.0, 1.0);
    g.path_length = 0.0;
    g.sample_point = px;
    g.lensAmt = 0.0;
    g.dLens = 0.0;
    g.vv = 0.0;
    g.col = vec3<f32>(0.0);
    return g;
}

// Lens normals from the lens field's gradient (port of `stageShape`,
// the first-class normal: everything downstream reads it from here).
fn stageShape(g: ShadingCtx) -> ShadingCtx {
    var o = g;
    let fxp = lensField(o.px + vec2<f32>(1.0, 0.0));
    let fxm = lensField(o.px - vec2<f32>(1.0, 0.0));
    let fyp = lensField(o.px + vec2<f32>(0.0, 1.0));
    let fym = lensField(o.px - vec2<f32>(0.0, 1.0));
    let grad = vec2<f32>(fxp - fxm, fyp - fym) + vec2<f32>(1e-6);
    o.gradLen = length(grad);
    o.normal = grad / o.gradLen;
    o.n3 = vec3<f32>(o.normal, 1.0);
    return o;
}

// --- Refract (id 1): port of `refract.wgsl` verbatim against the
// block banks. count = 1 in M1, so the merge neck collapses to 1; HC
// rides w4.z. ----------------------------------------------------------

fn stageRefract(g: ShadingCtx) -> ShadingCtx {
    var o = g;
    let A = u.block[2]; // strength, band, decay, ior
    let I = vec4<f32>(0.0, u.block[3].y, u.block[3].z, 0.0); // lens edge px + decay px in yz
    let C = vec4<f32>(0.0, 0.0, 0.0, u.block[3].x); // lens size ref in w
    let hc = u.block[4].z;

    // Merge necks: single card → the lens is one surface (neck = 1).
    let neck = 1.0;

    // Lens v3 (verbatim): exponential falloff with depth, cut at band,
    // size-scaled below the size ref, edge term on top.
    let ldepth = o.edge_dist / neck;
    let band = max(A.y, 1.0);
    let decay = max(A.z, 1e-3);
    let sc = select(1.0, min(1.0, o.halfMin / C.w), C.w > 0.0);
    let cut = exp(-band / decay);
    let ls = max(decay * sc, 1e-3);
    let ex = exp(-max(ldepth, 0.0) / ls);
    let v = max(ex - cut, 0.0) / max(1.0 - cut, 1e-6);
    let lensAmt = A.x * (1.0 - 0.5 * hc) * band * sc;
    o.lensAmt = lensAmt * v;
    o.dLens = select(0.0, -lensAmt * ex / ls / max(1.0 - cut, 1e-6), v > 0.0);
    // Lens edge term: an extra inward offset I.y x exp(-depth / I.z).
    let le = I.y * exp(-max(ldepth, 0.0) / max(I.z, 1e-3));
    o.lensAmt -= le;
    o.dLens += le / max(I.z, 1e-3);
    o.lensAmt *= neck;
    o.vv = v;
    o.sample_point = o.px + o.normal * o.lensAmt;

    // 3-D normal (verbatim): outward tilt via the Snell lift x3 at
    // ior 1.5 (A.w publishes the authored ior; M1 keeps the fitted
    // default — non-1.5 ior lands with the frost/spectral slices).
    let lift = iorLift(A.w);
    o.n3 = normalize(vec3<f32>(o.normal * (o.dLens * lift), 1.0));
    // Slab: optical travel = bevel-band profile (verbatim).
    o.path_length = v * band;
    return o;
}

// The lens displacement factor: strength x band (HC halves it on the
// lit outline, legacy `lensK`).
fn lensStrength(strength: f32, hc: f32) -> f32 {
    return strength * (1.0 - 0.5 * hc);
}

// Snell lift between the height slope and displacement slope:
// ior/(ior - 1), 3.0 at the fitted ior 1.5 — the reference's x3.
fn iorLift(ior: f32) -> f32 {
    return select(3.0, ior / (ior - 1.0), abs(ior - 1.5) > 1e-4 && ior > 1.0);
}

// --- Frost (id 2): stub reading uniforms through ShadingCtx ---
fn stageFrost(g: ShadingCtx) -> ShadingCtx {
    var o = g;
    let F = u.block[5]; // Frost A: blur px, tail radius px, tail strength, spare
    let _blur = F.x;
    let _tail_radius = F.y;
    let _tail_strength = F.z;
    // Stub: no change to sample path yet, just read uniforms.
    return o;
}

// --- BeerAbsorption (id 4): stub reading uniforms through ShadingCtx ---
fn stageBeer(g: ShadingCtx) -> ShadingCtx {
    var o = g;
    let B = u.block[7]; // Beer: tint r, g, b, density
    let _tint = B.xyz;
    let _density = B.w;
    // Stub: no change yet, just read uniforms.
    return o;
}

// --- Fill (id 8): the purge's one paint primitive. Solid colour at the
// silhouette's coverage — no backdrop read, no lens, no blur. Written
// by `foundation/fill.rs` into w8 (r, g, b, a straight).
fn stageFill(g: ShadingCtx) -> vec4<f32> {
    let f = u.block[8];
    let a = g.coverage * f.a;
    return vec4<f32>(f.rgb * a, a);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = pos.xy;

    // Mask 0 = nothing authored: transparent (a clear block draws
    // nothing — the frame still exists).
    let mask = u32(u.block[0].x);
    if (mask == 0u) {
        return vec4<f32>(0.0);
    }

    var g = glassSilhouette(px);
    if (g.coverage <= 0.0) {
        return vec4<f32>(0.0);
    }

    // Shape first: the normals must exist before anything refracts.
    g = stageShape(g);

    // stage_mask dispatch: M1 ships Refract. The following stages are
    // the reserved banks — each lands with its primitive as one more
    // `if ((mask & STAGE_*) != 0u)` branch.
    if ((mask & 1u) != 0u) {
        g = stageRefract(g);
    }
    if ((mask & 2u) != 0u) {
        g = stageFrost(g);
    }
    if ((mask & 4u) != 0u) {
        g = stageBeer(g);
    }
    // Fill short-circuits: a solid card never reads the backdrop.
    if ((mask & 8u) != 0u) {
        return stageFill(g);
    }
    // Future: RIM (16) — DirectionalRim; GLINT (32) — SpecularGlint;
    // ENV_REFLECT (64) — EnvironmentReflection; TONE_CURVE (128);
    // MOUSE_HIGHLIGHT (256) — HighlightOverMouse.

    // No Refract → the backdrop reads through the un-deflected point.
    let s = tex(g.sample_point);
    g.col = s.rgb;

    // Premultiplied output (the legacy pass contract); the card-blend
    // library rides with the composite stage — Normal is identity.
    let alpha = g.coverage;
    return vec4<f32>(clamp(g.col, vec3<f32>(0.0), vec3<f32>(1.0)), alpha) * vec4<f32>(vec3<f32>(alpha), 1.0);
}
