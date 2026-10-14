// Single responsibility: Bounded retained pixmap cache for raster image display list blits.

use rustc_hash::FxHashMap;
use std::sync::Arc;
use tiny_skia::{IntSize, Mask, Pixmap, PixmapMut, PixmapPaint, Transform};

use riode::foundation::ResolvedRect;
use riode::scene::image::ImageSource;

const MAX_CACHED_IMAGES: usize = 64;

/// Byte budget for retained pixmaps: count caps alone admit 64×4K images.
const MAX_IMAGE_BYTES: usize = 32 << 20;

/// Retained pixmap cache keyed by source buffer, avoiding per-draw reallocation.
#[derive(Default)]
pub struct ImageCache {
    cache: FxHashMap<usize, CachedImage>,
    /// Last-use clock per entry (stalest evicted, never arbitrary).
    used: FxHashMap<usize, u64>,
    clock: u64,
    bytes: usize,
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
        if !super::command::rect_visible_in_tile(rect, transform, tile_w, tile_h) {
            return;
        }

        let key = Arc::as_ptr(&image.data) as usize;
        if !self.cache.contains_key(&key) {
            while self.cache.len() >= MAX_CACHED_IMAGES || self.bytes >= MAX_IMAGE_BYTES {
                let victim = self.used.iter().min_by_key(|(_, t)| **t).map(|(k, _)| *k);
                match victim {
                    Some(k) => {
                        if let Some(old) = self.cache.remove(&k) {
                            self.bytes = self.bytes.saturating_sub(
                                old.pixmap.width() as usize * old.pixmap.height() as usize * 4,
                            );
                        }
                        self.used.remove(&k);
                    }
                    None => break,
                }
            }
            let pixmap = build_pixmap(image);
            self.bytes += pixmap.width() as usize * pixmap.height() as usize * 4;
            self.cache.insert(
                key,
                CachedImage {
                    _source: Arc::clone(&image.data),
                    pixmap,
                },
            );
        }
        self.clock += 1;
        self.used.insert(key, self.clock);
        let cached = self.cache.get(&key).expect("just inserted");

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
