// Tier B3 — Frost (08.5): every backdrop blur the glass applies.
//
// Taxonomy: L2 backdrop blur / frost, plus the L2a pyramid stand-in.
//
// Deliberately kept as ONE effect (08.9): its four terms — the core
// engine pre-blur (consumed, not applied here), the post-lens Jacobian
// re-blur, the per-shape extra golden-angle taps and the wide two-ring
// tail — compose variances in quadrature (`wsig = sqrt(E.x² + xr2)`)
// and share one sample budget. Splitting them into three effects would
// triple the taps and break the composition. `Frost` stays one effect
// with three internal terms; its *sigma* is the material knob.
//
// Reads: `sample_point`, `normal`, `base`, `depth`, `lensAmt`, `dLens`,
// `halfMin`, `xb`, `d`, `fxp/fxm/fyp/fym`, `gradLen`;
// `u_var[4]` (wide frost sigma, mix edge/centre, size ref), `u_var[5]`
// (wide size drop, normal radius scale at .w), `u_var[10]` (post-lens
// sigma, blur size ref), `u_var[26].w` (post-lens Jacobian clamp),
// `u_global2.w` (corner zone).
// Writes: `base`, `col`.

/// Core lens sample, post-lens Jacobian blur, extra frost taps, and the
/// wide two-ring tail. Sets `base` and `col`.
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
    o.base = tex(o.sample_point).rgb;
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
        let tng = vec2<f32>(-o.normal.y, o.normal.x);
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
                o.col = o.col + (wn * wt) * tex(o.sample_point + o.normal * (ja * on) + tng * (jb * ot)).rgb;
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
            acc += tex(o.sample_point + dir * o.xb.xy * r).rgb;
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
            ring1 += tex(o.sample_point + (wsig * 1.0823922) * vec2<f32>(cos(a), sin(a))).rgb;
        }
        var ring2 = vec3<f32>(0.0);
        for (var i = 0; i < 10; i = i + 1) {
            let a = 0.31415927 + f32(i) * 0.62831853;
            ring2 += tex(o.sample_point + (wsig * 2.6131259) * vec2<f32>(cos(a), sin(a))).rgb;
        }
        let wide = ring1 * (0.8535534 / 6.0) + ring2 * (0.1464466 / 10.0);
        o.col = mix(o.col, wide, wideW);
    }
    return o;
}
