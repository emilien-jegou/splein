// See GlassOptics.hpp for the model contract.

#include "GlassOptics.hpp"

#include <hyprland/src/render/OpenGL.hpp>
#include <hyprland/src/plugins/PluginAPI.hpp>

#include <GLES3/gl32.h>

#include <cmath>
#include <cstdio>
#include <cstring>

namespace GlassOptics {
namespace {

inline SP<CShader>          g_opticsShader;
inline GLuint      g_vao = 0;
inline GLuint      g_vbo = 0;
inline GLuint      g_ubo = 0;
inline uint64_t    g_opticsDraws = 0;
// UBO layout mirror (std140): engine header (unused by the shader,
// reserved) + uSize + the packed uniform stream = 200 vec4s (the
// 736-float prefix plus the 56-float glow tail since wire v5).
constexpr size_t kGlassUboVec4s = 200;
constexpr size_t kGlassUboBytes = kGlassUboVec4s * 16;
constexpr size_t kGlassPackedFloats = 792;

// Own trivial passthrough vertex (positions [0,1] to clip). Same shape as
// the blur pass vertex; own VAO below, no dependence on Hyprland's.
constexpr const char* OPTICS_VERT = R"GLSL(
#version 300 es
layout(location = 0) in vec2 pos;
uniform mat3 proj;
out vec2 v_uv;
void main() {
    v_uv = pos;
    vec3 p = proj * vec3(pos, 1.0);
    gl_Position = vec4(p.xy, 0.0, 1.0);
}
)GLSL";

// Direct port of PassCtx::shade (engine glass.rs): single-shape version,
// sample-box px, y-up. Reversed smoothsteps are rewritten (GLSL leaves
// edge0>edge1 undefined); all formulas otherwise verbatim.
// ----------------------------------------------------------------------
// Optics fragment: line-for-line GLSL ES 3.00 port.

constexpr const char* OPTICS_FRAG = R"GLSL(
#version 300 es
// Liquid-glass optics pass — line-for-line GLSL ES 3.00 port of
// `crates/engines/riode-engine-wgpu/src/shaders/glass.wgsl` (itself a
// line-for-line port of `adaptive_liquid_glass/shaders/liquid_glass.frag`).
// Function names (`sdSuperellipseBox`, `shapeOf`, `smin`, `lensField`,
// `lensBlend`, `toneLut`, `blendMode`, `glowWash`) and stage order are
// kept so the three files diff by eye. Deviations from the WGSL twin:
//
//  * `textureSampleLevel` → `textureLod(..., 0.0)`: the early returns put
//    texture sampling in non-uniform control flow in both languages.
//  * `select(a, b, c)` → ternary; the one call with a function argument
//    (`fc`) is hoisted into a temp first (WGSL `select` is eager, the
//    GLSL ternary is not specified to be).
//  * Uniforms ride a std140 UBO (`GlassU`, same vec4 order as the WGSL
//    `GlassU` struct and the Rust `GlassFrameGpu`), bound by the host.
//    The backdrop is ONE texture (core-blurred): wide tail, post-lens
//    blur and per-shape extra frost are taps on it.
//  * `px` is box-relative: `gl_FragCoord.xy - u_boxMin`, with `u_size`
//    set to the box (backdrop texture) size. Box origin and size arrive
//    as plain uniforms (the only structural delta vs the WGSL twin,
//    which samples a surface-sized texture at surface coords). Y-down
//    throughout: the compositor's capture rows are used directly as GL
//    rows (mirrored capture), exactly like the copy pass, so no flip.
precision highp float;
precision highp int;

layout(std140) uniform GlassU {
    vec4 engine;      // core blur sigma device px (informational; unused)
    vec4 size;        // uSize: backdrop (box) size, device px
    vec4 u_global;    // count, dpr, lightAngle (radians), opaque flag
    vec4 u_global2;   // default smoothing px, cornerExponent, highContrast, cornerZone
    vec4 u_opaque;    // opaque fill rgb
    vec4 u_touch;     // touch x, y, glow, glowRadius px
    vec4 u_rects[24];
    vec4 u_info[24];  // radius px, reserved (always 0), cornerExponent, fill scale
    vec4 u_tints[24]; // rgb, tint strength
    vec4 u_var[36];
    vec4 u_extra[24]; // group index, smoothing px, opacity, press light
    vec4 u_clip[24];  // clip x0, y0, x1, y1 px
    vec4 u_blur[24];  // extra sigma x/y px, taps, -
    // Glow tail (wire v5, appended — every offset above is stable):
    // lobes (centre, sigma, amplitude px along x), colours,
    // geom (cross-edge line px, sigma px, refracted flag 0/1, blend mode
    // id 0..15 — see blendMode), outer rule (spill length device px,
    // tap count, boost, alpha cap — LightSpill; renderer-side replay).
    vec4 u_glow_lobes[6];
    vec4 u_glow_cols[6];
    vec4 u_glow_geom;
    vec4 u_outer;
} glass;

uniform sampler2D backdrop;
uniform vec2 u_boxMin;  // sample-box origin, capture rows (y-down)

layout(location = 0) out vec4 fragColor;

// Backdrop sample at device px `px` (box-relative), clamped to the
// texture, returned unpremultiplied (the reference reads straight
// `tex().rgb`; our offscreens store premultiplied sRGB).
// `textureLod`, not `texture`: the early returns below make the control
// flow non-uniform.
vec4 tex(vec2 px) {
    vec2 uv = clamp(px / glass.size.xy, vec2(0.0), vec2(1.0));
    vec4 s = textureLod(backdrop, uv, 0.0);
    float a = max(s.a, 1.0 / 255.0);
    return vec4(s.rgb / a, s.a);
}

float sdSuperellipseBox(vec2 p, vec2 halfSize, float r0, float n0) {
    float r = min(r0, min(halfSize.x, halfSize.y));
    vec2 q = abs(p) - halfSize + r;
    vec2 m = max(q, vec2(0.0));
    float corner = pow(pow(m.x, n0) + pow(m.y, n0), 1.0 / n0);
    return corner + min(max(q.x, q.y), 0.0) - r;
}

// Continuous corners: with a zone factor `zf` > 1 the corner curve
// starts zf x r from the corner (capped at half the shorter side), with
// the superellipse exponent that puts it through the circular arc's 45
// degree point, so it becomes the circle (n = 2) when the zone clamps to
// r (pills). A per-shape exponent (info.z > 0: capsules, circles) keeps
// the plain arc.
float shapeOf(vec2 p, vec4 rc, vec4 info, float rs, float zf) {
    vec2 hs = rc.zw * 0.5;
    float hm = min(hs.x, hs.y);
    float r = min(info.x * rs, hm);
    float n = info.z > 0.0 ? info.z : glass.u_global2.y;
    if (info.z <= 0.0 && zf > 1.0 && r > 0.0) {
        float zone = min(zf * r, hm);
        if (zone > r) {
            n = -1.0 / log2(clamp(1.0 - 0.29289322 * r / zone, 0.5, 0.9999));
            r = zone;
        } else {
            n = 2.0;
        }
    }
    return sdSuperellipseBox(p - (rc.xy + hs), hs, r, n);
}

float smin(float a, float b, float k) {
    if (k <= 0.0) {
        return min(a, b);
    }
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

// Lens field: the smooth union of the shapes with each corner radius x
// the normal radius scale. Its gradient gives the lens normals.
float lensField(vec2 p) {
    float d = 1e6;
    float dg = 1e6;
    float g = -1.0;
    for (int i = 0; i < 24; i++) {
        if (i >= int(glass.u_global.x)) {
            break;
        }
        vec4 info = glass.u_info[i];
        vec4 ex = glass.u_extra[i];
        if (ex.x != g) {
            d = min(d, dg);
            dg = 1e6;
            g = ex.x;
        }
        float rs = glass.u_var[5].w;
        dg = smin(dg, shapeOf(p, glass.u_rects[i], info, rs, 1.0), ex.y);
    }
    return min(d, dg);
}

// Tone LUT: 9 grey output knots at inputs i/8, hat-sum form. Identity
// knots reproduce c.
vec3 toneLut(vec3 c, vec4 a, vec4 b, float k8) {
    vec3 c8 = c * 8.0;
    return vec3(a.x)
        + (vec3(a.y) - vec3(a.x)) * clamp(c8, 0.0, 1.0)
        + (vec3(a.z) - vec3(a.y)) * clamp(c8 - 1.0, 0.0, 1.0)
        + (vec3(a.w) - vec3(a.z)) * clamp(c8 - 2.0, 0.0, 1.0)
        + (vec3(b.x) - vec3(a.w)) * clamp(c8 - 3.0, 0.0, 1.0)
        + (vec3(b.y) - vec3(b.x)) * clamp(c8 - 4.0, 0.0, 1.0)
        + (vec3(b.z) - vec3(b.y)) * clamp(c8 - 5.0, 0.0, 1.0)
        + (vec3(b.w) - vec3(b.z)) * clamp(c8 - 6.0, 0.0, 1.0)
        + (vec3(k8) - vec3(b.w)) * clamp(c8 - 7.0, 0.0, 1.0);
}

// The largest smin weight met while folding the lens field at p: 0
// outside every merge neck. Only the shapes of group [grp] count.
float lensBlend(vec2 p, float grp) {
    float d = 1e6;
    float hmax = 0.0;
    for (int i = 0; i < 24; i++) {
        if (i >= int(glass.u_global.x)) {
            break;
        }
        vec4 ex = glass.u_extra[i];
        if (ex.x != grp) {
            continue;
        }
        vec4 info = glass.u_info[i];
        float rs = glass.u_var[5].w;
        float di = shapeOf(p, glass.u_rects[i], info, rs, 1.0);
        if (ex.y > 0.0) {
            hmax = max(hmax, max(ex.y - abs(d - di), 0.0) / ex.y);
        }
        d = smin(d, di, ex.y);
    }
    return hmax;
}

// ----------------------------------------------------------------------
// Blend-mode library (PDF 1.4 / CSS Compositing and Blending Level 1) —
// GLSL ES 3.00 port of `blend_nonseparable.wgsl` + `blend_separable.wgsl`
// + the `blend.wgsl` dispatch (the wgpu engine's three-file library,
// concatenated there at pipeline build). Function names and wire ids kept
// so the files diff by eye; WGSL `select(a, b, c)` becomes `mix` with a
// `step` weight (`step(edge, x)` = 0 below the edge, 1 at/above — so
// `select(a, b, x <= e)` == `mix(a, b, step(x, e))`). All colours are
// straight (non-premultiplied) 0..1 channels. `base` is the backdrop,
// `src` the source pass.

// -- non-separable: Lum/Sat/SetSat/ClipColor/SetLum + the four modes.

float blendLum(vec3 c) {
    return dot(c, vec3(0.3, 0.59, 0.11));
}

float blendSat(vec3 c) {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

// Rescale mid/max toward the target spread, min to 0, preserving which
// original channel held each rank.
vec3 blendSetSat(vec3 c, float s) {
    vec3 v = c;
    int iMin = 0;
    int iMid = 1;
    int iMax = 2;
    if (v[iMid] < v[iMin]) { int t = iMin; iMin = iMid; iMid = t; }
    if (v[iMax] < v[iMin]) { int t = iMin; iMin = iMax; iMax = t; }
    if (v[iMax] < v[iMid]) { int t = iMid; iMid = iMax; iMax = t; }
    float mn = v[iMin];
    float md = v[iMid];
    float mx = v[iMax];
    if (mx > mn) {
        v[iMid] = (md - mn) * s / (mx - mn);
        v[iMax] = s;
    } else {
        v[iMid] = 0.0;
        v[iMax] = 0.0;
    }
    v[iMin] = 0.0;
    return v;
}

// Keep the colour in gamut after a luminance shift.
vec3 blendClipColor(vec3 c) {
    float l = blendLum(c);
    float n = min(c.r, min(c.g, c.b));
    float x = max(c.r, max(c.g, c.b));
    vec3 o = c;
    if (n < 0.0) {
        o = vec3(l) + (c - vec3(l)) * l / max(l - n, 1e-6);
    }
    if (x > 1.0) {
        o = vec3(l) + (o - vec3(l)) * (1.0 - l) / max(x - l, 1e-6);
    }
    return o;
}

vec3 blendSetLum(vec3 c, float l) {
    return blendClipColor(c + vec3(l - blendLum(c)));
}

vec3 blendHue(vec3 base, vec3 src) {
    return blendSetLum(blendSetSat(src, blendSat(base)), blendLum(base));
}

vec3 blendSaturation(vec3 base, vec3 src) {
    return blendSetLum(blendSetSat(base, blendSat(src)), blendLum(base));
}

vec3 blendColor(vec3 base, vec3 src) {
    return blendSetLum(src, blendLum(base));
}

vec3 blendLuminosity(vec3 base, vec3 src) {
    return blendSetLum(base, blendLum(src));
}

// -- separable: the eleven channel-pair modes.

vec3 blendMultiply(vec3 base, vec3 src) {
    return base * src;
}

vec3 blendScreen(vec3 base, vec3 src) {
    return base + src - base * src;
}

vec3 blendHardLight(vec3 base, vec3 src) {
    return mix(blendScreen(base, 2.0 * src - 1.0), blendMultiply(base, 2.0 * src), step(src, vec3(0.5)));
}

// Overlay(base, src) = HardLight(src, base).
vec3 blendOverlay(vec3 base, vec3 src) {
    return blendHardLight(src, base);
}

vec3 blendDarken(vec3 base, vec3 src) {
    return min(base, src);
}

vec3 blendLighten(vec3 base, vec3 src) {
    return max(base, src);
}

vec3 blendColorDodge(vec3 base, vec3 src) {
    vec3 r = min(vec3(1.0), base / max(vec3(1.0) - src, vec3(1e-6)));
    r = mix(r, vec3(1.0), step(vec3(1.0), src));
    r = mix(r, vec3(0.0), 1.0 - step(vec3(0.0), base));
    return r;
}

vec3 blendColorBurn(vec3 base, vec3 src) {
    vec3 d = vec3(1.0) - min(vec3(1.0), (vec3(1.0) - base) / max(src, vec3(1e-6)));
    d = mix(d, vec3(0.0), step(src, vec3(0.0)));
    d = mix(d, vec3(1.0), step(vec3(1.0), base));
    return d;
}

vec3 blendSoftLight(vec3 base, vec3 src) {
    // W3C D(B): the cubic ramp below 0.25, sqrt above — continuous at
    // the knot (both give 0.5 at B = 0.25); NOT the B^2 misremembering.
    vec3 d = mix(sqrt(base), ((16.0 * base - 12.0) * base + 4.0) * base, step(base, vec3(0.25)));
    return mix(base + (2.0 * src - 1.0) * (d - base), base - (1.0 - 2.0 * src) * base * (1.0 - base), step(src, vec3(0.5)));
}

vec3 blendDifference(vec3 base, vec3 src) {
    return abs(base - src);
}

vec3 blendExclusion(vec3 base, vec3 src) {
    return base + src - 2.0 * base * src;
}

// Dispatch: one switch over the packed wire id (`BlendMode::
// as_uniform_id` — the Rust discriminant, never renumbered). Normal (0)
// and anything out of range return the source — the pre-blend path.
vec3 blendMode(vec3 base, vec3 src, int mode) {
    switch (mode) {
        case 1: return blendMultiply(base, src);
        case 2: return blendScreen(base, src);
        case 3: return blendOverlay(base, src);
        case 4: return blendDarken(base, src);
        case 5: return blendLighten(base, src);
        case 6: return blendColorDodge(base, src);
        case 7: return blendColorBurn(base, src);
        case 8: return blendHardLight(base, src);
        case 9: return blendSoftLight(base, src);
        case 10: return blendDifference(base, src);
        case 11: return blendExclusion(base, src);
        case 12: return blendHue(base, src);
        case 13: return blendSaturation(base, src);
        case 14: return blendColor(base, src);
        case 15: return blendLuminosity(base, src);
        default: return src;
    }
}

// ----------------------------------------------------------------------
// Glow (emissive wash inside the slab) — line-for-line GLSL port of the
// wgpu twin's `glowOverlay`/`glowWash` (see glass.wgsl: the CPU resolves
// the stop list to these uniform lobes at pack time and the shader only
// evaluates them — no palette or geometry is hardcoded). Interior only:
// the halo outside the edge is the renderer's theatre-mode replay, not
// shading. Display-referred adaptation: the returned rgb is the
// normalized weighted lobe mean (<=1) and the alpha is the envelope x
// gain — addition would clip to invisible white on our hard-clamped
// backdrops.

vec4 glowOverlay(vec2 p, float gain) {
    float along = p.x;
    float cross = (p.y - glass.u_glow_geom.x) / max(glass.u_glow_geom.y, 1.0);
    float cross2 = cross * cross;
    vec3 num = vec3(0.0);
    float den = 0.0;
    for (int j = 0; j < 6; j++) {
        vec4 L = glass.u_glow_lobes[j];
        float d = (along - L.x) / max(L.y, 1.0);
        float wgt = L.z * exp(-d * d - cross2);
        num += glass.u_glow_cols[j].rgb * wgt;
        den += wgt;
    }
    vec3 rgb = num / max(den, 1e-4);
    return vec4(rgb, clamp(den * gain, 0.0, 1.0));
}

// The interior wash: lens-independent evaluation — geometric pixel, no
// bent sampling. Alpha-weighted vertical taps blend neighbouring lobes
// into one continuous wash instead of separated plumes.
vec4 glowWash(vec2 p, float gain) {
    float off = max(glass.u_glow_geom.y, 1.0) * 0.75;
    vec4 s0 = glowOverlay(p, gain);
    vec4 s1 = glowOverlay(p + vec2(0.0, off), gain);
    vec4 s2 = glowOverlay(p - vec2(0.0, off), gain);
    float asum = max(s0.a + s1.a + s2.a, 1e-4);
    vec3 rgb = (s0.rgb * s0.a + s1.rgb * s1.a + s2.rgb * s2.a) / asum;
    return vec4(rgb, asum / 3.0);
}

void main() {
    // Output is premultiplied colour composited srcOver onto the sharp
    // backdrop: transparent outside the shape.
    // gl_FragCoord is used directly as capture rows (mirrored capture),
    // minus the sample-box origin.
    vec2 px = gl_FragCoord.xy - u_boxMin;
    int count = int(glass.u_global.x);
    if (count == 0) {
        fragColor = vec4(0.0);
        return;
    }

    // Per-shape attributes blended by proximity (online softmax: weights
    // shifted by the nearest distance so the nearest shape always weighs
    // 1). The nearest shape's group owns the pixel; the outline is the
    // union of every group's smooth union.
    float d = 1e6;
    float dg = 1e6;
    float g = -1.0;
    float dOwn = 1e6;
    vec4 own = vec4(0.0, 0.0, 1.0, 0.0);
    vec4 ownClip = vec4(-1e9, -1e9, 1e9, 1e9);
    for (int i = 0; i < 24; i++) {
        if (i >= count) {
            break;
        }
        vec4 ex = glass.u_extra[i];
        if (ex.x != g) {
            d = min(d, dg);
            dg = 1e6;
            g = ex.x;
        }
        float di = shapeOf(px, glass.u_rects[i], glass.u_info[i], 1.0, glass.u_global2.w);
        dg = smin(dg, di, ex.y);
        if (di < dOwn) {
            dOwn = di;
            own = ex;
            ownClip = glass.u_clip[i];
        }
    }
    d = min(d, dg);
    float ownK = own.y;
    float keep = own.z
        * step(ownClip.x, px.x) * step(ownClip.y, px.y)
        * step(px.x, ownClip.z) * step(px.y, ownClip.w);
    if (keep <= 0.0) {
        fragColor = vec4(0.0);
        return;
    }

    float wsum = 0.0;
    float halfMin = 0.0;
    float fillScale = 0.0;
    float light = 0.0;
    vec4 tint = vec4(0.0);
    vec4 rcB = vec4(0.0);
    vec4 xb = vec4(0.0);
    float wk = ownK * 0.5 + 1.0;
    float dmin = 1e6;
    for (int i = 0; i < 24; i++) {
        if (i >= count) {
            break;
        }
        if (glass.u_extra[i].x != own.x) {
            continue;
        }
        vec4 info = glass.u_info[i];
        float di = shapeOf(px, glass.u_rects[i], info, 1.0, glass.u_global2.w);
        float e = max(di, 0.0);
        if (e < dmin) {
            float s = exp(-(dmin - e) / wk);
            wsum *= s;
            halfMin *= s;
            fillScale *= s;
            light *= s;
            tint *= s;
            rcB *= s;
            xb *= s;
            dmin = e;
        }
        float w = exp(-(e - dmin) / wk);
        wsum += w;
        halfMin += w * 0.5 * min(glass.u_rects[i].z, glass.u_rects[i].w);
        fillScale += w * info.w;
        light += w * glass.u_extra[i].w;
        tint += w * glass.u_tints[i];
        rcB += w * glass.u_rects[i];
        xb += w * glass.u_blur[i];
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
    vec4 A = glass.u_var[0];
    vec4 B = glass.u_var[1];
    vec4 C = glass.u_var[2];
    vec4 D = glass.u_var[3];
    vec4 E = glass.u_var[4];
    vec4 F = glass.u_var[5];
    vec4 G = glass.u_var[6];
    vec4 H = glass.u_var[7];
    vec4 I = glass.u_var[8];
    vec4 J = glass.u_var[9];
    vec4 K = glass.u_var[10];
    vec4 L4 = glass.u_var[11];
    vec4 Q4 = glass.u_var[32];
    vec4 R4 = glass.u_var[34];
    vec4 M = glass.u_var[24];
    vec4 N = glass.u_var[25];
    vec4 O = glass.u_var[26];
    vec4 P = glass.u_var[30];
    float hc = glass.u_global2.z;

    float inside = 1.0 - smoothstep(-0.75, 0.75, d);

    // Outside the silhouette the optics pass emits nothing: the halo is
    // the renderer's theatre-mode replay (blurred finished card under
    // the sharp one, `refraction_replay`), not shading. A shader-side
    // halo could only re-evaluate the lobe field — a second gradient
    // that diverges from the card by construction — so it was deleted
    // (twin parity, commit 4100f2f7).
    float glowGain = Q4.w;
    float glowIn = R4.x;
    if (inside <= 0.0) {
        fragColor = vec4(0.0);
        return;
    }

    // Opaque (Reduce Transparency).
    if (glass.u_global.w > 0.5) {
        vec3 solid = mix(glass.u_opaque.rgb, tint.rgb, tint.a);
        fragColor = vec4(solid * inside, inside) * keep;
        return;
    }

    float fxp = lensField(px + vec2(1.0, 0.0));
    float fxm = lensField(px - vec2(1.0, 0.0));
    float fyp = lensField(px + vec2(0.0, 1.0));
    float fym = lensField(px - vec2(0.0, 1.0));
    vec2 grad = vec2(fxp - fxm, fyp - fym) + vec2(1e-6);
    float gradLen = length(grad);
    vec2 nrm = grad / gradLen;

    // Merge necks: where the lens field's smooth union is active the lens
    // treats the blob as one surface (depth over gradient length, scaled
    // displacement fades to 0 at the saddle instead of flipping sides).
    float neck = 1.0;
    if (count > 1 && ownK > 0.0 && lensBlend(px, own.x) > 0.0) {
        neck = clamp(0.5 * gradLen, 0.05, 1.0);
    }

    // Lens v3: displacement along the normal falls off exponentially with
    // depth, cut to 0 at `band`; shapes below the size ref get a scaled lens.
    float depth = -d;
    float ldepth = depth / neck;
    float band = max(A.y, 1.0);
    float decay = max(A.x, 1e-3);
    float sc = C.w > 0.0 ? min(1.0, halfMin / C.w) : 1.0;
    float cut = exp(-band / decay);
    float ls = max(decay * sc, 1e-3);
    float ex = exp(-max(ldepth, 0.0) / ls);
    float v = max(ex - cut, 0.0) / max(1.0 - cut, 1e-6);
    float lensK = A.z * (1.0 - 0.5 * hc) * band * sc;
    float lensAmt = lensK * v;
    float dLens = v > 0.0 ? -lensK * ex / ls / max(1.0 - cut, 1e-6) : 0.0;
    float le = I.y * exp(-max(ldepth, 0.0) / max(I.z, 1e-3));
    lensAmt -= le;
    dLens += le / max(I.z, 1e-3);
    lensAmt *= neck;
    vec2 sp = px + nrm * lensAmt;

    // Frost v2 + post-lens blur (3x3 Gauss-Hermite through the lens
    // Jacobian) + extra frost (uBlur) + wide tail (two rings).
    vec3 base = tex(sp).rgb;
    vec3 col = base;
    float sPost = K.z * (K.w > 0.0 ? min(1.0, halfMin / K.w) : 1.0);
    if (sPost >= 0.25) {
        float fc_lf = lensField(px);
        float fc = (glass.u_var[5].w == 1.0 && glass.u_var[17].w == 1.0 && glass.u_global2.w <= 1.0)
            ? d : fc_lf;
        float lap = fxp + fxm + fyp + fym - 4.0 * fc;
        float kappa = lap / max(0.5 * gradLen, 1e-3);
        float jm = max(O.w, 1e-3);
        float ja = clamp(1.0 - dLens, -jm, jm) * sPost;
        float jb = clamp(1.0 + lensAmt * kappa, -jm, jm) * sPost;
        vec2 tng = vec2(-nrm.y, nrm.x);
        col = base * 0.44444445;
        for (int i = 0; i < 3; i++) {
            float on = float(i - 1) * 1.7320508;
            float wn = i == 1 ? 0.6666667 : 0.1666667;
            for (int j = 0; j < 3; j++) {
                if (i == 1 && j == 1) {
                    continue;
                }
                float ot = float(j - 1) * 1.7320508;
                float wt = j == 1 ? 0.6666667 : 0.1666667;
                col = col + (wn * wt) * tex(sp + nrm * (ja * on) + tng * (jb * ot)).rgb;
            }
        }
    }
    float xr2 = 0.0;
    if (xb.z >= 1.0 && max(xb.x, xb.y) >= 0.25) {
        int n = int(xb.z + 0.5);
        vec3 acc = vec3(0.0);
        vec2 dir = vec2(1.0, 0.0);
        mat2 turn = mat2(vec2(-0.7373688, 0.6754903), vec2(-0.6754903, -0.7373688));
        for (int i = 0; i < 32; i++) {
            if (i >= n) {
                break;
            }
            float r = sqrt(-2.0 * log(1.0 - (float(i) + 0.5) / float(n)));
            acc += tex(sp + dir * xb.xy * r).rgb;
            dir = turn * dir;
        }
        col = col + acc / float(n) - base;
        xr2 = xb.x * xb.y;
    }
    float hmw = max(halfMin, 1.0);
    float wideW = E.y + (E.z - E.y) * (depth - lensAmt) / hmw;
    if (E.w > 0.0) {
        wideW -= F.x * max(0.0, 1.0 - hmw / E.w);
    }
    wideW = E.x > 0.0 ? clamp(wideW, 0.0, 1.0) : 0.0;
    if (wideW > 0.0) {
        float wsig = sqrt(E.x * E.x + xr2);
        vec3 ring1 = vec3(0.0);
        for (int i = 0; i < 6; i++) {
            float a = float(i) * 1.0471976;
            ring1 += tex(sp + (wsig * 1.0823922) * vec2(cos(a), sin(a))).rgb;
        }
        vec3 ring2 = vec3(0.0);
        for (int i = 0; i < 10; i++) {
            float a = 0.31415927 + float(i) * 0.62831853;
            ring2 += tex(sp + (wsig * 2.6131259) * vec2(cos(a), sin(a))).rgb;
        }
        vec3 wide = ring1 * (0.8535534 / 6.0) + ring2 * (0.1464466 / 10.0);
        col = mix(col, wide, wideW);
    }
    // Dispersion: red/blue taps carry the core channel's full frost
    // offset, so dispersion 0 is an exact no-op.
    if (A.w != 0.0) {
        vec3 coreOff = col - base;
        vec2 disp = nrm * lensAmt * abs(A.w);
        float dw = v;
        col.r = mix(col.r, tex(sp + disp).r + coreOff.r, dw);
        vec3 sm = tex(sp - disp).rgb + coreOff;
        col.b = mix(col.b, sm.b, dw);
        if (A.w < 0.0) {
            col.g = mix(col.g, sm.g, dw);
        }
    }

    float luma = dot(col, vec3(0.2126, 0.7152, 0.0722));
    col = mix(vec3(luma), col, D.w);

    // Tone LUT (+ luma-keyed blend), small-shape shadow lift, fill +
    // dim, ambient lift, small-shape tone curve, tint, ambient colour.
    vec3 toned = toneLut(col, G, H, I.x);
    if (P.z != 0.0) {
        float y = dot(col, vec3(0.2126, 0.7152, 0.0722));
        vec3 keyed = col + vec3(toneLut(vec3(y), G, H, I.x).x - y);
        toned = mix(toned, keyed, P.z);
    }
    col = toned;

    float liftSz = K.y > 0.0 ? max(0.0, 1.0 - halfMin / max(K.y, 1e-3)) : 0.0;
    vec3 lk = max(vec3(1.0) - col / max(K.x, 1e-3), vec3(0.0));
    col += (I.w * liftSz) * lk * lk;

    col = mix(col, D.rgb, B.z * fillScale);
    col *= (1.0 - B.w);

    float sw = 0.0;
    if (O.z > 0.0) {
        sw = clamp((O.z - halfMin) / max(O.z - O.y, 1e-3), 0.0, 1.0);
        col = mix(col, toneLut(col, M, N, O.x), sw);
    }

    col = mix(col, tint.rgb, tint.a);

    float am = P.x * (1.0 - sw);
    if (am != 0.0) {
        vec2 a0 = rcB.xy - vec2(P.y);
        vec2 asz = rcB.zw + vec2(2.0 * P.y);
        vec3 acc = vec3(0.0);
        for (int i = 0; i < 5; i++) {
            for (int j = 0; j < 5; j++) {
                acc += tex(a0 + asz * (vec2(float(i), float(j)) + 0.5) * 0.2).rgb;
            }
        }
        acc *= 0.04;
        col += am * (acc - vec3(dot(acc, vec3(0.2126, 0.7152, 0.0722))));
    }

    // Voice glow inside the slab: the shared wash (`glowWash`) composited
    // over the glass base at `Glow::blend` (`u_glow_geom.w`): Normal (0)
    // is the plain source-over mix (blendMode returns src), the other
    // modes tone the base like ink on glass. Reference behaviour samples
    // at the geometric pixel; the opt-in deviation (`Glow::refracted`,
    // `u_glow_geom.z`) samples at the lens-displaced point. Same stage
    // order as the wgpu twin: before the rim lights, which stay white.
    if (glowGain > 0.0) {
        vec2 gp = glass.u_glow_geom.z > 0.5 ? sp : px;
        vec4 ov = glowWash(gp, glowGain * glowIn);
        col = mix(col, blendMode(col, ov.rgb, int(glass.u_glow_geom.w)), ov.a);
    }

    // Directional + isotropic rim, press lift + touch glow.
    vec2 L = vec2(cos(glass.u_global.z), sin(glass.u_global.z));
    float rimW = max(B.x * (1.0 + hc), 0.5);
    float rim = 1.0 - smoothstep(0.0, rimW, depth);
    float spec = rim * (max(dot(nrm, L), 0.0) + L4.x * max(dot(nrm, -L), 0.0));
    col += B.y * (1.0 + hc) * spec;

    float ra = J.x * clamp(1.0 - depth / max(J.y, 1e-3), 0.0, 1.0)
        * (1.0 - smoothstep(J.z - 0.5, J.z + 0.5, depth));
    float rl = dot(col, vec3(0.2126, 0.7152, 0.0722));
    float ra0 = ra * (J.w + (1.0 - J.w) * clamp(rl / 0.5, 0.0, 1.0));
    col += (vec3(1.0) - col) * ra0;

    col += R4.z * light;
    if (glass.u_touch.z > 0.0) {
        vec2 dt = px - glass.u_touch.xy;
        float gr = max(glass.u_touch.w, 1.0);
        col += F.y * glass.u_touch.z * exp(-dot(dt, dt) / (2.0 * gr * gr));
    }

    fragColor = vec4(clamp(col, vec3(0.0), vec3(1.0)) * inside, inside) * keep;
}

)GLSL";

constexpr std::array<float, 9> FULLSCREEN_PROJECTION = {
    2.0f, 0.0f, 0.0f,
    0.0f, 2.0f, 0.0f,
    -1.0f, -1.0f, 1.0f,
};

bool ensureShaderImpl(HANDLE handle) {
    if (g_opticsShader)
        return true;
    g_opticsShader = makeShared<CShader>();
    if (!g_opticsShader->createProgram(OPTICS_VERT, OPTICS_FRAG, true)) {
        HyprlandAPI::addNotification(handle, "[riode-glass] optics shader failed to compile", CHyprColor{1.0, 0.2, 0.2, 1.0},
                                     5000);
        g_opticsShader.reset();
        return false;
    }
    if (!g_vao) {
        constexpr float QUAD[16] = {
            0.0f, 0.0f, 0.0f, 0.0f,
            1.0f, 0.0f, 1.0f, 0.0f,
            0.0f, 1.0f, 0.0f, 1.0f,
            1.0f, 1.0f, 1.0f, 1.0f,
        };
        glGenVertexArrays(1, &g_vao);
        glGenBuffers(1, &g_vbo);
        glBindVertexArray(g_vao);
        glBindBuffer(GL_ARRAY_BUFFER, g_vbo);
        glBufferData(GL_ARRAY_BUFFER, sizeof(QUAD), QUAD, GL_STATIC_DRAW);
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(float), (void*)0);
        glEnableVertexAttribArray(1);
        glVertexAttribPointer(1, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(float), (void*)(2 * sizeof(float)));
        glBindVertexArray(0);
        glBindBuffer(GL_ARRAY_BUFFER, 0);
    }
    return true;
}

} // namespace

bool parseShape(const std::string& shape, float& x, float& y, float& w, float& h, float radii[4]) {
    if (shape.empty() || shape == "none")
        return false;
    double sx, sy, sw, sh, r[4];
    if (std::sscanf(shape.c_str(), "%lf,%lf,%lfx%lf %lf,%lf,%lf,%lf", &sx, &sy, &sw, &sh, &r[0], &r[1], &r[2], &r[3]) != 8)
        return false;
    if (sw <= 0.0 || sh <= 0.0)
        return false;
    x = (float)sx;
    y = (float)sy;
    w = (float)sw;
    h = (float)sh;
    for (int i = 0; i < 4; ++i)
        radii[i] = (float)std::max(r[i], 0.0);
    return true;
}

void clampRadii(float radii[4], float w, float h) {
    float ratio = 1.0f;
    const auto shrinkFor = [&](float a, float b, float edge) {
        if (a + b > edge && a + b > 0.0f)
            ratio = std::min(ratio, edge / (a + b));
    };
    shrinkFor(radii[0], radii[1], w);
    shrinkFor(radii[1], radii[2], h);
    shrinkFor(radii[2], radii[3], w);
    shrinkFor(radii[3], radii[0], h);
    if (ratio < 1.0f)
        for (int i = 0; i < 4; ++i)
            radii[i] *= ratio;
}

bool ensureShader(HANDLE handle) {
    return ensureShaderImpl(handle);
}

void draw(const DrawArgs& args) {
    auto* gl = Render::GL::g_pHyprOpenGL.get();
    if (!gl || !g_opticsShader || !args.coreFb)
        return;
    auto coreTex = args.coreFb->getTexture();
    if (!coreTex || !args.packed || args.sampleW <= 0.0f || args.sampleH <= 0.0f || args.fbW <= 0 || args.fbH <= 0)
        return;

    auto shader = gl->useShader(g_opticsShader).lock();
    if (!shader)
        return;
    const auto program = shader->program();
    shader->setUniformMatrix3fv(SHADER_PROJ, 1, GL_FALSE, FULLSCREEN_PROJECTION);
    const auto U = [&](const char* name) {
        return glGetUniformLocation(program, name);
    };
    // Backdrop on unit 0; the UBO rides binding point 0.
    glActiveTexture(GL_TEXTURE0);
    coreTex->bind();
    glUniform1i(U("backdrop"), 0);
    // Offscreen card FBs are box-sized: fragCoord 0.5 IS box-local 0.5,
    // identical to the window-path fragCoord − boxMin. Same px values.
    const bool offscreen = args.offscreenFbid != 0;
    glUniform2f(U("u_boxMin"), offscreen ? 0.0f : args.sampleMinX, offscreen ? 0.0f : args.sampleMinY);

    if (!g_ubo)
        glGenBuffers(1, &g_ubo);
    // std140 mirror of the WGSL GlassU / Rust GlassFrameGpu: engine
    // header (frame sigmas, informational) + uSize + the packed uniform
    // stream (prefix + glow tail since wire v5).
    struct GlassUbo {
        float engine[4];
        float size[4];
        float data[kGlassPackedFloats];
    };
    static_assert(sizeof(GlassUbo) == kGlassUboBytes, "UBO must mirror GlassFrameGpu");
    GlassUbo ubo{};
    ubo.engine[0] = args.sigmaX;
    ubo.engine[1] = args.sigmaY;
    ubo.size[0] = args.sampleW;
    ubo.size[1] = args.sampleH;
    memcpy(ubo.data, args.packed, sizeof(ubo.data));
    glBindBuffer(GL_UNIFORM_BUFFER, g_ubo);
    glBufferData(GL_UNIFORM_BUFFER, sizeof(ubo), &ubo, GL_DYNAMIC_DRAW);
    const GLuint block = glGetUniformBlockIndex(program, "GlassU");
    if (block == GL_INVALID_INDEX) {
        glBindBuffer(GL_UNIFORM_BUFFER, 0);
        return;
    }
    glUniformBlockBinding(program, block, 0);
    glBindBufferBase(GL_UNIFORM_BUFFER, 0, g_ubo);
    glBindBuffer(GL_UNIFORM_BUFFER, 0);

    glBindVertexArray(g_vao);
    // The caller leaves DRAW bound to a pre-blur temp; rebind the pass
    // target (offscreen card: box-sized viewport/scissor at the origin —
    // window-path coords are monitor-FB px).
    glBindFramebuffer(GL_DRAW_FRAMEBUFFER, offscreen ? args.offscreenFbid : args.drawFbid);
    gl->blend(true);
    glViewport(0, 0, offscreen ? static_cast<int>(args.sampleW) : args.fbW,
               offscreen ? static_cast<int>(args.sampleH) : args.fbH);
    glEnable(GL_SCISSOR_TEST);
    if (offscreen)
        glScissor(0, 0, static_cast<int>(args.sampleW), static_cast<int>(args.sampleH));
    else
        glScissor(args.scissorX, args.scissorY, args.scissorW, args.scissorH);
    glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
    glDisable(GL_SCISSOR_TEST);
    glBindVertexArray(0);

    g_opticsDraws++;
}


uint64_t opticsDraws() {
    return g_opticsDraws;
}

} // namespace GlassOptics
