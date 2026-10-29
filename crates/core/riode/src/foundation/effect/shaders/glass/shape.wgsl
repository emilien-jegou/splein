// Tier B1 — Shape (08.5): geometry, silhouette, and the lens normal as
// a first-class output.
//
// Taxonomy: L19 coverage/mask/AA (the fixed 1.5 px `coverage` ramp) plus
// the cross-cutting shape layer (superellipse SDF, continuous-corner
// zone, per-group smooth union, proximity-blended per-shape attributes).
//
// Why it is its own effect: the normal used to be a by-product of the
// displacement computation (`stageRefract` computed the gradient and
// then immediately bent by it), so nothing downstream could read it
// without re-deriving it. `stageShape` now stops at the normal and
// `refract.wgsl` starts from it — which is what makes Fresnel/GGX
// (Phase 4.1/4.2) addable later without another refactor.
//
// Deliberately NOT split further (08.9): the superellipse, the
// continuous-corner zone, the smooth union and the proximity-blended
// attributes are one geometry computation and stay in one place.

// Per-quadrant superellipse box: `r` carries tl / tr / br / bl
// (device px). The quadrant is picked from the sign of `p` — the
// offset from the rect centre — so exactly one radius shapes each
// corner and straight edges stay exact (the picked radius cancels on
// the axis the SDF is evaluated against). Uniform cards take the
// fast scalar path (identical math to the old single-radius SDF).
fn sdSuperellipseBox4(p: vec2<f32>, halfSize: vec2<f32>, r: vec4<f32>, n0: f32) -> f32 {
    let rr = min(r, vec4<f32>(min(halfSize.x, halfSize.y)));
    // Branchless quadrant pick: +x = right, +y = bottom (device px
    // run y-down; tl = (-x,-y), tr = (+x,-y), br = (+x,+y), bl =
    // (-x,+y)). `select` on a bool is a mix — no dynamic branch.
    var rq = rr.x;
    rq = select(rq, rr.y, p.x > 0.0);
    rq = select(rq, rr.z, p.x > 0.0 && p.y > 0.0);
    rq = select(rq, rr.w, p.x <= 0.0 && p.y > 0.0);
    let q = abs(p) - halfSize + rq;
    let m = max(q, vec2<f32>(0.0));
    let corner = pow(pow(m.x, n0) + pow(m.y, n0), 1.0 / n0);
    return corner + min(max(q.x, q.y), 0.0) - rq;
}

fn sdSuperellipseBox(p: vec2<f32>, halfSize: vec2<f32>, r0: f32, n0: f32) -> f32 {
    return sdSuperellipseBox4(p, halfSize, vec4<f32>(r0), n0);
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
fn shapeOf(p: vec2<f32>, rc: vec4<f32>, info: vec4<f32>, si: i32, rs: f32, zf: f32) -> f32 {
    let hs = rc.zw * 0.5;
    let hm = min(hs.x, hs.y);
    // Per-corner radii (device px) x the normal radius scale. The
    // scalar the packer keeps in `info.x` is the smallest corner, so
    // uniform cards take this same path with a flat vec4 — byte-same SDF.
    let rv = min(u.u_corner[si] * rs, vec4<f32>(hm));
    var n = select(u.u_global2.y, info.z, info.z > 0.0);
    // Continuous-corner zone on genuinely uniform cards only: the zone
    // widens the exponent under the assumption that one radius shapes
    // every corner. Asymmetric cards keep the plain arc (n = 2) — the
    // per-quadrant SDF above already lands each corner on its own radius.
    let rmin = min(min(rv.x, rv.y), min(rv.z, rv.w));
    let rmax = max(max(rv.x, rv.y), max(rv.z, rv.w));
    let uniform = rmax - rmin <= max(rmax * 1e-3, 1e-3);
    var r = rmin;
    if (info.z <= 0.0 && zf > 1.0 && r > 0.0 && uniform) {
        let zone = min(zf * r, hm);
        if (zone > r) {
            n = -1.0 / log2(clamp(1.0 - 0.29289322 * r / zone, 0.5, 0.9999));
            r = zone;
        } else {
            n = 2.0;
        }
        return sdSuperellipseBox(p - (rc.xy + hs), hs, r, n);
    }
    if (info.z > 0.0) {
        // Capsule/circle exponent path: one arc radius for the whole
        // shape — the scalar the packer keeps (smallest corner) is the
        // pill radius itself on these shapes.
        return sdSuperellipseBox(p - (rc.xy + hs), hs, rmin, n);
    }
    return sdSuperellipseBox4(p - (rc.xy + hs), hs, rv, 2.0);
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
        dg = smin(dg, shapeOf(p, u.u_rects[i], info, i, rs, 1.0), ex.y);
    }
    return min(d, dg);
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
        let di = shapeOf(p, u.u_rects[i], info, i, rs, 1.0);
        if (ex.y > 0.0) {
            hmax = max(hmax, max(ex.y - abs(d - di), 0.0) / ex.y);
        }
        d = smin(d, di, ex.y);
    }
    return hmax;
}

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
        let di = shapeOf(px, u.u_rects[i], u.u_info[i], i, 1.0, u.u_global2.w);
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
    g.coverage = 1.0 - smoothstep(-0.75, 0.75, d);

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
        let di = shapeOf(px, u.u_rects[i], info, i, 1.0, u.u_global2.w);
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
    // Substrate defaults: flat surface (unit z normal) and zero optical
    // path until the stages that own them publish (`stageShape` keeps
    // the flat baseline, `stageRefract` bends + fills).
    g.n3 = vec3<f32>(0.0, 0.0, 1.0);
    g.path_length = 0.0;
    // Refract-owned fields, identity until Refract publishes: the
    // explicit fold may run Frost/Spectral/Rim without Refract, and
    // every one of those reads through `sample_point` (and the rim
    // reads the lens amounts). Unwritten WGSL struct fields are
    // undefined — without this, a Refract-less fold sampled garbage
    // (the black-card fold bug).
    g.sample_point = px;
    g.lensAmt = 0.0;
    g.dLens = 0.0;
    g.vv = 0.0;
    return g;
}

/// Lens normals from the lens field's gradient: the first-class `normal`
/// output (08.5 B1). Everything the displacement, the frost Jacobian and
/// the rim lights read as a normal is produced here and nowhere else.
/// The 3-D normal starts at the flat baseline (`normal.xy`, 1): the
/// height field that bends it is Refract's — with Refract absent
/// (Shape-only stacks) the surface stays flat, which is honest.
fn stageShape(g: GlassCtx) -> GlassCtx {
    var o = g;
    o.fxp = lensField(o.px + vec2<f32>(1.0, 0.0));
    o.fxm = lensField(o.px - vec2<f32>(1.0, 0.0));
    o.fyp = lensField(o.px + vec2<f32>(0.0, 1.0));
    o.fym = lensField(o.px - vec2<f32>(0.0, 1.0));
    let grad = vec2<f32>(o.fxp - o.fxm, o.fyp - o.fym) + vec2<f32>(1e-6);
    o.gradLen = length(grad);
    o.normal = grad / o.gradLen;
    o.n3 = vec3<f32>(o.normal, 1.0);
    return o;
}
