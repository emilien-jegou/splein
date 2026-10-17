// Single responsibility: Glyph bitmap Porter-Duff alpha compositing via TinySkia.

use riode::foundation::{Color, Point};
use riode::scene::{quantize_eighth, quantize_quarter};
use riode::text::{GlyphBitmap, TextContext, TextLayout};
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
/// same device integer origin, same baseline-snap mode. The clip is
/// deliberately excluded — it applies at draw time, so one entry serves
/// every clip state.
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
    /// Baseline Y-snap mode: snapped and gliding verts differ for
    /// identical origins, so the flag rides the key (toggling repaints
    /// instead of replaying the other mode).
    snap: bool,
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
    /// Rebounds the retained cache to a geometry-aware budget, evicting
    /// oldest entries until it fits. Safe to call on a live cache (e.g.
    /// from a backend resize): the family of keys is unchanged, only retained
    /// bytes are trimmed.
    pub fn set_cap(&mut self, cap: usize) {
        let cap = cap.max(1);
        self.cap = cap;
        while self.bytes > self.cap || self.entries.len() > LINE_CACHE_MAX_ENTRIES {
            let victim = self
                .entries
                .iter()
                .take(64)
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


    /// Marks a composited line most-recently-used and returns its pixmap.
    ///
    /// Single lookup where the call site previously did `touch` + `pixmap`
    /// (two hashes per line per tile). The returned borrow ends before the
    /// caller mutates the cache again.
    fn hit(&mut self, key: &LineKey) -> Option<&Pixmap> {
        self.clock += 1;
        let clock = self.clock;
        let entry = self.entries.get_mut(key)?;
        // The key carries the pointer and the entry holds the `Arc`: a hit
        // is the same object, never a recycled address.
        debug_assert_eq!(Arc::as_ptr(&entry.layout) as usize, key.ptr);
        entry.used = clock;
        Some(&entry.pixmap)
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
            // Sampled oldest-used (Redis-style, like the glyph cache): a full
            // scan per eviction costs O(2048) on exactly the pressured path.
            let victim = self
                .entries
                .iter()
                .take(64)
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

/// Coverage-to-alpha conversion: vello_cpu behavior — raw coverage ×
/// opacity, no transfer handling. An optional dwrite grayscale
/// contrast + gamma chain lives in [`riode::text::gamma`] (scalar twin
/// of the wgpu glyph shader, ported from GPUI/MS-terminal `dwrite.hlsl`)
/// and applies only with `RIODE_TEXT_CORRECTION=on`; default off, one
/// table at all sizes either way. Monotonic, so threshold tests and
/// bounding boxes are unaffected.

/// Subpixel position quanta per pixel: X quantizes to quarters (validated
/// smooth, matching the legacy phase bins), Y to eighths (1/16px residue,
/// so horizontal features never read as doubled rows). Asymmetric by
/// design — see `quantize_eighth`.
pub const POS_QUANTA_X: u8 = 4;
pub const POS_QUANTA_Y: u8 = 8;

/// Renders glyph runs for translation-only transforms.
///
/// Quarter-quantized analytic coverage: each pen quantizes to its nearest
/// quarter on both axes, the glyph outline is integrated at exactly that
/// quantum (see `raster_glyph_exact`), and the result integer-blits. No
/// fractional draw call exists anywhere in this path, so no backend
/// filtering behavior can quantize, snap, or otherwise reshape one axis
/// differently from the other; placement carries at most 1/8px snap with
/// zero coverage smoothing, so subpixel text is as sharp as integer text.
///
/// Cost notes: alphas fold opacity/gamma once per run, coverage blits
/// straight out of the cache handle (no staging copy, no per-glyph alloc).
/// At integer pens output matches the old exact integer blit.
pub(crate) fn render_text_aligned(
    pixmap: &mut PixmapMut,
    text_ctx: &TextContext,
    params: TextRenderParams,
) {
    let ink_r = (params.color.r * 255.0).round() as u8;
    let ink_g = (params.color.g * 255.0).round() as u8;
    let ink_b = (params.color.b * 255.0).round() as u8;
    let total_opacity = params.opacity * params.color.a;
    if total_opacity <= 0.0 {
        return;
    }

    // Corrected alpha table for this run: the gamma module folds the
    // display chain (keyed off the run ink) and the scalar opacity into
    // 256 bytes. `opacity_q` survives only as part of the line-cache key.
    let opacity_q = (total_opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    let alpha_table: [u8; 256] = riode::text::corrected_alpha_table(
        [params.color.r, params.color.g, params.color.b],
        total_opacity,
    );
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

    let transform = params.transform;
    let (ttx, tty) = (transform.tx, transform.ty);
    let clip = params.clip;
    let layout: &TextLayout = params.layout;
    let layout_arc = params.layout_arc;
    let mut line_cache = params.line_cache;
    let origin = params.origin;
    // Paints one line's glyphs into `target`. Glyph device positions are
    // rebased onto integral `space` for the blit fast path and onto `draw`
    // for the draw call under `tx` with clip `clip_now`: canvas passes
    // space (0, 0) (tile pixels), draw (ttx, tty) (layout space); the
    // line-cache path passes the pixmap origin for both with identity and no
    // clip (the clip applies once, when the composited line is drawn).
    // Captures only the shaper handle and scalars — never the canvas or
    // the line cache.
    let paint_line = |target: &mut PixmapMut,
                          line: &riode::text::ShapedLine,
                          alpha_lut: &[u8; 256],
                          space_x: f32,
                          space_y: f32,
                          _draw_x: f32,
                          _draw_y: f32,
                          _tx: Transform,
                          clip_now: Option<&Mask>,
                          clip_now_w: u32,
                          clip_now_h: u32| {
        // Shape-invariant analytic coverage on both axes: each quarter
        // quantum integrates one hinter-fitted outline (identical shape
        // at every position — no per-quantum hinter re-fit, no bearing
        // breathing, no Y jiggle; per-variant re-rendering was tried in
        // 6274e02 and reverted for exactly that motion bug).
        let (pw, ph) = (target.width(), target.height());
        let mut paint_at = |img: &GlyphBitmap, px: i32, py: i32| {
            let (w, h) = (img.width, img.height);
            if w == 0 || h == 0 || img.data.len() < (w * h) as usize {
                return;
            }
            blit_glyph(
                target.pixels_mut(),
                pw,
                ph,
                img.data,
                w,
                h,
                px - space_x as i32,
                py - space_y as i32,
                ink_r,
                ink_g,
                ink_b,
                alpha_lut,
                clip_now,
                clip_now_w,
                clip_now_h,
            );
        };
        for g in &line.glyphs {
            // Device pen = layout pen + canvas translation; positions below
            // are rebased onto `space`/`draw`, so the same code paints canvas
            // space and line-local space.
            let dev_x = origin.x + line.align_offset + g.point.x + ttx;
            let dev_y = origin.y + g.point.y + tty;
            // X quantizes to quarters, Y to eighths; the analytic bitmap
            // is integrated exactly at the quantum, so integer placement
            // carries <=1/8px snap on X, <=1/16px on Y, with no coverage
            // smoothing. With baseline Y-snap armed (`B` key, GPUI's 4x1),
            // Y snaps to whole device pixels instead (qy forced 0).
            // Settled runs arrive pre-rounded from core compile, so they
            // take the integer path naturally here.
            // Rebasing onto `space` keeps canvas and line-local painting
            // on the same code path.
            let (bx, qx) = quantize_quarter(dev_x);
            let (by, qy) = if riode::text::baseline_snap_y() {
                (dev_y.round() as i32, 0u8)
            } else {
                quantize_eighth(dev_y)
            };
            text_ctx.raster_glyph_exact(
                g.cache_key,
                qx,
                qy,
                |img_opt| {
                    if let Some(img) = img_opt {
                        paint_at(&img, bx + img.left, by - img.top);
                    }
                },
            );
        }
    };
    for (idx, line) in layout.lines.iter().enumerate() {
        if line.glyphs.is_empty() {
            continue;
        }
        let alpha_lut: &[u8; 256] = &alpha_table;
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
                    snap: riode::text::baseline_snap_y(),
                };
                if let Some(entry) = cache.hit(&key) {
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
/// the full transform from its exact origin: the engine carries
/// origin + transform, tiny-skia does the filtering and clipping.
pub(crate) fn render_text_transformed(
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
        // Opacity already lives in `alpha_table` below; the draw itself
        // must not apply it a second time.
        opacity: 1.0,
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };

    // Staging buffer for glyph coverage, copied out from under the
    // font-system locks like the aligned path does.
    let mut coverage = Vec::with_capacity(256);

    // Corrected alpha table shared by every glyph below: same chain as
    // the aligned path (correct-then-premultiply, matching wgpu's
    // correct-then-blend order).
    let alpha_table: [u8; 256] = riode::text::corrected_alpha_table(
        [params.color.r, params.color.g, params.color.b],
        opacity,
    );

    // One-entry tint pixmap reuse (M7): same-size glyphs (the common case
    // within one run) skip malloc+zero per glyph. Exact-size match only —
    // a wider buffer would corrupt row stride in the tint loop below.
    let mut scratch: Option<(u32, u32, Pixmap)> = None;

    for line in &params.layout.lines {
        for g in &line.glyphs {
            // Analytic coverage quantized X-quarters / Y-eighths;
            // placement stays exact (the fractional origin under the
            // transform is legitimate filtering of a sharp source).
            // Matches the aligned path.
            let pen_x = params.origin.x + line.align_offset + g.point.x;
            let pen_y = params.origin.y + g.point.y;
            let (qx, qy) = (
                quantize_quarter(pen_x).1,
                quantize_eighth(pen_y).1,
            );

            let mut placed: Option<(f32, f32, u32, u32)> = None;
            text_ctx.raster_glyph_exact(g.cache_key, qx, qy, |img_opt| {
                if let Some(img) = img_opt {
                    coverage.clear();
                    coverage.extend_from_slice(img.data);
                    // Same convention as the aligned path: the quantum
                    // offset is baked into the bitmap, placement backs it
                    // out of the exact pen.
                    placed = Some((
                        pen_x - qx as f32 * 0.25 + img.left as f32,
                        pen_y - qy as f32 * 0.125 - img.top as f32,
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
            let mut glyph = match scratch.take() {
                Some((sw, sh, mut p)) if sw == w && sh == h => {
                    p.fill(tiny_skia::Color::TRANSPARENT);
                    p
                }
                // Size differs (or first glyph): fresh alloc; the old buffer
                // drops here, which is rare vs same-size runs.
                _ => {
                    let Some(p) = Pixmap::new(w, h) else {
                        continue;
                    };
                    p
                }
            };
            for (dst, &cov) in glyph.pixels_mut().iter_mut().zip(coverage.iter()) {
                if cov == 0 {
                    continue;
                }
                let a = alpha_table[cov as usize];
                let tint = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
                if let Some(px) =
                    PremultipliedColorU8::from_rgba(tint(ink_r), tint(ink_g), tint(ink_b), a)
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
            scratch = Some((w, h, glyph));
        }
    }
}

/// Integer source-over blit of a coverage bitmap: the live aligned path
/// places analytic bitmaps this way (positions are quantum-exact, so no
/// filtering is ever wanted here).
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

    /// Quarter quantization covers the pixel exactly once: base plus
    /// quantum reconstructs the pen within 1/8px, quantum in [0, 4).
    #[test]
    fn quantize_quarter_covers_the_pixel() {
        assert_eq!(quantize_quarter(4.0), (4, 0));
        assert_eq!(quantize_quarter(4.5), (4, 2));
        assert_eq!(quantize_quarter(4.9), (5, 0));
        let (base, q) = quantize_quarter(-0.1);
        assert_eq!((base, q), (0, 0));
        for hundredths in -200..200 {
            let pos = hundredths as f32 / 100.0;
            let (b, q) = quantize_quarter(pos);
            assert!(q < 4, "pos {pos}: quantum {q}");
            let snapped = b as f32 + q as f32 / 4.0;
            assert!(
                (snapped - pos).abs() <= 1.0 / 8.0 + 1e-6,
                "pos {pos} snaps within 1/8px (got {snapped})"
            );
        }
    }

    /// Eighth quantization covers the pixel exactly once: base plus
    /// quantum reconstructs the pen within 1/16px, quantum in [0, 8).
    #[test]
    fn quantize_eighth_covers_the_pixel() {
        assert_eq!(quantize_eighth(4.0), (4, 0));
        assert_eq!(quantize_eighth(4.5), (4, 4));
        assert_eq!(quantize_eighth(4.9), (4, 7));
        assert_eq!(quantize_eighth(4.95), (5, 0));
        for hundredths in -200..200 {
            let pos = hundredths as f32 / 100.0;
            let (b, q) = quantize_eighth(pos);
            assert!(q < 8, "pos {pos}: quantum {q}");
            let snapped = b as f32 + q as f32 / 8.0;
            assert!(
                (snapped - pos).abs() <= 1.0 / 16.0 + 1e-6,
                "pos {pos} snaps within 1/16px (got {snapped})"
            );
        }
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
    // Encoded-space premultiplied source-over — the vello_cpu composite
    // verbatim (premultiplied sRGB, W3C src-over, no linearization, no
    // transfer handling). A prior revision blended here in linear light;
    // that darkened every semi-transparent dst texel relative to vello
    // (white at a=128 decoded to 0.21 instead of 0.5) and diverged from
    // the reference on every fringe over a non-opaque background — so it
    // was reverted. Linear blending also disagrees with vello_cpu
    // systematically on opaque backgrounds (fringe 0.5 lands 0.5 encoded
    // vs 0.735 through linear); encoded is the reference behavior.
    // Only the glyph blit calls this; every other primitive keeps its
    // own path, so the blast radius is text only.
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
