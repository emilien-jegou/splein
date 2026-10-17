// Single responsibility: SVG rasterization cache (resvg -> RGBA8 -> ImageCache).
//
// Mirrors the tiny-skia engine: an SVG is rasterized once per (tree, target
// size) via resvg into premultiplied RGBA8 and then flows through the same
// GPU image cache as any raster image — one code path to the screen.
//
// Size is quantized to whole pixels (the raster target is the device-space
// quad size), so a static SVG re-rasterizes only when its drawn size changes.

#![cfg_attr(not(feature = "svg-runtime"), allow(dead_code))]

use riode::foundation::ResolvedRect;
use riode::scene::image::ImageSource;
use riode::scene::vector::VectorGraphic;
use rustc_hash::FxHashMap;
use std::sync::Arc;

const MAX_CACHED_SVGS: usize = 128;
/// Byte budget for rasterized SVGs: size-keyed entries (same SVG at many
/// sizes) can dwarf a count cap.
const MAX_SVG_BYTES: usize = 16 << 20;

#[cfg(feature = "svg-runtime")]
#[derive(Default)]
pub struct SvgCache {
    cache: FxHashMap<u64, ImageSource>,
    /// Last-use clock per entry (stalest evicted, never arbitrary).
    used: FxHashMap<u64, u64>,
    clock: u64,
    bytes: usize,
}

#[cfg(feature = "svg-runtime")]
impl SvgCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances the LRU clock. Call once per frame.
    pub fn next_frame(&mut self) {
        self.clock += 1;
    }

    /// Returns a retained RGBA8 raster of `graphic` at the device-space
    /// size of `rect` (already transformed; `scale` is the device scale,
    /// so the raster matches on-screen pixels 1:1), uploading through
    /// `images` on first use. `None` for empty rects or parse-stub trees.
    pub fn get_or_insert(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &mut crate::image::ImageCache,
        graphic: &VectorGraphic,
        rect: &ResolvedRect,
        scale: f32,
    ) -> Option<std::sync::Arc<crate::image::ImageGpu>> {
        if rect.width <= 0.0 || rect.height <= 0.0 || graphic.width <= 0.0 || graphic.height <= 0.0
        {
            return None;
        }
        let (rw, rh) = riode::scene::raster_cache_size(rect.width, rect.height, scale);
        let ptr_key = (Arc::as_ptr(&graphic.tree) as usize as u64) & 0x0000_FFFF_FFFF;
        let key = (ptr_key << 32) | ((rw as u64) << 16) | (rh as u64);

        if !self.cache.contains_key(&key) {
            while self.cache.len() >= MAX_CACHED_SVGS || self.bytes >= MAX_SVG_BYTES {
                let victim = self.used.iter().min_by_key(|(_, t)| **t).map(|(k, _)| *k);
                match victim {
                    Some(k) => {
                        if let Some(old) = self.cache.remove(&k) {
                            self.bytes = self.bytes.saturating_sub(old.data.len());
                        }
                        self.used.remove(&k);
                    }
                    None => break,
                }
            }
            let _span = tracing::trace_span!("rasterize_svg", w = rw, h = rh).entered();
            let mut pixmap = resvg::tiny_skia::Pixmap::new(rw, rh)?;
            let sx = rw as f32 / graphic.width;
            let sy = rh as f32 / graphic.height;
            resvg::render(
                &graphic.tree,
                resvg::tiny_skia::Transform::from_scale(sx, sy),
                &mut pixmap.as_mut(),
            );
            self.bytes += rw as usize * rh as usize * 4;
            self.cache
                .insert(key, ImageSource::from_rgba8(rw, rh, pixmap.take()));
        }
        self.clock += 1;
        self.used.insert(key, self.clock);
        let source = self.cache.get(&key).expect("just inserted");
        // The GPU cache keys by allocation identity; `source` is retained
        // here, so the rasterization stays uploaded for the cache's life.
        images.get_or_insert(device, queue, source)
    }
}

/// Stub when `svg-runtime` is off: parsing is unavailable; draws skip.
#[cfg(not(feature = "svg-runtime"))]
#[derive(Default)]
pub struct SvgCache;

#[cfg(not(feature = "svg-runtime"))]
impl SvgCache {
    pub fn new() -> Self {
        Self
    }

    pub fn next_frame(&mut self) {}

    #[allow(clippy::too_many_arguments)]
    pub fn get_or_insert(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _images: &mut crate::image::ImageCache,
        _graphic: &VectorGraphic,
        _rect: &ResolvedRect,
        _scale: f32,
    ) -> Option<std::sync::Arc<crate::image::ImageGpu>> {
        None
    }
}
