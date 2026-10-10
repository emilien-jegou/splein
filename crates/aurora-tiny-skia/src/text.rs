// Single responsibility: Glyph bitmap Porter-Duff alpha compositing via TinySkia.

use aurora::foundation::{Color, Point};
use aurora::text::{TextContext, TextLayout};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tiny_skia::{
    FilterQuality, Mask, Pixmap, PixmapMut, PixmapPaint, PremultipliedColorU8, Transform,
};

/// Parameters for typography run rasterization.
pub struct TextRenderParams<'a> {
    /// Translation offset in destination buffer.
    pub origin: Point,
    /// Shaped text layout data.
    pub layout: &'a TextLayout,
    /// Retained layout handle, enabling the line composite cache below.
    ///
    /// `None` disables that cache (per-glyph path only); the transformed
    /// backend ignores it either way.
    pub layout_arc: Option<Arc<TextLayout>>,
    /// Text foreground color.
    pub color: Color,
    /// Global opacity multiplier.
    pub opacity: f32,
    /// Active coordinate transformation.
    pub transform: Transform,
    /// Clipping mask, if active.
    pub clip: Option<&'a Mask>,
    /// Retained per-line composited bitmaps for static runs, if enabled.
    pub line_cache: Option<&'a mut LineCompositeCache>,
}

/// Identity of one cached composited line: same shaped content, same ink,
/// same device integer origin. The clip is deliberately excluded — it applies
/// at draw time, so one entry serves every clip state.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct LineKey {
    /// `Arc::as_ptr` of the retained layout: the entry holds the `Arc`, so
    /// the address cannot be recycled under a live key.
    ptr: usize,
    line: u32,
    ink: [u8; 3],
    opacity_q: u32,
    ox: i32,
    oy: i32,
}

/// One composited line bitmap plus its device integer origin.
struct LineEntry {
    layout: Arc<TextLayout>,
    pixmap: Pixmap,
    used: u64,
    bytes: usize,
}

/// Retained per-line composited bitmaps for static text runs.
///
/// A static run (integer device origin) composites once and replays with a
/// single draw; any motion bypasses to the per-glyph glide path without
/// disturbing the cache, so settling back reuses the entry. Bounded by bytes
/// (default 8 MiB) with oldest-used eviction; a missed invalidation would
/// paint stale text, but keys carry layout identity, ink, opacity, and exact
/// origin, and Clip applies at draw time rather than bake time.
pub struct LineCompositeCache {
    entries: FxHashMap<LineKey, LineEntry>,
    clock: u64,
    bytes: usize,
    cap: usize,
}

/// Default byte budget: ~180 typical body lines resident.
pub const LINE_CACHE_CAP_BYTES: usize = 8 << 20;
/// Bound on entry count so eviction scans stay trivial.
const LINE_CACHE_MAX_ENTRIES: usize = 2048;

impl LineCompositeCache {
    /// Constructs an empty cache under the default budget.
    pub fn new() -> Self {
        Self::with_cap(LINE_CACHE_CAP_BYTES)
    }

    /// Constructs an empty cache under an explicit byte budget.
    pub fn with_cap(cap: usize) -> Self {
        Self {
            entries: FxHashMap::default(),
            clock: 0,
            bytes: 0,
            cap: cap.max(1),
        }
    }

    /// Marks a composited line most-recently-used; true on hit.
    ///
    /// Split from [`Self::pixmap`] so the draw below never borrows the cache:
    /// touch first, release, then read the resident pixmap.
    fn touch(&mut self, key: &LineKey) -> bool {
        self.clock += 1;
        let clock = self.clock;
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        // The key carries the pointer and the entry holds the `Arc`: a hit
        // is the same object, never a recycled address.
        debug_assert_eq!(Arc::as_ptr(&entry.layout) as usize, key.ptr);
        entry.used = clock;
        true
    }

    /// Accesses the resident pixmap for a key just returned by [`Self::get`].
    fn pixmap(&self, key: &LineKey) -> Option<&Pixmap> {
        self.entries.get(key).map(|e| &e.pixmap)
    }

    /// Stores a freshly composited line, evicting oldest-used past the budget.
    fn insert(&mut self, key: LineKey, layout: Arc<TextLayout>, pixmap: Pixmap) {
        let bytes = pixmap.width() as usize * pixmap.height() as usize * 4
            + std::mem::size_of::<LineEntry>();
        // Oversized single lines bypass rather than emptying the cache.
        if bytes > self.cap {
            return;
        }
        self.clock += 1;
        if let Some(old) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes += bytes;
        self.entries.insert(
            key,
            LineEntry {
                layout,
                pixmap,
                used: self.clock,
                bytes,
            },
        );
        while self.bytes > self.cap || self.entries.len() > LINE_CACHE_MAX_ENTRIES {
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| *k);
            match victim {
                Some(k) => {
                    if let Some(old) = self.entries.remove(&k) {
                        self.bytes = self.bytes.saturating_sub(old.bytes);
                    }
                }
                None => break,
            }
        }
    }
}

impl Default for LineCompositeCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Renders shaped typography glyph runs directly into a destination pixel buffer.
pub fn render_text(pixmap: &mut PixmapMut, text_ctx: &TextContext, params: TextRenderParams) {
    if params.layout.lines.is_empty() {
        return;
    }

    // Skewed or scaled transforms change glyph shapes, not just their origins,
    // so those go through the backend's transformed blit. Pure translations
    // keep the pixel-snapped fast path below, bit-identical to before.
    if params.transform.has_skew() || params.transform.has_scale() {
        render_text_transformed(pixmap, text_ctx, params);
        return;
    }

    render_text_aligned(pixmap, text_ctx, params);
}

/// Display gamma assumed for coverage-to-alpha conversion.
///
/// Coverage masks are linear-light fractions, but compositing happens in
/// sRGB-encoded space. Blending raw coverage as alpha darkens mid-tone fringe
/// pixels (stems render heavier than specified). The LUT below folds the
/// transfer function in: `alpha = 1 - (1 - coverage)^(1/gamma)`, so a
/// half-covered pixel blends to the sRGB encoding of half light instead of
/// half ink. Monotonic, so threshold tests and bounding boxes are unaffected.
pub const COVERAGE_GAMMA: f32 = 2.2;

/// Gamma table at full opacity, computed once: 256 `powf` calls per `DrawText`
/// showed up as a real cost with many small runs on screen, so the curve is
/// process-wide and each run only folds in its scalar opacity (integer math).
static GAMMA_LUT: std::sync::LazyLock<[u8; 256]> = std::sync::LazyLock::new(|| {
    let inv_gamma = 1.0 / COVERAGE_GAMMA;
    std::array::from_fn(|i| {
        let linear = 1.0 - (1.0 - i as f32 / 255.0).powf(inv_gamma);
        (linear * 255.0).round() as u8
    })
});

/// Subpixel X phase bins per pixel: the pen quantizes to quarters and the
/// rasterizer produces one coverage variant per bin (see `raster_glyph`).
/// Animation cross-fades adjacent bins (see `blend_phase_coverages`), so
/// horizontal motion stays continuous while every source bitmap is
/// phase-correct.
pub const PHASE_BINS: u8 = 4;

/// Subpixel Y phase bins per pixel: baselines snap to the nearest eighth.
/// Eighth-pixel vertical steps hide in motion as whole-pixel ones never do,
/// while static text keeps a single crisp bitmap (blending Y measurably
/// softened it: the Y fraction is almost never exactly zero).
pub const PHASE_BINS_Y: u8 = 8;

/// Split a pen position into an integer base plus the fractional position
/// inside the pixel, in quarter-pixel units: `frac4` in [0, 4).
fn split_phase(pos: f32) -> (i32, f32) {
    let floor = pos.floor();
    (floor as i32, (pos - floor) * PHASE_BINS as f32)
}

/// Split a pen position into an integer base plus the fractional position
/// inside the pixel, in eighth-pixel units: `frac8` in [0, 8).
fn split_phase_y(pos: f32) -> (i32, f32) {
    let floor = pos.floor();
    (floor as i32, (pos - floor) * PHASE_BINS_Y as f32)
}

/// Blend two adjacent phase-bin coverages onto a common grid.
///
/// Kept for reference / animated cross-fade experiments; the aligned path no
/// longer blends (it snaps to the nearest bin) because alpha-mixing two
/// integer placements leaves half-intensity edge tears.
#[allow(dead_code)]
fn blend_phase_coverages(
    // `a` is the bitmap for bin `q` placed at `(ax, ay)`, `b` the bitmap
    a: &[u8],
    aw: u32,
    ah: u32,
    ax: i32,
    ay: i32,
    w0: f32,
    b: &[u8],
    bw: u32,
    bh: u32,
    bx: i32,
    by: i32,
    w1: f32,
    out: &mut Vec<u8>,
) -> (i32, i32, u32, u32) {
    let x0 = ax.min(bx);
    let y0 = ay.min(by);
    let x1 = (ax + aw as i32).max(bx + bw as i32);
    let y1 = (ay + ah as i32).max(by + bh as i32);
    let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    out.clear();
    out.resize(w * h, 0);
    if w == 0 || h == 0 {
        return (x0, y0, 0, 0);
    }
    let at = |data: &[u8], dw: u32, ox: i32, oy: i32, x: usize, y: usize| -> f32 {
        let lx = x as i32 + x0 - ox;
        let ly = y as i32 + y0 - oy;
        if lx < 0 || ly < 0 {
            return 0.0;
        }
        data.get(ly as usize * dw as usize + lx as usize)
            .copied()
            .unwrap_or(0) as f32
    };
    for y in 0..h {
        for x in 0..w {
            let v = at(a, aw, ax, ay, x, y) * w0 + at(b, bw, bx, by, x, y) * w1;
            out[y * w + x] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    (x0, y0, w as u32, h as u32)
}

/// Renders glyph runs for translation-only transforms.
///
/// Axis-split phase + owned subpixel blend: X rasterizes at its nearest
/// quarter bin (phase-matched grid-fit stems stay whole), Y rasterizes frozen
/// at bin 0 (no per-eighth variant swaps re-quantizing horizontal features
/// mid-motion). The fractional remainder is blended explicitly into the
/// scratch buffer (bilinear over the coverage alphas) and placed at integer
/// pixels — no fractional draw call exists anywhere in this path, so no
/// backend filtering behavior can quantize, snap, or otherwise reshape one
/// axis differently from the other. At integer pens output matches the old
/// exact integer blit.
///
/// Cost notes (all output-identical): alphas fold opacity/gamma once per run,
/// one grow-only scratch pixmap is reused across glyphs (no per-glyph alloc),
/// coverage is blended straight out of the cache handle (no staging copy),
/// and exactly-integral pens take the integer `blit_glyph` fast path.
fn render_text_aligned(pixmap: &mut PixmapMut, text_ctx: &TextContext, params: TextRenderParams) {
    let ink_r = (params.color.r * 255.0).round() as u8;
    let ink_g = (params.color.g * 255.0).round() as u8;
    let ink_b = (params.color.b * 255.0).round() as u8;
    let total_opacity = params.opacity * params.color.a;
    if total_opacity <= 0.0 {
        return;
    }

    // Coverage-to-alpha tables for this run: small text composites raw
    // coverage (density carries grid-fit stems), display type folds the
    // gamma curve in. Threshold mirrors the hinter cutoff in aurora
    // (`SMALL_TEXT_HINT_CUTOFF_PX`): hinted bitmaps want linear density,
    // linear outlines want corrected fringes.
    let opacity_q = (total_opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    let linear_lut: [u8; 256] = std::array::from_fn(|i| ((i as u32 * opacity_q + 127) / 255) as u8);
    let gamma_lut: [u8; 256] =
        std::array::from_fn(|i| ((GAMMA_LUT[i] as u32 * opacity_q + 127) / 255) as u8);
    let paint = PixmapPaint {
        opacity: 1.0,
        // Nearest: placement below is always integral (fractions are blended
        // into the scratch by hand), so no backend interpolation is wanted.
        quality: FilterQuality::Nearest,
        ..Default::default()
    };
    // Mask dimensions hoisted out of the per-glyph path.
    let (clip_w, clip_h) = match params.clip {
        Some(mask) => (mask.width(), mask.height()),
        None => (0, 0),
    };

    let origin = params.origin;
    let transform = params.transform;
    let (ttx, tty) = (transform.tx, transform.ty);
    let clip = params.clip;
    let layout: &TextLayout = params.layout;
    let layout_arc = params.layout_arc;
    let mut line_cache = params.line_cache;
    // Paints one line's glyphs into `target`. Glyph device positions are
    // rebased onto integral `space` for the blit fast path and onto `draw`
    // for the draw call under `tx` with clip `clip_now`: canvas passes
    // space (0, 0) (tile pixels), draw (ttx, tty) (layout space); the
    // line-cache path passes the pixmap origin for both with identity and no
    // clip (the clip applies once, when the composited line is drawn).
    // Captures only the shaper handle, the shared scratch, and scalars —
    // never the canvas or the line cache.
    let mut scratch: Option<Pixmap> = None;
    let mut paint_line = |target: &mut PixmapMut,
                          line: &aurora::text::ShapedLine,
                          alpha_lut: &[u8; 256],
                          space_x: f32,
                          space_y: f32,
                          draw_x: f32,
                          draw_y: f32,
                          tx: Transform,
                          clip_now: Option<&Mask>,
                          clip_now_w: u32,
                          clip_now_h: u32| {
        for g in &line.glyphs {
            // Device pen = layout pen + canvas translation; positions below
            // are rebased onto `space`/`draw`, so the same code paints canvas
            // space and line-local space.
            let dev_x = origin.x + line.align_offset + g.point.x + ttx;
            let dev_y = origin.y + g.point.y + tty;
            // X: nearest quarter bin (wrap bin 4 to bin 0 of the next pixel);
            // the swash x-offset is baked into the bitmap, placement backs it
            // out so the blend carries <=1/8px.
            let (base_x, frac4x) = split_phase(dev_x);
            let q = (frac4x + 0.5).floor() as i32;
            let (_snap_x, binx) = if q >= PHASE_BINS as i32 {
                (base_x + 1, 0u8)
            } else {
                (base_x, q.max(0) as u8)
            };
            // Y: frozen at bin 0 — one constant bitmap per glyph, the full
            // fraction rides the blend. Per-eighth variants re-quantize
            // horizontal features on different frames per letter (morph).
            let biny = 0u8;

            // Bitmap baked with (binx*0.25, 0); the exact device origin is
            // the pen minus baked offset plus bitmap origin. The integer
            // part places (rebased onto `sub`); the fraction is blended
            // explicitly below, so no draw call carries a fraction.
            // Exactly-integral pens skip tint + blend via `blit_glyph`.
            let mut placed: Option<(i32, i32, u32, u32)> = None;
            let mut drawn = false;
            text_ctx.raster_glyph(
                g.cache_key,
                binx | (biny << 2),
                |img_opt| {
                    let Some(img) = img_opt else {
                        return;
                    };
                    let (w, h) = (img.width, img.height);
                    if w == 0 || h == 0 || img.data.len() < (w * h) as usize {
                        return;
                    }
                    let gx_exact = dev_x - binx as f32 * 0.25 + img.left as f32;
                    let gy_exact = dev_y - img.top as f32;
                    if gx_exact.fract() == 0.0 && gy_exact.fract() == 0.0 {
                        let (pw, ph) = (target.width(), target.height());
                        blit_glyph(
                            target.pixels_mut(),
                            pw,
                            ph,
                            img.data,
                            w,
                            h,
                            gx_exact as i32 - space_x as i32,
                            gy_exact as i32 - space_y as i32,
                            ink_r,
                            ink_g,
                            ink_b,
                            alpha_lut,
                            clip_now,
                            clip_now_w,
                            clip_now_h,
                        );
                        drawn = true;
                        return;
                    }
                    // Owned bilinear shift of the coverage alphas into the
                    // reused scratch (no staging copy, no backend filtering):
                    // dest (i, j) samples source at (i - fx, j - fy), zero
                    // outside the bitmap. Scratch is one pixel wider/taller
                    // than the source to hold the shifted footprint.
                    let ix = gx_exact.floor() as i32;
                    let iy = gy_exact.floor() as i32;
                    let (fx, fy) = (gx_exact - ix as f32, gy_exact - iy as f32);
                    let (sw, sh) = (w + 1, h + 1);
                    let reuse = scratch.as_ref().is_some_and(|s| {
                        s.width() == sw && s.height() == sh
                    });
                    if !reuse {
                        scratch = Pixmap::new(sw, sh);
                    }
                    let Some(s) = scratch.as_mut() else {
                        return;
                    };
                    s.fill(tiny_skia::Color::TRANSPARENT);
                    let cov_at = |x: i32, y: i32| -> f32 {
                        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                            0.0
                        } else {
                            alpha_lut[img.data[y as usize * w as usize + x as usize] as usize]
                                as f32
                        }
                    };
                    let dst = s.pixels_mut();
                    for j in 0..sh as i32 {
                        let sy = j as f32 - fy;
                        let y0 = sy.floor() as i32;
                        let wy = sy - y0 as f32;
                        for i in 0..sw as i32 {
                            let sx = i as f32 - fx;
                            let x0 = sx.floor() as i32;
                            let wx = sx - x0 as f32;
                            let a = (cov_at(x0, y0) * (1.0 - wx)
                                + cov_at(x0 + 1, y0) * wx)
                                * (1.0 - wy)
                                + (cov_at(x0, y0 + 1) * (1.0 - wx)
                                    + cov_at(x0 + 1, y0 + 1) * wx)
                                    * wy;
                            let ai = a.round() as u8;
                            if ai == 0 {
                                continue;
                            }
                            let tint =
                                |ch: u8| ((ch as u32 * ai as u32 + 127) / 255) as u8;
                            if let Some(px) = PremultipliedColorU8::from_rgba(
                                tint(ink_r),
                                tint(ink_g),
                                tint(ink_b),
                                ai,
                            ) {
                                dst[j as usize * sw as usize + i as usize] = px;
                            }
                        }
                    }
                    placed = Some((ix, iy, sw, sh));
                },
            );
            if drawn {
                continue;
            }
            let Some((gx, gy, _w, _h)) = placed else {
                continue;
            };
            let Some(s) = scratch.as_ref() else {
                continue;
            };
            // Integer translate only: the backend performs no interpolation.
            target.draw_pixmap(
                0,
                0,
                s.as_ref(),
                &paint,
                tx.pre_translate(gx as f32 - draw_x, gy as f32 - draw_y),
                clip_now,
            );
        }
    };
    for (idx, line) in layout.lines.iter().enumerate() {
        if line.glyphs.is_empty() {
            continue;
        }
        let alpha_lut: &[u8; 256] = if line.font_size <= aurora::text::SMALL_TEXT_HINT_CUTOFF_PX {
            &linear_lut
        } else {
            &gamma_lut
        };
        let dev_lx = origin.x + line.align_offset + ttx;
        let dev_top = origin.y + tty + idx as f32 * line.height;
        let integral = dev_lx.fract() == 0.0 && dev_top.fract() == 0.0 && line_cache.is_some();
        if integral {
            // Static run at an integer origin: replay or populate the
            // composited line. Any motion bypasses below without disturbing
            // the cache, so settling back reuses the entry.
            let ink = [ink_r, ink_g, ink_b];
            // Glyph pen cluster in line-local terms; pads cover bitmap
            // overhang (bearings, italic slant), the 1px blend footprint,
            // and ascender/descender spill beyond the pen cluster.
            let size = line.font_size;
            let (pad_x, pad_t, pad_b) = (
                (size * 0.25).ceil() as i32 + 2,
                size.ceil() as i32 + 2,
                (size * 0.5).ceil() as i32 + 2,
            );
            let (mut minx, mut maxx, mut miny, mut maxy) = (
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
            );
            for g in &line.glyphs {
                minx = minx.min(g.point.x);
                maxx = maxx.max(g.point.x + g.advance);
                miny = miny.min(g.point.y);
                maxy = maxy.max(g.point.y);
            }
            // Line-local box top in layout terms: the uniform stack position.
            let top_local = idx as f32 * line.height;
            let ox = (dev_lx + minx).floor() as i32 - pad_x;
            let oy = (dev_top + (miny - top_local)).floor() as i32 - pad_t;
            let lsw = ((maxx - minx).ceil() as i32 + 2 * pad_x).max(1) as u32;
            let lsh = ((maxy - miny).ceil() as i32 + pad_t + pad_b).max(1) as u32;
            if let (Some(cache), Some(owner)) = (line_cache.as_mut(), layout_arc.as_ref()) {
                let key = LineKey {
                    ptr: Arc::as_ptr(owner) as usize,
                    line: idx as u32,
                    ink,
                    opacity_q,
                    ox,
                    oy,
                };
                if cache.touch(&key) {
                    let entry = cache.pixmap(&key).expect("key just hit");
                    let (ew, eh) = (entry.width(), entry.height());
                    debug_assert_eq!((ew, eh), (lsw, lsh));
                    if (ew, eh) == (lsw, lsh) {
                        pixmap.draw_pixmap(
                            0,
                            0,
                            entry.as_ref(),
                            &paint,
                            transform.pre_translate(ox as f32 - ttx, oy as f32 - tty),
                            clip,
                        );
                        continue;
                    }
                }
                // Miss: composite into a fresh pixmap (rare: once per static
                // configuration), replay it this frame, and retain it.
                let mut fresh = match Pixmap::new(lsw, lsh) {
                    Some(p) => p,
                    None => {
                        paint_line(
                            pixmap, line, alpha_lut, 0.0, 0.0, ttx, tty, transform, clip, clip_w,
                            clip_h,
                        );
                        continue;
                    }
                };
                {
                    let mut fresh_mut = fresh.as_mut();
                    paint_line(
                        &mut fresh_mut,
                        line,
                        alpha_lut,
                        ox as f32,
                        oy as f32,
                        ox as f32,
                        oy as f32,
                        Transform::identity(),
                        None,
                        0,
                        0,
                    );
                }
                pixmap.draw_pixmap(
                    0,
                    0,
                    fresh.as_ref(),
                    &paint,
                    transform.pre_translate(ox as f32 - ttx, oy as f32 - tty),
                    clip,
                );
                cache.insert(key, Arc::clone(owner), fresh);
                continue;
            }
        }
        paint_line(
            pixmap, line, alpha_lut, 0.0, 0.0, ttx, tty, transform, clip, clip_w, clip_h,
        );
    }
}

/// Renders glyph runs through the backend so rotation and scale shape the
/// glyphs themselves. Each glyph becomes a tinted coverage pixmap drawn with
/// the full transform from its quarter-snapped origin: the engine carries
/// origin + transform, tiny-skia does the filtering and clipping.
fn render_text_transformed(
    pixmap: &mut PixmapMut,
    text_ctx: &TextContext,
    params: TextRenderParams,
) {
    let ink_r = (params.color.r * 255.0).round() as u8;
    let ink_g = (params.color.g * 255.0).round() as u8;
    let ink_b = (params.color.b * 255.0).round() as u8;
    let opacity = (params.opacity * params.color.a).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let paint = PixmapPaint {
        opacity,
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };

    // Staging buffer for glyph coverage, copied out from under the
    // font-system locks like the aligned path does.
    let mut coverage = Vec::with_capacity(256);

    for line in &params.layout.lines {
        for g in &line.glyphs {
            // Phase-matched bitmap per axis (X quarters, Y eighths): the
            // residual the backend filters is tiny, and the coverage itself
            // is rasterized at the right phase. Matches the aligned path.
            let pen_x = params.origin.x + line.align_offset + g.point.x;
            let (base_x, frac4x) = split_phase(pen_x);
            let q = (frac4x + 0.5).floor() as i32;
            let (snap_x, binx) = if q >= PHASE_BINS as i32 {
                (base_x + 1, 0u8)
            } else {
                (base_x, q.max(0) as u8)
            };
            let pen_y = params.origin.y + g.point.y;
            let (ybase, yfrac8) = split_phase_y(pen_y);
            let yq = yfrac8.round() as i32;
            let (base_y, biny) = (
                ybase + yq.div_euclid(PHASE_BINS_Y as i32),
                yq.rem_euclid(PHASE_BINS_Y as i32) as u8,
            );

            let mut placed: Option<(f32, f32, u32, u32)> = None;
            text_ctx.raster_glyph(g.cache_key, binx | (biny << 2), |img_opt| {
                if let Some(img) = img_opt {
                    coverage.clear();
                    coverage.extend_from_slice(img.data);
                    // Same convention as the aligned path: the swash offset
                    // is baked into the bitmap, placement is floor + left.
                    placed = Some((
                        snap_x as f32 + img.left as f32,
                        base_y as f32 - img.top as f32,
                        img.width,
                        img.height,
                    ));
                }
            });

            let Some((gx, gy, w, h)) = placed else {
                continue;
            };
            if w == 0 || h == 0 || coverage.len() < (w * h) as usize {
                continue;
            }
            let Some(mut glyph) = Pixmap::new(w, h) else {
                continue;
            };
            for (dst, &cov) in glyph.pixels_mut().iter_mut().zip(coverage.iter()) {
                if cov == 0 {
                    continue;
                }
                let tint = |c: u8| ((c as u32 * cov as u32 + 127) / 255) as u8;
                if let Some(px) =
                    PremultipliedColorU8::from_rgba(tint(ink_r), tint(ink_g), tint(ink_b), cov)
                {
                    *dst = px;
                }
            }
            pixmap.draw_pixmap(
                0,
                0,
                glyph.as_ref(),
                &paint,
                params.transform.pre_translate(gx, gy),
                params.clip,
            );
        }
    }
}

/// Integer blit kept for the unit test below; the live aligned path now
/// places via filtered `draw_pixmap` (Plan B) instead.
#[allow(dead_code)]
fn blit_glyph(
    pixels: &mut [PremultipliedColorU8],
    pw: u32,
    ph: u32,
    data: &[u8],
    gw: u32,
    gh: u32,
    x: i32,
    y: i32,
    r: u8,
    g: u8,
    b: u8,
    alpha_lut: &[u8; 256],
    clip: Option<&Mask>,
    clip_w: u32,
    clip_h: u32,
) {
    let (w, h) = (gw as i32, gh as i32);

    for row in 0..h {
        let target_y = y + row;
        if target_y < 0 || target_y >= ph as i32 {
            continue;
        }
        let src_offset = (row * w) as usize;

        for col in 0..w {
            let target_x = x + col;
            if target_x < 0 || target_x >= pw as i32 {
                continue;
            }

            let mut alpha = alpha_lut[data[src_offset + col as usize] as usize];
            if let Some(mask) = clip {
                let tx = target_x as u32;
                let ty = target_y as u32;
                if tx >= clip_w || ty >= clip_h {
                    continue;
                }
                // Modulate by the mask instead of thresholding so clipped
                // glyph edges fade like every other primitive.
                let coverage = mask.data()[(ty * clip_w + tx) as usize] as f32 / 255.0;
                alpha = (alpha as f32 * coverage).round() as u8;
            }

            if alpha == 0 {
                continue;
            }

            let idx = (target_y as usize * pw as usize) + target_x as usize;
            blend_source_over(&mut pixels[idx], r, g, b, alpha);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn identity_lut() -> [u8; 256] {
        std::array::from_fn(|i| i as u8)
    }

    fn blank(w: u32, h: u32) -> Pixmap {
        Pixmap::new(w, h).expect("test pixmap allocates")
    }

    /// Phase split covers the pixel exactly once: base plus fraction
    /// reconstructs the pen, fraction in [0, 4).
    #[test]
    fn split_phase_covers_the_pixel() {
        assert_eq!(split_phase(4.0), (4, 0.0));
        assert_eq!(split_phase(4.5), (4, 2.0));
        let (base, frac4) = split_phase(-0.1);
        assert_eq!(base, -1);
        assert!((frac4 - 3.6).abs() < 1e-6, "frac4 = {frac4}");
        for hundredths in -200..200 {
            let pos = hundredths as f32 / 100.0;
            let (b, f) = split_phase(pos);
            assert!((0.0..4.0).contains(&f), "pos {pos}: frac4 {f}");
            assert!(
                ((b as f32 + f / 4.0) - pos).abs() < 1e-6,
                "pos {pos} reconstructs"
            );
        }
    }

    /// Blend endpoints: weight 1.0 reproduces the first bitmap, 0.0 the
    /// second, and the union origin covers both placements.
    #[test]
    fn blend_endpoints_reproduce_inputs() {
        let mut out = Vec::new();
        // t = 0 → a verbatim (b placed one right, must not leak in).
        let (x, y, w, h) =
            blend_phase_coverages(&[200], 1, 1, 5, 5, 1.0, &[100], 1, 1, 6, 5, 0.0, &mut out);
        assert_eq!((x, y, w, h), (5, 5, 2, 1));
        assert_eq!(out, vec![200, 0]);
        // t = 1 → b verbatim.
        let (x, y, w, h) =
            blend_phase_coverages(&[200], 1, 1, 5, 5, 0.0, &[100], 1, 1, 6, 5, 1.0, &mut out);
        assert_eq!((x, y, w, h), (5, 5, 2, 1));
        assert_eq!(out, vec![0, 100]);
    }

    /// Blend midpoint: overlapping texels average, ragged edges contribute
    /// zero outside their own bitmap (wrap case: bin 3 at base, bin 0 at
    /// base + 1).
    #[test]
    fn blend_midpoint_averages_overlap() {
        let mut out = Vec::new();
        let (x, y, w, h) = blend_phase_coverages(
            &[200, 200],
            2,
            1,
            5,
            5,
            0.5,
            &[100, 100],
            2,
            1,
            6,
            5,
            0.5,
            &mut out,
        );
        assert_eq!((x, y, w, h), (5, 5, 3, 1));
        assert_eq!(out, vec![100, 150, 50]);
    }

    /// Integer blit sanity: full-coverage texel lands opaque ink, zero stays.
    #[test]
    fn integer_blit_paints_coverage_verbatim() {
        let lut = identity_lut();
        let mut canvas = blank(8, 8);
        blit_glyph(
            canvas.pixels_mut(),
            8,
            8,
            &[255, 0],
            2,
            1,
            3,
            2,
            255,
            255,
            255,
            &lut,
            None,
            0,
            0,
        );
        let at = |x: usize, y: usize| canvas.pixels()[y * 8 + x].alpha();
        assert_eq!(at(3, 2), 255);
        assert_eq!(at(4, 2), 0);
        assert_eq!(at(2, 2), 0);
    }
}

fn blend_source_over(pixel: &mut PremultipliedColorU8, r: u8, g: u8, b: u8, a: u8) {
    let sa = a as f32 / 255.0;
    let inv_sa = 1.0 - sa;

    let dr = pixel.red() as f32;
    let dg = pixel.green() as f32;
    let db = pixel.blue() as f32;
    let da = pixel.alpha() as f32;

    let out_r = ((r as f32 * sa) + (dr * inv_sa)).round().min(255.0) as u8;
    let out_g = ((g as f32 * sa) + (dg * inv_sa)).round().min(255.0) as u8;
    let out_b = ((b as f32 * sa) + (db * inv_sa)).round().min(255.0) as u8;
    let out_a = ((a as f32) + (da * inv_sa)).round().min(255.0) as u8;

    if let Some(p) = PremultipliedColorU8::from_rgba(out_r, out_g, out_b, out_a) {
        *pixel = p;
    }
}
