// Single responsibility: Font database storage, lifetime management, and shaping dispatcher.

use crate::foundation::Constraints;
#[cfg(feature = "text-runtime")]
use crate::text::analytic;
use crate::text::cache::ShapedTextCache;
use crate::text::config::TextConfig;
use crate::text::engine::ParleyEngine;
use crate::text::glyph::GlyphKey;
use crate::text::glyph_cache::{CachedBitmap, GlyphBitmapCache, GlyphCacheBudget, GlyphCacheStats};
use crate::text::layout::TextLayout;
use crate::text::min_content;
use crate::text::shaper::TextShapeParams;
use crate::text::shaper::TextShaper;
#[cfg(feature = "text-runtime")]
use rustc_hash::FxHasher;
#[cfg(feature = "text-runtime")]
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
#[cfg(feature = "text-runtime")]
use swash::scale::{Render, ScaleContext, Source, StrikeWith};

/// Size at or below which outlines rasterize hinted: grid-fit stems carry
/// small UI text, while display type above it stays linear and smooth.
pub const SMALL_TEXT_HINT_CUTOFF_PX: f32 = 16.0;

/// Quantizes a device scale factor for raster identity: 1/16 steps over
/// [0.5, 4.0], with near-unity scales snapping to exactly 1.0 so the
/// scale-aware path shares cache slots with the unscaled path.
pub fn quantize_raster_scale(scale: f32) -> f32 {
    if !scale.is_finite() || (scale - 1.0).abs() < 1e-4 {
        return 1.0;
    }
    ((scale * 16.0).round() / 16.0).clamp(0.5, 4.0)
}

/// Rasterized pixel buffer and metrics for an individual glyph.
pub struct GlyphBitmap<'a> {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

/// Retained typography context providing font loading, layout shaping, and glyph rasterization.
#[derive(Clone)]
pub struct TextContext {
    pub(crate) engine: ParleyEngine,
    #[cfg(feature = "text-runtime")]
    scaler: Arc<Mutex<ScaleContext>>,
    cache: ShapedTextCache,
    bitmaps: GlyphBitmapCache,
    pub(crate) font_blobs: Arc<Mutex<Vec<Arc<Vec<u8>>>>>,
}

/// Tag mixed into analytic cache keys so exact-coverage slots can never
/// collide with phase-bitmap or scaled-bitmap slots sharing the cache.
const ANALYTIC_TAG: u64 = 0x616E616C79746963;

/// Miss threshold for parallel batch shaping: below this, thread
/// spawn/sync costs exceed the shaping work itself.
pub const PARALLEL_SHAPE_THRESHOLD: usize = 8;

impl TextContext {
    /// Constructs a clean text context with an empty font database.
    pub fn new() -> Self {
        Self::with_glyph_budget(GlyphCacheBudget::default())
    }

    /// Constructs a text context under an explicit glyph bitmap budget.
    ///
    /// A `min == max` budget pins the cap (deterministic/embedded targets).
    pub fn with_glyph_budget(budget: GlyphCacheBudget) -> Self {
        let engine = ParleyEngine::new();
        let font_blobs = Arc::clone(&engine.font_blobs);
        Self {
            engine,
            #[cfg(feature = "text-runtime")]
            scaler: Arc::new(Mutex::new(ScaleContext::new())),
            cache: ShapedTextCache::new(),
            bitmaps: GlyphBitmapCache::with_budget(budget),
            font_blobs,
        }
    }

    /// Overrides the glyph bitmap budget on an existing context.
    pub fn set_glyph_budget(&mut self, budget: GlyphCacheBudget) {
        self.bitmaps = GlyphBitmapCache::with_budget(budget);
    }

    /// Returns a telemetry snapshot of the glyph bitmap cache.
    pub fn glyph_cache_stats(&self) -> GlyphCacheStats {
        self.bitmaps.stats()
    }

    /// Loads TrueType/OpenType font bytes into context and retains binary handle for GPU engines.
    pub fn load_font(&self, bytes: &[u8]) {
        let index = {
            let mut blobs = self.font_blobs.lock().unwrap();
            let index = blobs.len();
            blobs.push(Arc::new(bytes.to_vec()));
            index
        };
        self.engine.load_font(bytes, index);
    }

    /// Loads a font file from disk into the database for lazy system registration.
    pub fn load_font_file(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let bytes = std::fs::read(path)?;
        self.load_font(&bytes);
        Ok(())
    }

    /// Accesses retained raw font binary data by registration index.
    pub fn font_data(&self, index: usize) -> Option<Arc<Vec<u8>>> {
        self.font_blobs.lock().unwrap().get(index).cloned()
    }

    /// Resolved variable-font instance behind an opaque glyph key, as raw
    /// normalized coordinates (F2Dot14 bits, one per axis).
    ///
    /// GPU vector paths (Vello `draw_glyphs`) need this per run: the blob
    /// alone always resolves to the default instance, so weight/width
    /// variations would silently render as Regular without it.
    pub fn glyph_coords(&self, key: GlyphKey) -> Option<Vec<i16>> {
        let map = self.engine.key_map.lock().unwrap();
        // Tripwire: key_map is insert-only (shaping proof, never invalidated).
        // Past 64k distinct glyphs something is generating unbounded identity
        // (e.g. per-frame variation coordinates) — warn once, keep working.
        if map.len() > 65536 {
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!(
                    glyph_keys = map.len(),
                    "key_map past 64k entries: unbounded growth, audit shaping inputs (further warnings silent)"
                );
            }
        }
        map.get(&key).map(|rk| rk.coords.clone())
    }

    /// Shapes a text string into multi-line glyph runs under given constraints.
    pub fn shape(&self, params: TextShapeParams) -> Arc<TextLayout> {
        if let Some(cached) = self.cache.get(&params) {
            return cached;
        }
        let layout = Arc::new(self.engine.shape(params));
        self.cache.store(&params, Arc::clone(&layout));
        layout
    }

    /// Shapes a batch of strings, parallelizing cache misses across threads.
    ///
    /// Cache hits resolve inline in order; misses below
    /// [`PARALLEL_SHAPE_THRESHOLD`] shape inline, larger bursts split across
    /// scoped workers. Each worker owns a cloned font collection plus its own
    /// thread-local layout scratch (see `shape.rs`), so only short cache/lock
    /// operations are shared — no global build mutex convoys.
    pub fn shape_many(&self, params: &[TextShapeParams<'_>]) -> Vec<Arc<TextLayout>> {
        let mut out: Vec<Option<Arc<TextLayout>>> = params.iter().map(|_| None).collect();
        let mut misses = Vec::new();
        for (i, p) in params.iter().enumerate() {
            match self.cache.get(p) {
                Some(hit) => out[i] = Some(hit),
                None => misses.push(i),
            }
        }
        if misses.is_empty() {
            return out
                .into_iter()
                .map(|o| o.expect("hits fill every slot"))
                .collect();
        }
        if misses.len() < PARALLEL_SHAPE_THRESHOLD {
            for i in misses {
                let layout = Arc::new(self.engine.shape(params[i]));
                self.cache.store(&params[i], Arc::clone(&layout));
                out[i] = Some(layout);
            }
        } else {
            // Snapshot the shared collection once; workers clone per thread so
            // late registrations (after this call) are consistently excluded.
            let base = self.engine.font_ctx.lock().unwrap().clone();
            let workers = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(misses.len())
                .max(1);
            let chunk = misses.len().div_ceil(workers);
            let mut results = Vec::with_capacity(misses.len());
            std::thread::scope(|s| {
                let mut handles = Vec::new();
                for ids in misses.chunks(chunk) {
                    // Cloned per worker up front: `move` then carries the chunk
                    // reference (into `misses`, outlives the scope) plus this
                    // owned context, without dragging `base` along.
                    let worker_fcx = base.clone();
                    handles.push(s.spawn(move || {
                        let mut fcx = worker_fcx;
                        let mut done = Vec::with_capacity(ids.len());
                        for &i in ids {
                            let layout = Arc::new(self.engine.shape_with(&mut fcx, params[i]));
                            self.cache.store(&params[i], Arc::clone(&layout));
                            done.push((i, layout));
                        }
                        done
                    }));
                }
                for h in handles {
                    results.extend(h.join().expect("shaping worker panicked"));
                }
            });
            for (i, layout) in results {
                out[i] = Some(layout);
            }
        }
        out.into_iter()
            .map(|o| o.expect("every slot is filled"))
            .collect()
    }

    /// Shapes text directly from configuration specification under layout constraints.
    pub fn shape_config(&self, config: &TextConfig, constraints: Constraints) -> Arc<TextLayout> {
        self.shape(TextShapeParams {
            text: &config.content,
            font: config.font_id,
            family: config.family.as_deref(),
            style: config.style,
            size: config.size,
            line_height: config.line_height.resolve(config.size),
            letter_spacing: config.letter_spacing,
            align: config.align,
            wrap: config.wrap,
            overflow: config.overflow,
            max_lines: config.max_lines,
            weight: config.weight,
            variations: &config.variations,
            constraints,
        })
    }

    /// Widest unbreakable segment of a run: its min-content inline size, i.e. CSS `min-width: auto`.
    ///
    /// Derived from the unbounded layout, which `Size::Fit` text already shapes and caches, so
    /// measuring the floor costs no extra shaping pass. A `NoWrap` run cannot break at all,
    /// so its floor is its full (max-content) width.
    pub fn min_inline_width(&self, config: &TextConfig) -> f32 {
        let layout = self.shape_config(config, Constraints::UNCONSTRAINED);
        if config.wrap.is_nowrap() {
            return layout.total_size.width;
        }
        min_content::min_inline_width(&config.content, &layout.lines)
    }

    /// Full single-line width of a run: its max-content inline size.
    ///
    /// Same unbounded cached layout as [`Self::min_inline_width`]; no extra shaping.
    pub fn max_inline_width(&self, config: &TextConfig) -> f32 {
        let layout = self.shape_config(config, Constraints::UNCONSTRAINED);
        min_content::max_inline_width(&layout.lines)
    }

    /// Rasterizes a glyph by opaque key and subpixel phase, invoking the
    /// consumer closure with bitmap data.
    ///
    /// The key encodes (font, glyph id, size, variation instance); `phase`
    /// packs X quarter-pixel bins in the low 2 bits and Y eighth-pixel bins
    /// in the next 3 (values wrap per axis). Bitmaps are memoized per
    /// (key, phase) in a bounded adaptive cache, so repeated requests cost
    /// one hash lookup instead of a full swash re-render.
    ///
    /// Outlines rasterize hinted at or below [`SMALL_TEXT_HINT_CUTOFF_PX`]
    /// (grid-fit stems carry small UI text) and linear above it (display
    /// type stays smooth; swash's hinter preserves linear metrics anyway,
    /// so hinting never snaps advances).
    #[cfg(feature = "text-runtime")]
    pub fn raster_glyph<R>(
        &self,
        key: GlyphKey,
        phase: u8,
        f: impl FnOnce(Option<GlyphBitmap>) -> R,
    ) -> R {
        self.raster_inner(key, key, phase, 1.0, f)
    }

    /// Rasterizes a glyph at `size * quantize_raster_scale(scale)`, for
    /// device-pixel coverage on HiDPI displays.
    ///
    /// Bitmaps land in distinct cache slots derived from the scale, so the
    /// unscaled path (and every existing caller, e.g. tiny-skia) is
    /// untouched. Returned metrics are in *scaled* pixels: divide bearings
    /// and dimensions by [`quantize_raster_scale(scale)`] to recover layout
    /// units. Near-unity scales delegate to [`Self::raster_glyph`] exactly
    /// (shared slot, shared bitmap).
    #[cfg(feature = "text-runtime")]
    pub fn raster_glyph_scaled<R>(
        &self,
        key: GlyphKey,
        phase: u8,
        scale: f32,
        f: impl FnOnce(Option<GlyphBitmap>) -> R,
    ) -> R {
        let sq = quantize_raster_scale(scale);
        if sq == 1.0 {
            return self.raster_glyph(key, phase, f);
        }
        let mut h = FxHasher::default();
        key.0.hash(&mut h);
        ((sq * 16.0).round() as u16).hash(&mut h);
        self.raster_inner(GlyphKey(h.finish()), key, phase, sq, f)
    }

    /// Rasterizes a glyph with exact analytic coverage at a quarter-pixel
    /// subpixel position, invoking the consumer closure with bitmap data.
    ///
    /// Unlike [`Self::raster_glyph`] (which rasterizes swash bitmaps at phase
    /// bins and leaves the caller to interpolate the remainder), this path
    /// flattens the glyph outline and integrates exact pixel coverage at
    /// `(qx/4, qy/4)` via [`analytic`](crate::text::analytic). The caller
    /// integer-places the result: residual placement error is at most 1/8px
    /// with no coverage smoothing, so subpixel text stays as sharp as
    /// pixel-aligned text. Bitmaps are memoized per (key, qx, qy) in distinct
    /// cache slots that cannot collide with phase or scaled slots.
    ///
    /// Glyphs with no scalable outline (bitmap-emoji strikes), color
    /// outlines, or outlines larger than the analytic cap fall back to
    /// [`Self::raster_glyph`] at the matching phase, so behavior is total.
    #[cfg(feature = "text-runtime")]
    pub fn raster_glyph_exact<R>(
        &self,
        key: GlyphKey,
        qx: u8,
        qy: u8,
        f: impl FnOnce(Option<GlyphBitmap>) -> R,
    ) -> R {
        let (qx, qy) = (qx & 3, qy & 3);
        let mut h = FxHasher::default();
        key.0.hash(&mut h);
        ANALYTIC_TAG.hash(&mut h);
        qx.hash(&mut h);
        qy.hash(&mut h);
        let cache_key = GlyphKey(h.finish());
        if let Some(cached) = self.bitmaps.get(&cache_key, 0) {
            return f(Some(GlyphBitmap {
                left: cached.left,
                top: cached.top,
                width: cached.width,
                height: cached.height,
                data: &cached.data,
            }));
        }
        // Exact-phase fallback for non-analytic glyphs: the quarter quantum
        // maps onto eighth bins (2 per quarter) so the baked swash offset
        // matches the analytic translation.
        let fallback_phase = qx | ((qy * 2) << 2);

        let rk = self.engine.key_map.lock().unwrap().get(&key).cloned();
        let Some(rk) = rk else {
            self.bitmaps.note_miss();
            return f(None);
        };
        let size = f32::from_bits(rk.size_bits);
        let mut scaler = self.scaler.lock().unwrap();
        let Some(font) = swash::FontRef::from_index(rk.font.data.data(), rk.font.index as usize)
        else {
            drop(scaler);
            self.bitmaps.note_miss();
            return f(None);
        };
        let mut context = scaler
            .builder(font)
            .size(size)
            .hint(size <= SMALL_TEXT_HINT_CUTOFF_PX)
            .normalized_coords(&rk.coords)
            .build();
        // Outline extraction holds the scaler lock; the (expensive)
        // coverage integration below runs lock-free on the owned outline.
        let outline = if context.has_outlines() {
            context.scale_outline(rk.glyph_id as u16)
        } else {
            None
        };
        drop(scaler);
        let Some(outline) = outline else {
            return self.raster_glyph(key, fallback_phase, f);
        };
        if outline.is_color() || outline.is_empty() {
            return self.raster_glyph(key, fallback_phase, f);
        }
        // Oversized outlines bypass the analytic path (bounded work per
        // miss): the swash bitmap path handles them, including its own
        // oversized-bypass convention.
        let bounds = outline.bounds();
        if bounds.width() > 512.0 || bounds.height() > 512.0 {
            return self.raster_glyph(key, fallback_phase, f);
        }
        let (tx, ty) = (qx as f32 * 0.25, qy as f32 * 0.25);
        let mut acc = analytic::AnalyticBitmap::empty();
        let mut first = true;
        for i in 0..outline.len() {
            let Some(layer) = outline.get(i) else {
                continue;
            };
            let b = analytic::rasterize_layer(layer.points(), layer.verbs(), tx, ty);
            if first {
                acc = b;
                first = false;
            } else {
                acc = analytic::union_max(acc, b);
            }
        }
        if acc.is_empty() {
            // Empty coverage (e.g. space): cache the empty entry so the hot
            // path never pays for a repeated outline walk.
            self.bitmaps
                .store(cache_key, 0, CachedBitmap::new(0, 0, 0, 0, Vec::new()));
            return match self.bitmaps.get(&cache_key, 0) {
                Some(cached) => f(Some(GlyphBitmap {
                    left: cached.left,
                    top: cached.top,
                    width: cached.width,
                    height: cached.height,
                    data: &cached.data,
                })),
                None => {
                    self.bitmaps.note_miss();
                    f(None)
                }
            };
        }
        // Grid space is y-down with `top` = floor(min-y) (usually
        // negative); placement convention stores `-top` so backends keep
        // their `dev_y - top` formulas unchanged.
        self.bitmaps.store(
            cache_key,
            0,
            CachedBitmap::new(acc.left, -acc.top, acc.width, acc.height, acc.data),
        );
        match self.bitmaps.get(&cache_key, 0) {
            Some(cached) => f(Some(GlyphBitmap {
                left: cached.left,
                top: cached.top,
                width: cached.width,
                height: cached.height,
                data: &cached.data,
            })),
            None => {
                self.bitmaps.note_miss();
                f(None)
            }
        }
    }

    #[cfg(feature = "text-runtime")]
    fn raster_inner<R>(
        &self,
        cache_key: GlyphKey,
        map_key: GlyphKey,
        phase: u8,
        size_mul: f32,
        f: impl FnOnce(Option<GlyphBitmap>) -> R,
    ) -> R {
        // 2 bits of X phase, 3 bits of Y; out-of-range callers wrap instead
        // of panicking. Normalize once so cache, render, and store agree.
        let phase = phase & 0x1F;
        let (px, py) = (phase & 3, (phase >> 2) & 7);
        if let Some(cached) = self.bitmaps.get(&cache_key, phase) {
            return f(Some(GlyphBitmap {
                left: cached.left,
                top: cached.top,
                width: cached.width,
                height: cached.height,
                data: &cached.data,
            }));
        }

        let rk = self.engine.key_map.lock().unwrap().get(&map_key).cloned();
        let Some(rk) = rk else {
            self.bitmaps.note_miss();
            return f(None);
        };
        let size = f32::from_bits(rk.size_bits) * size_mul;
        let mut scaler = self.scaler.lock().unwrap();
        let Some(font) = swash::FontRef::from_index(rk.font.data.data(), rk.font.index as usize)
        else {
            drop(scaler);
            self.bitmaps.note_miss();
            return f(None);
        };
        let mut context = scaler
            .builder(font)
            .size(size)
            .hint(size <= SMALL_TEXT_HINT_CUTOFF_PX)
            .normalized_coords(&rk.coords)
            .build();
        // Rasterize at the requested subpixel phase: the caller places the
        // bitmap at `floor(pen) + phase/4` per axis, so the integer blit
        // composes exactly with no coverage redistribution.
        let mut render = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ]);
        render.offset(swash::zeno::Vector::new(
            px as f32 * 0.25,
            py as f32 * 0.125,
        ));
        let image = render.render(&mut context, rk.glyph_id as u16);
        drop(scaler);

        // Grayscale coverage reaches the blit path; anything else (color
        // outlines/bitmaps carry RGBA payloads the compositor cannot consume,
        // empty glyphs carry no ink) is cached as an empty entry so the hot
        // space key never pays for a repeated failed render.
        let (bitmap, fallback) = match image {
            Some(ref img) if img.content == swash::scale::image::Content::Mask => (
                CachedBitmap::new(
                    img.placement.left,
                    img.placement.top,
                    img.placement.width,
                    img.placement.height,
                    img.data.clone(),
                ),
                true,
            ),
            _ => (CachedBitmap::new(0, 0, 0, 0, Vec::new()), false),
        };
        self.bitmaps.store(cache_key, phase, bitmap);
        // Borrow back out of the cache so hit and miss paths hand the closure
        // identically-owned data. Oversized entries bypass the cache: fall
        // back to the freshly rendered image itself (downsampled copy when SS).
        match self.bitmaps.get(&cache_key, phase) {
            Some(cached) => f(Some(GlyphBitmap {
                left: cached.left,
                top: cached.top,
                width: cached.width,
                height: cached.height,
                data: &cached.data,
            })),
            None if fallback => match image {
                Some(ref img) => f(Some(GlyphBitmap {
                    left: img.placement.left,
                    top: img.placement.top,
                    width: img.placement.width,
                    height: img.placement.height,
                    data: &img.data,
                })),
                None => {
                    self.bitmaps.note_miss();
                    f(None)
                }
            },
            None => {
                self.bitmaps.note_miss();
                f(None)
            }
        }
    }
}

impl Default for TextContext {
    fn default() -> Self {
        Self::new()
    }
}

// ── Stub rasterization when `text-runtime` is off: no glyphs rasterize. ──
#[cfg(not(feature = "text-runtime"))]
impl TextContext {
    pub fn raster_glyph<R>(
        &self,
        _key: GlyphKey,
        _phase: u8,
        f: impl FnOnce(Option<GlyphBitmap>) -> R,
    ) -> R {
        self.bitmaps.note_miss();
        f(None)
    }

    pub fn raster_glyph_scaled<R>(
        &self,
        _key: GlyphKey,
        _phase: u8,
        _scale: f32,
        f: impl FnOnce(Option<GlyphBitmap>) -> R,
    ) -> R {
        self.bitmaps.note_miss();
        f(None)
    }

    pub fn raster_glyph_exact<R>(
        &self,
        _key: GlyphKey,
        _qx: u8,
        _qy: u8,
        f: impl FnOnce(Option<GlyphBitmap>) -> R,
    ) -> R {
        self.bitmaps.note_miss();
        f(None)
    }
}
