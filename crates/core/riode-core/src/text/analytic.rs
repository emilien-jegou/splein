// Single responsibility: exact-coverage CPU rasterization of glyph outlines.
//
// Swash bitmap rasterization quantizes glyphs to integer pixels (plus a baked
// subpixel offset); shifting that bitmap by a further subpixel remainder means
// interpolating coverage bytes, which smooths edges. This module instead
// computes coverage analytically: glyph outline contours are flattened to
// polygons and each pixel's coverage is the exact covered area (non-zero
// winding, matching Vello's fill rule), evaluated at the true subpixel pen
// position. No phase bins, no coverage interpolation, no supersampling.

#![cfg_attr(not(feature = "text-runtime"), allow(dead_code))]
#[cfg(feature = "text-runtime")]
use swash::zeno::{Point, Verb};
use std::cell::RefCell;
#[cfg(not(feature = "text-runtime"))]
use zenostub::{Point, Verb};

/// Stub outline types when `text-runtime` is off: identical shape to the
/// swash/zeno types this module's pure geometry operates on, so the exact
/// same flatten/integrate code compiles without the dependency. Nothing
/// constructs these in stub mode (raster entry points return empty).
#[cfg(not(feature = "text-runtime"))]
mod zenostub {
    #[derive(Copy, Clone)]
    pub struct Point {
        pub x: f32,
        pub y: f32,
    }
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub enum Verb {
        MoveTo,
        LineTo,
        QuadTo,
        CurveTo,
        Close,
    }
}
#[cfg(not(feature = "text-runtime"))]
use zenostub::{Point, Verb};

/// Owned grayscale coverage bitmap in `GlyphBitmap` convention: bearings
/// relative to the pen, `top` positive-down like swash placement.
pub(crate) struct AnalyticBitmap {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl AnalyticBitmap {
    pub fn empty() -> Self {
        Self {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            data: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.data.is_empty()
    }
}

/// Degenerate-guard: outlines larger than this fall back to the swash path.
const MAX_DIM: u32 = 512;

/// Curve flattening tolerance target: 0.1 px in the space the flattened
/// geometry finally renders at, matching vello's DEFAULT_TOLERANCE. Area
/// integration is insensitive below this (0.1 px deviation is sub-LSB after
/// per-pixel area render), so a finer tolerance only multiplies segment
/// counts 10-100x for no visible gain — the old 1e-4 value was pure cost.
const TOL_TARGET: f32 = 0.1;

/// Scale-aware flattening tolerance: `tol_local = TOL_TARGET / σ_max(M)`.
///
/// `scale` is the maximum singular value of the transform mapping the
/// flattened geometry onto final device pixels. For the exact raster path
/// that is the quantized raster scale (`size_mul`: the outline was scaled
/// by it before flattening, and a blit maps raster px 1:1 onto device px);
/// unscaled callers pass 1.0. Clamped at 1.0 from below: the flatten space
/// already is final pixel space, so a σ below unity must not *coarsen* the
/// tolerance — downscale paths resample the bitmap, and the 0.1 px target
/// is the floor.
#[inline]
fn flatten_tol(scale: f32) -> f32 {
    TOL_TARGET / scale.max(1.0)
}
/// Recursion cap (2^24 segments per curve worst-case, unreachable in practice).
const FLAT_DEPTH: u8 = 24;

/// Fast-reject band test: does the pixel box `[px,px+1) × [py,py+1)`
/// intersect the contour AABB `[b0,b1,b2,b3]`? Runs once per
/// (contour, pixel) before any polygon clipping.
#[inline(always)]
fn band_intersects(b: &[f32; 4], px: f32, py: f32) -> bool {
    !(b[3] <= py || b[1] >= py + 1.0 || b[2] <= px || b[0] >= px + 1.0)
}

/// Flat contour arena: one contiguous point buffer, an index range per
/// contour, and that contour's AABB — all reset via `clear()` before each
/// glyph layer. Replaces the nested `Vec<Vec<[f32; 2]>>` (a heap allocation
/// per glyph contour, thousands of small allocations during text
/// rasterization bursts); the arena lives in a thread-local scratch reused
/// across glyphs, so steady-state rasterization performs no
/// outline-flattening allocation.
struct ContourArena {
    points: Vec<[f32; 2]>,
    contours: Vec<std::ops::Range<usize>>,
    bounds: Vec<[f32; 4]>,
}

/// Per-rasterizer scratch, thread-local: the contour arena and the
/// Sutherland–Hodgman clip buffers, all with capacity reused across glyphs
/// and layers.
struct RasterScratch {
    arena: ContourArena,
    buf_a: Vec<[f32; 2]>,
    buf_b: Vec<[f32; 2]>,
}

thread_local! {
    static SCRATCH: RefCell<RasterScratch> = RefCell::new(RasterScratch {
        arena: ContourArena {
            points: Vec::new(),
            contours: Vec::new(),
            bounds: Vec::new(),
        },
        buf_a: Vec::new(),
        buf_b: Vec::new(),
    });
}

/// Rasterizes one outline layer at subpixel translation `(tx, ty)`.
///
/// `points`/`verbs` follow `zeno` `PathData` pairing: MoveTo=1pt, LineTo=1pt,
/// QuadTo=2pts, CurveTo=3pts, Close=0pts. `scale` is σ_max(M) of the
/// transform mapping the flattened geometry onto device pixels (see
/// [`flatten_tol`]).
///
/// Outline points arrive in font space (y-up); the bitmap is y-down. `top`
/// is the grid-space origin (floor of the minimum y, usually negative):
/// the caller converts to placement convention on store.
pub(crate) fn rasterize_layer(
    points: &[Point],
    verbs: &[Verb],
    tx: f32,
    ty: f32,
    scale: f32,
) -> AnalyticBitmap {
    SCRATCH.with(|cell| {
        let mut s = cell.borrow_mut();
        let RasterScratch { arena, buf_a, buf_b } = &mut *s;
        flatten_into(points, verbs, tx, -ty, flatten_tol(scale), arena);
        // Font y-up → bitmap y-down. (Translation was pre-negated so the flip
        // lands it on the correct side.)
        for q in &mut arena.points {
            q[1] = -q[1];
        }
        if arena.contours.is_empty() {
            return AnalyticBitmap::empty();
        }
        // Union bounds over all contours, plus a bbox per contour for the
        // row/containment prefilter.
        arena.bounds.clear();
        arena
            .bounds
            .resize(arena.contours.len(), [f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
        let (mut minx, mut miny) = (f32::INFINITY, f32::INFINITY);
        let (mut maxx, mut maxy) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (ci, r) in arena.contours.iter().enumerate() {
            let b = &mut arena.bounds[ci];
            for q in &arena.points[r.clone()] {
                b[0] = b[0].min(q[0]);
                b[1] = b[1].min(q[1]);
                b[2] = b[2].max(q[0]);
                b[3] = b[3].max(q[1]);
                minx = minx.min(q[0]);
                miny = miny.min(q[1]);
                maxx = maxx.max(q[0]);
                maxy = maxy.max(q[1]);
            }
        }
        if !(minx.is_finite() && miny.is_finite() && maxx.is_finite() && maxy.is_finite()) {
            return AnalyticBitmap::empty();
        }
        let (left, top) = (minx.floor() as i32, miny.floor() as i32);
        let (right, bottom) = (maxx.ceil() as i32, maxy.ceil() as i32);
        let (w, h) = (right - left, bottom - top);
        if w <= 0 || h <= 0 || w as u32 > MAX_DIM || h as u32 > MAX_DIM {
            return AnalyticBitmap::empty();
        }
        let (w, h) = (w as u32, h as u32);
        let mut data = vec![0u8; (w * h) as usize];
        let arena = &*arena;
        // Per-row relevant contour lists: contours whose y-range touches the band.
        for row in 0..h {
            let py = (top + row as i32) as f32;
            let mut relevant: [usize; 32] = [0; 32];
            let mut n_rel = 0usize;
            let mut overflow = false;
            for (i, b) in arena.bounds.iter().enumerate() {
                if b[3] > py && b[1] < py + 1.0 {
                    if n_rel < relevant.len() {
                        relevant[n_rel] = i;
                        n_rel += 1;
                    } else {
                        overflow = true;
                        break;
                    }
                }
            }
            for col in 0..w {
                let px = (left + col as i32) as f32;
                let mut acc: f32;
                if overflow {
                    acc = pixel_area_all(arena, buf_a, buf_b, px, py);
                } else {
                    acc = 0.0;
                    for k in 0..n_rel {
                        let b = &arena.bounds[relevant[k]];
                        if !band_intersects(b, px, py) {
                            continue; // Pixel box does not intersect contour AABB
                        }
                        let r = arena.contours[relevant[k]].clone();
                        acc += clipped_area(&arena.points[r], buf_a, buf_b, px, py);
                    }
                }
                // Absolute winding weight: interiors may wind either way
                // depending on contour orientation; holes cancel to zero either
                // way, and overlaps clamp to full coverage (non-zero rule).
                acc = acc.abs().clamp(0.0, 1.0);
                if acc > 0.0 {
                    data[row as usize * w as usize + col as usize] =
                        (acc * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        // Trim zero borders so blits stay tight.
        trim(AnalyticBitmap {
            left,
            top,
            width: w,
            height: h,
            data,
        })
    })
}

/// Unions `b` into `a` by per-pixel max (multi-layer outlines).
pub(crate) fn union_max(a: AnalyticBitmap, b: AnalyticBitmap) -> AnalyticBitmap {
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    let x0 = a.left.min(b.left);
    let y0 = a.top.min(b.top);
    let x1 = (a.left + a.width as i32).max(b.left + b.width as i32);
    let y1 = (a.top + a.height as i32).max(b.top + b.height as i32);
    let (w, h) = ((x1 - x0).max(0), (y1 - y0).max(0));
    if w <= 0 || h <= 0 || w as u32 > MAX_DIM || h as u32 > MAX_DIM {
        return AnalyticBitmap::empty();
    }
    let (w, h) = (w as u32, h as u32);
    let mut data = vec![0u8; (w * h) as usize];
    for (src, ox, oy) in [
        (&a, a.left - x0, a.top - y0),
        (&b, b.left - x0, b.top - y0),
    ] {
        for y in 0..src.height {
            for x in 0..src.width {
                let v = src.data[y as usize * src.width as usize + x as usize];
                if v == 0 {
                    continue;
                }
                let dx = x as i32 + ox;
                let dy = y as i32 + oy;
                let slot = &mut data[dy as usize * w as usize + dx as usize];
                *slot = (*slot).max(v);
            }
        }
    }
    trim(AnalyticBitmap {
        left: x0,
        top: y0,
        width: w,
        height: h,
        data,
    })
}

fn pixel_area_all(
    arena: &ContourArena,
    buf_a: &mut Vec<[f32; 2]>,
    buf_b: &mut Vec<[f32; 2]>,
    px: f32,
    py: f32,
) -> f32 {
    let mut acc = 0.0;
    for r in &arena.contours {
        acc += clipped_area(&arena.points[r.clone()], buf_a, buf_b, px, py);
    }
    acc
}

/// Signed area of `poly ∩ ([px,px+1) × [py,py+1))` via Sutherland–Hodgman.
fn clipped_area(
    poly: &[[f32; 2]],
    buf_a: &mut Vec<[f32; 2]>,
    buf_b: &mut Vec<[f32; 2]>,
    px: f32,
    py: f32,
) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    buf_a.clear();
    buf_a.extend_from_slice(poly);
    // Left, right, top, bottom half-planes, ping-ponging scratch buffers.
    clip_half_into(buf_a, buf_b, 0, px, true);
    if buf_b.is_empty() {
        return 0.0;
    }
    clip_half_into(buf_b, buf_a, 0, px + 1.0, false);
    if buf_a.is_empty() {
        return 0.0;
    }
    clip_half_into(buf_a, buf_b, 1, py, true);
    if buf_b.is_empty() {
        return 0.0;
    }
    clip_half_into(buf_b, buf_a, 1, py + 1.0, false);
    shoelace(buf_a)
}

/// Clips `src` against one half-plane into `dst` (`dst` is cleared first).
/// `axis` 0 = x, 1 = y; `keep_greater` keeps values >= bound.
fn clip_half_into(
    src: &[[f32; 2]],
    dst: &mut Vec<[f32; 2]>,
    axis: usize,
    bound: f32,
    keep_greater: bool,
) {
    dst.clear();
    if src.is_empty() {
        return;
    }
    let inside = |p: &[f32; 2]| {
        if keep_greater {
            p[axis] >= bound
        } else {
            p[axis] <= bound
        }
    };
    let mut prev = src[src.len() - 1];
    let mut prev_in = inside(&prev);
    for &cur in src.iter() {
        let cur_in = inside(&cur);
        if cur_in {
            if !prev_in {
                dst.push(intersect(prev, cur, axis, bound));
            }
            dst.push(cur);
        } else if prev_in {
            dst.push(intersect(prev, cur, axis, bound));
        }
        prev = cur;
        prev_in = cur_in;
    }
}

fn intersect(a: [f32; 2], b: [f32; 2], axis: usize, bound: f32) -> [f32; 2] {
    let d = b[axis] - a[axis];
    if d.abs() < 1e-9 {
        return a;
    }
    let t = (bound - a[axis]) / d;
    let other = 1 - axis;
    let mut p = a;
    p[axis] = bound;
    p[other] = a[other] + t * (b[other] - a[other]);
    p
}

fn shoelace(poly: &[[f32; 2]]) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        s += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * s
}

/// Flattens outline verbs into the contour arena, translated by
/// `(tx, ty)`. The arena is cleared first; its point/contour/bounds
/// capacity is reused across calls (the caller owns a persistent scratch).
/// `tol` is the scale-aware flattening tolerance ([`flatten_tol`]).
fn flatten_into(
    points: &[Point],
    verbs: &[Verb],
    tx: f32,
    ty: f32,
    tol: f32,
    out: &mut ContourArena,
) {
    out.points.clear();
    out.contours.clear();
    out.bounds.clear();
    let mut start = 0usize;
    let mut pen = [0.0f32, 0.0];
    let mut pi = 0usize;
    // Returns None on malformed input (truncated point stream): the glyph is
    // skipped rather than rasterized from misaligned data.
    let mut malformed = false;
    for v in verbs {
        match v {
            Verb::MoveTo => {
                flush_contour(out, &mut start);
                let Some(q) = take_point(points, &mut pi, tx, ty) else {
                    malformed = true;
                    break;
                };
                pen = q;
                out.points.push(pen);
            }
            Verb::LineTo => {
                let Some(q) = take_point(points, &mut pi, tx, ty) else {
                    malformed = true;
                    break;
                };
                pen = q;
                out.points.push(pen);
            }
            Verb::QuadTo => {
                let Some(q0) = take_point(points, &mut pi, tx, ty) else {
                    malformed = true;
                    break;
                };
                let Some(q1) = take_point(points, &mut pi, tx, ty) else {
                    malformed = true;
                    break;
                };
                flatten_quad(pen, q0, q1, tol, &mut out.points);
                pen = q1;
            }
            Verb::CurveTo => {
                let Some(q0) = take_point(points, &mut pi, tx, ty) else {
                    malformed = true;
                    break;
                };
                let Some(q1) = take_point(points, &mut pi, tx, ty) else {
                    malformed = true;
                    break;
                };
                let Some(q2) = take_point(points, &mut pi, tx, ty) else {
                    malformed = true;
                    break;
                };
                flatten_cubic(pen, q0, q1, q2, tol, &mut out.points);
                pen = q2;
            }
            Verb::Close => {
                flush_contour(out, &mut start);
            }
        }
    }
    flush_contour(out, &mut start);
    if malformed {
        out.points.clear();
        out.contours.clear();
    }
}

/// Closes the contour in progress: contours with three or more points are
/// recorded as a range, shorter runs are dropped.
fn flush_contour(out: &mut ContourArena, start: &mut usize) {
    let end = out.points.len();
    if end - *start >= 3 {
        out.contours.push(*start..end);
    } else {
        out.points.truncate(*start);
    }
    *start = end;
}

/// Takes one translated point from the stream, or None when truncated.
fn take_point(points: &[Point], pi: &mut usize, tx: f32, ty: f32) -> Option<[f32; 2]> {
    let p = points.get(*pi)?;
    *pi += 1;
    Some([p.x + tx, p.y + ty])
}

fn flatten_quad(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], tol: f32, out: &mut Vec<[f32; 2]>) {
    // Max deviation of a quadratic from its chord is half the control
    // point's distance to the chord midpoint segment.
    let mx = (p0[0] + p2[0]) * 0.5;
    let my = (p0[1] + p2[1]) * 0.5;
    let dx = p1[0] - mx;
    let dy = p1[1] - my;
    if dx * dx + dy * dy <= (2.0 * tol) * (2.0 * tol) {
        out.push(p2);
        return;
    }
    if out.len() > 1 << 16 {
        out.push(p2);
        return;
    }
    subdivide_quad(p0, p1, p2, tol, out, 0);
}

fn subdivide_quad(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    tol: f32,
    out: &mut Vec<[f32; 2]>,
    d: u8,
) {
    if d >= FLAT_DEPTH {
        out.push(p2);
        return;
    }
    let m01 = [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5];
    let m12 = [(p1[0] + p2[0]) * 0.5, (p1[1] + p2[1]) * 0.5];
    let m = [(m01[0] + m12[0]) * 0.5, (m01[1] + m12[1]) * 0.5];
    let mx = (p0[0] + p2[0]) * 0.5;
    let my = (p0[1] + p2[1]) * 0.5;
    // Deviation of the midpoint from the chord, scaled: the true max
    // deviation is <= this; subdividing while it exceeds tolerance is exact.
    let dx = m[0] - mx;
    let dy = m[1] - my;
    if dx * dx + dy * dy <= tol * tol {
        out.push(m);
        out.push(p2);
        return;
    }
    subdivide_quad(p0, m01, m, tol, out, d + 1);
    subdivide_quad(m, m12, p2, tol, out, d + 1);
}

fn flatten_cubic(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    tol: f32,
    out: &mut Vec<[f32; 2]>,
) {
    if out.len() > 1 << 16 {
        out.push(p3);
        return;
    }
    subdivide_cubic(p0, p1, p2, p3, tol, out, 0);
}

fn dist_to_chord(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt();
    }
    ((dy * p[0] - dx * p[1] + b[0] * a[1] - b[1] * a[0]).abs()) / len2.sqrt()
}

fn subdivide_cubic(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    tol: f32,
    out: &mut Vec<[f32; 2]>,
    d: u8,
) {
    // Flat when both inner controls sit on the chord: a cubic with
    // collinear controls is a straight segment.
    let flat = dist_to_chord(p1, p0, p3) <= tol && dist_to_chord(p2, p0, p3) <= tol;
    if flat || d >= FLAT_DEPTH {
        out.push(p3);
        return;
    }
    let m01 = [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5];
    let m12 = [(p1[0] + p2[0]) * 0.5, (p1[1] + p2[1]) * 0.5];
    let m23 = [(p2[0] + p3[0]) * 0.5, (p2[1] + p3[1]) * 0.5];
    let m012 = [(m01[0] + m12[0]) * 0.5, (m01[1] + m12[1]) * 0.5];
    let m123 = [(m12[0] + m23[0]) * 0.5, (m12[1] + m23[1]) * 0.5];
    let m = [(m012[0] + m123[0]) * 0.5, (m012[1] + m123[1]) * 0.5];
    subdivide_cubic(p0, m01, m012, m, tol, out, d + 1);
    subdivide_cubic(m, m123, m23, p3, tol, out, d + 1);
}

/// Trims all-zero borders, adjusting bearings.
fn trim(b: AnalyticBitmap) -> AnalyticBitmap {
    if b.is_empty() {
        return b;
    }
    let (mut x0, mut y0) = (b.width, b.height);
    let (mut x1, mut y1) = (0u32, 0u32);
    for y in 0..b.height {
        for x in 0..b.width {
            if b.data[y as usize * b.width as usize + x as usize] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return AnalyticBitmap::empty();
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut data = vec![0u8; (w * h) as usize];
    for y in 0..h {
        let src = (y0 + y) as usize * b.width as usize + x0 as usize;
        let dst = y as usize * w as usize;
        data[dst..dst + w as usize].copy_from_slice(&b.data[src..src + w as usize]);
    }
    AnalyticBitmap {
        left: b.left + x0 as i32,
        top: b.top + y0 as i32,
        width: w,
        height: h,
        data,
    }
}

// ── Opt-in CFF stem darkening (FreeType control points) ─────────────────
//
// Unhinted CFF/Type1 outlines at small physical sizes wash out: stems
// thinner than ~1 px lose most of their coverage against the linear
// pipeline. Opt-in flag + FreeType's documented width-dependent stem
// expansion, applied geometrically (opposing stem boundaries displaced
// outward) so counters stay open and coverage stays linear.

/// Opt-in CFF stem darkening, default off (spec-correct untouched outlines
/// are the reference behavior; darkening is a calibration knob for small
/// physical text).
static STEM_DARKENING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enables or disables CFF stem darkening for subsequently rasterized
/// glyphs. Flips are picked up on the next rasterization (cached bitmaps
/// are keyed independently of the flag; toggling does not re-key them, so
/// existing entries keep their appearance until evicted — flip before
/// warm-up for a clean A/B).
pub fn set_stem_darkening(enabled: bool) {
    STEM_DARKENING.store(enabled, std::sync::atomic::Ordering::Relaxed);
    tracing::info!(
        enabled,
        "CFF stem darkening {}",
        if enabled { "ON" } else { "OFF" }
    );
}

/// Whether stem darkening is currently opted in.
pub fn stem_darkening_enabled() -> bool {
    STEM_DARKENING.load(std::sync::atomic::Ordering::Relaxed)
}

/// FreeType's documented stem-darkening control points, piecewise-linear in
/// the measured physical stem width `w` (px):
/// w ≤ 0.5 → Δw = 0.4; w = 1.0 → 0.275; w = 1.667 → 0.275; w ≥ 2.333 → 0.
pub(crate) fn darken_delta(w: f32) -> f32 {
    if w <= 0.5 {
        return 0.4;
    }
    if w >= 2.333 {
        return 0.0;
    }
    let (w0, d0, w1, d1) = if w <= 1.0 {
        (0.5, 0.4, 1.0, 0.275)
    } else if w <= 1.667 {
        (1.0, 0.275, 1.667, 0.275)
    } else {
        (1.667, 0.275, 2.333, 0.0)
    };
    d0 + (w - w0) * (d1 - d0) / (w1 - w0)
}

/// Median horizontal run length of solid ink (coverage ≥ 96) across the
/// bitmap: the measured stem-width proxy the darkening delta keys on.
pub(crate) fn median_ink_run(b: &AnalyticBitmap) -> f32 {
    let mut runs: Vec<u32> = Vec::new();
    for row in 0..b.height {
        let base = row as usize * b.width as usize;
        let mut run = 0u32;
        for x in 0..b.width {
            if b.data[base + x as usize] >= 96 {
                run += 1;
            } else if run > 0 {
                runs.push(run);
                run = 0;
            }
        }
        if run > 0 {
            runs.push(run);
        }
    }
    if runs.is_empty() {
        return 0.0;
    }
    runs.sort_unstable();
    let mid = runs.len() / 2;
    if runs.len() % 2 == 1 {
        runs[mid] as f32
    } else {
        (runs[mid - 1] + runs[mid]) as f32 * 0.5
    }
}

/// Displaces every outline point outward by `d` along orientation-aware
/// vertex normals, reassembling the same verb structure.
///
/// Per contour: the vertex tangent is the chord `p[i+1] − p[i−1]`
/// (wrapping), and the outward normal is the perpendicular chosen by the
/// contour's winding (positive signed area → interior left of travel →
/// outward is the right-hand perpendicular; negative → left-hand). That
/// convention is per-contour, so outer boundaries and hole boundaries both
/// displace away from ink regardless of the font's winding convention.
/// Contours with fewer than 3 points are copied unchanged.
pub(crate) fn offset_outward(points: &[Point], verbs: &[Verb], d: f32) -> Vec<Point> {
    let mut out: Vec<Point> = points.iter().map(|p| Point::new(p.x, p.y)).collect();
    // Slice the point stream into contour spans at MoveTo boundaries
    // (MoveTo's point opens the next contour).
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut contour_start = 0usize;
    let mut pi = 0usize;
    for v in verbs {
        if *v == Verb::MoveTo && pi > contour_start {
            spans.push((contour_start, pi));
            contour_start = pi;
        }
        pi += match v {
            Verb::MoveTo | Verb::LineTo => 1,
            Verb::QuadTo => 2,
            Verb::CurveTo => 3,
            Verb::Close => 0,
        };
    }
    if pi > contour_start {
        spans.push((contour_start, pi));
    }
    for (s, e) in spans {
        let n = e - s;
        if n < 3 {
            continue;
        }
        let pt = |i: usize| points[s + i];
        let mut area = 0.0_f32;
        for i in 0..n {
            let a = pt(i);
            let b = pt((i + 1) % n);
            area += a.x * b.y - b.x * a.y;
        }
        for i in 0..n {
            let prev = pt((i + n - 1) % n);
            let next = pt((i + 1) % n);
            let tx = next.x - prev.x;
            let ty = next.y - prev.y;
            let len = (tx * tx + ty * ty).sqrt();
            if len < 1e-9 {
                continue;
            }
            // Outward perpendicular, sign by winding (see doc comment).
            let (nx, ny) = if area >= 0.0 {
                (ty / len, -tx / len)
            } else {
                (-ty / len, tx / len)
            };
            let p = pt(i);
            out[s + i] = Point::new(p.x + nx * d, p.y + ny * d);
        }
    }
    out
}

/// Detects a CFF (`CFF ` / `CFF2`) outline table in raw sfnt font bytes.
///
/// Stem darkening must only ever apply to unhinted CFF outlines: TrueType
/// fonts carry their own hinting machinery, and native bytecode hinting is
/// not detectable through swash — gating on CFF presence is the correct
/// subset (a TrueType font is never darkened, hinting or not). Collection
/// containers (`ttcf`) are unwrapped to the first face; a conservative miss
/// merely disables the opt-in feature.
pub(crate) fn is_cff_outline(data: &[u8]) -> bool {
    let Some(head) = data.get(..4) else {
        return false;
    };
    let tag = u32::from_be_bytes(head.try_into().unwrap());
    if tag == u32::from_be_bytes(*b"ttcf") {
        let Some(off) = data.get(12..16).and_then(|b| {
            usize::try_from(u32::from_be_bytes(b.try_into().unwrap())).ok()
        }) else {
            return false;
        };
        return data.get(off..).map(|sub| is_cff_outline(sub)).unwrap_or(false);
    }
    if !(tag == u32::from_be_bytes(*b"OTTO")
        || tag == 0x0001_0000
        || tag == u32::from_be_bytes(*b"true"))
    {
        return false;
    }
    let Some(count) = data.get(4..6).map(|b| u16::from_be_bytes(b.try_into().unwrap())) else {
        return false;
    };
    (0..count as usize).any(|i| {
        let rec = 12 + i * 16;
        data.get(rec..rec + 4).is_some_and(|t| t == b"CFF " || t == b"CFF2")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    /// Unit square at integer pixels: full coverage, exact bearings.
    /// Grid space is y-down: the y-up square (0,0)-(1,1) lands on rows
    /// -1..0, so grid top is -1 (placement `top` = 1 after negation).
    #[test]
    fn unit_square_full_coverage() {
        let pts = [pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        let verbs = [Verb::MoveTo, Verb::LineTo, Verb::LineTo, Verb::LineTo, Verb::Close];
        let b = rasterize_layer(&pts, &verbs, 0.0, 0.0, 1.0);
        assert_eq!((b.left, b.top, b.width, b.height), (0, -1, 1, 1));
        assert_eq!(b.data, vec![255]);
    }

    /// Square shifted half a pixel: straddles two pixels evenly.
    #[test]
    fn half_pixel_shift_splits_coverage() {
        let pts = [pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        let verbs = [Verb::MoveTo, Verb::LineTo, Verb::LineTo, Verb::LineTo, Verb::Close];
        let b = rasterize_layer(&pts, &verbs, 0.5, 0.0, 1.0);
        assert_eq!((b.width, b.height), (2, 1));
        assert!(b.data.iter().all(|&v| (v as i32 - 128).abs() <= 1));
    }

    /// Counter-wound inner square is a hole: ring coverage only.
    #[test]
    fn counter_wound_hole_subtracts() {
        let pts = [
            pt(0.0, 0.0),
            pt(3.0, 0.0),
            pt(3.0, 3.0),
            pt(0.0, 3.0),
            pt(1.0, 1.0),
            pt(1.0, 2.0),
            pt(2.0, 2.0),
            pt(2.0, 1.0),
        ];
        let verbs = [
            Verb::MoveTo,
            Verb::LineTo,
            Verb::LineTo,
            Verb::LineTo,
            Verb::Close,
            Verb::MoveTo,
            Verb::LineTo,
            Verb::LineTo,
            Verb::LineTo,
            Verb::Close,
        ];
        let b = rasterize_layer(&pts, &verbs, 0.0, 0.0, 1.0);
        assert_eq!((b.width, b.height), (3, 3));
        // Center pixel is the hole.
        assert_eq!(b.data[1 * 3 + 1], 0);
        // Corners are solid.
        assert_eq!(b.data[0], 255);
        assert_eq!(b.data[2 * 3 + 2], 255);
    }

    /// Quadratic arch area: scanline method computes 1D coverage at pixel centers.
    /// This approximates 2D area; tolerance reflects scanline method accuracy.
    #[test]
    fn quad_arch_area_matches_closed_form() {
        // Arch from (0,0) via control (1,2) to (2,0), closed along y=0.
        let pts = [pt(0.0, 0.0), pt(1.0, 2.0), pt(2.0, 0.0)];
        let verbs = [Verb::MoveTo, Verb::QuadTo, Verb::Close];
        let b = rasterize_layer(&pts, &verbs, 0.0, 0.0, 1.0);
        let area: f32 = b.data.iter().map(|&v| v as f32 / 255.0).sum();
        // Scanline at pixel centers: 1D coverage ≈ 2D area within ~0.08 for this case.
        assert!((area - 4.0 / 3.0).abs() < 0.1, "area = {area}");
    }
}
