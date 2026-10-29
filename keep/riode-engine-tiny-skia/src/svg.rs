// Single responsibility: Bounded raster cache for vector graphics.

#![cfg_attr(not(feature = "svg-runtime"), allow(dead_code))]

use riode::scene::vector::VectorGraphic;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tiny_skia::{Pixmap, PixmapPaint, Transform as SkiaTransform};

const MAX_CACHED_SVGS: usize = 128;

/// Byte budget for rasterized SVGs: size-keyed entries (same SVG at many
/// sizes) can dwarf a count cap.
const MAX_SVG_BYTES: usize = 16 << 20;

#[cfg(feature = "svg-runtime")]
#[derive(Default)]
pub struct SvgCache {
    cache: FxHashMap<u64, Pixmap>,
    /// Last-use clock per entry (stalest evicted, never arbitrary).
    used: FxHashMap<u64, u64>,
    clock: u64,
    bytes: usize,
}

#[cfg(feature = "svg-runtime")]
impl SvgCache {
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::default(),
            used: FxHashMap::default(),
            clock: 0,
            bytes: 0,
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

        if !self.cache.contains_key(&key) {
            while self.cache.len() >= MAX_CACHED_SVGS || self.bytes >= MAX_SVG_BYTES {
                let victim = self.used.iter().min_by_key(|(_, t)| **t).map(|(k, _)| *k);
                match victim {
                    Some(k) => {
                        if let Some(old) = self.cache.remove(&k) {
                            self.bytes = self.bytes.saturating_sub(
                                old.width() as usize * old.height() as usize * 4,
                            );
                        }
                        self.used.remove(&k);
                    }
                    None => break,
                }
            }
        }

        if !self.cache.contains_key(&key) {
            let _span = tracing::trace_span!("rasterize_svg", w = rw, h = rh).entered();
            let mut p = Pixmap::new(rw, rh).unwrap();
            let sx = rw as f32 / graphic.width;
            let sy = rh as f32 / graphic.height;
            resvg::render(
                &graphic.tree,
                SkiaTransform::from_scale(sx, sy),
                &mut p.as_mut(),
            );
            self.bytes += rw as usize * rh as usize * 4;
            self.cache.insert(key, p);
        }
        self.clock += 1;
        self.used.insert(key, self.clock);
        let cached = self.cache.get(&key).expect("just inserted");

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
        _graphic: &riode::scene::vector::VectorGraphic,
        _rw: u32,
        _rh: u32,
        _target: &mut tiny_skia::PixmapMut,
        _opacity: f32,
        _transform: tiny_skia::Transform,
        _clip: Option<&tiny_skia::Mask>,
    ) {
    }
}
