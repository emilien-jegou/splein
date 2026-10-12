// Single responsibility: Bounded raster cache for vector graphics.

#![cfg_attr(not(feature = "svg-runtime"), allow(dead_code))]

use aurora::scene::vector::VectorGraphic;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tiny_skia::{Pixmap, PixmapPaint, Transform as SkiaTransform};

const MAX_CACHED_SVGS: usize = 128;

#[cfg(feature = "svg-runtime")]
#[derive(Default)]
pub struct SvgCache {
    cache: FxHashMap<u64, Pixmap>,
}

#[cfg(feature = "svg-runtime")]
impl SvgCache {
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::default(),
        }
    }

    pub fn draw(
        &mut self,
        graphic: &VectorGraphic,
        rw: u32,
        rh: u32,
        target: &mut tiny_skia::PixmapMut,
        opacity: f32,
        transform: SkiaTransform,
        clip: Option<&tiny_skia::Mask>,
    ) {
        if rw == 0 || rh == 0 || graphic.width <= 0.0 || graphic.height <= 0.0 {
            return;
        }

        let ptr_key = (Arc::as_ptr(&graphic.tree) as usize as u64) & 0x0000_FFFF_FFFF;
        let key = (ptr_key << 32) | ((rw as u64) << 16) | (rh as u64);

        if !self.cache.contains_key(&key) && self.cache.len() >= MAX_CACHED_SVGS {
            if let Some(&first) = self.cache.keys().next() {
                self.cache.remove(&first);
            }
        }

        let cached = self.cache.entry(key).or_insert_with(|| {
            let _span = tracing::trace_span!("rasterize_svg", w = rw, h = rh).entered();
            let mut p = Pixmap::new(rw, rh).unwrap();
            let sx = rw as f32 / graphic.width;
            let sy = rh as f32 / graphic.height;
            resvg::render(
                &graphic.tree,
                SkiaTransform::from_scale(sx, sy),
                &mut p.as_mut(),
            );
            p
        });

        target.draw_pixmap(
            0,
            0,
            cached.as_ref(),
            &PixmapPaint {
                opacity,
                ..Default::default()
            },
            transform,
            clip,
        );
    }
}

/// Stub when `svg-runtime` is off: vector draws become no-ops.
#[cfg(not(feature = "svg-runtime"))]
#[derive(Default)]
pub struct SvgCache;

#[cfg(not(feature = "svg-runtime"))]
impl SvgCache {
    pub fn new() -> Self {
        Self
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        _graphic: &aurora::scene::vector::VectorGraphic,
        _rw: u32,
        _rh: u32,
        _target: &mut tiny_skia::PixmapMut,
        _opacity: f32,
        _transform: tiny_skia::Transform,
        _clip: Option<&tiny_skia::Mask>,
    ) {
    }
}
