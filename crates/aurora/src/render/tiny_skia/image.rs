// Single responsibility: Bounded retained pixmap cache for raster image display list blits.

use rustc_hash::FxHashMap;
use std::sync::Arc;
use tiny_skia::{IntSize, Mask, Pixmap, PixmapMut, PixmapPaint, Transform};

use crate::foundation::ResolvedRect;
use crate::scene::image::ImageSource;

const MAX_CACHED_IMAGES: usize = 64;

/// Retained pixmap cache keyed by source buffer, avoiding per-draw reallocation.
#[derive(Default)]
pub struct ImageCache {
    cache: FxHashMap<usize, CachedImage>,
}

/// A cached pixmap plus the source handle keeping its key address unique.
struct CachedImage {
    _source: Arc<Vec<u8>>,
    pixmap: Pixmap,
}

impl ImageCache {
    /// Constructs a clean image cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Blits a raster image through the retained cache, culling against the tile bounds.
    pub fn draw(
        &mut self,
        target: &mut PixmapMut,
        rect: &ResolvedRect,
        image: &ImageSource,
        opacity: f32,
        transform: Transform,
        clip: Option<&Mask>,
        tile_w: f32,
        tile_h: f32,
    ) {
        if image.width == 0 || image.height == 0 {
            return;
        }
        if (rect.x + transform.tx) >= tile_w
            || (rect.y + transform.ty) >= tile_h
            || (rect.x + transform.tx + rect.width) <= 0.0
            || (rect.y + transform.ty + rect.height) <= 0.0
        {
            return;
        }

        let key = Arc::as_ptr(&image.data) as usize;
        if !self.cache.contains_key(&key) && self.cache.len() >= MAX_CACHED_IMAGES {
            if let Some(&first) = self.cache.keys().next() {
                self.cache.remove(&first);
            }
        }

        let cached = self.cache.entry(key).or_insert_with(|| CachedImage {
            _source: Arc::clone(&image.data),
            pixmap: build_pixmap(image),
        });

        let paint = PixmapPaint {
            opacity,
            ..Default::default()
        };
        let image_tx = transform.pre_translate(rect.x, rect.y).pre_scale(
            rect.width / image.width as f32,
            rect.height / image.height as f32,
        );
        target.draw_pixmap(0, 0, cached.pixmap.as_ref(), &paint, image_tx, clip);
    }
}

/// Copies an image source into an owned pixmap once for retention.
fn build_pixmap(image: &ImageSource) -> Pixmap {
    let size = IntSize::from_wh(image.width, image.height).expect("image dimensions overflow");
    Pixmap::from_vec(image.data.as_slice().to_vec(), size).expect("image buffer size mismatch")
}
