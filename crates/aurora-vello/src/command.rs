// Single responsibility: Translates abstract SceneCommand primitives into Vello GPU commands.

use bon::builder;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use vello::kurbo::{Affine, Rect as KurboRect, Stroke as KurboStroke};
use vello::peniko::{
    BlendMode, Blob, Fill as VelloFillRule, Format as PenikoFormat, Image as PenikoImage,
};
use vello::Scene as VelloScene;

use crate::path::{build_kurbo_rect, build_kurbo_rounded_rect};
use crate::shader::build_vello_brush;
use crate::shadow::VelloShadowCache;
use crate::svg::VelloSvgCache;
use crate::text::VelloGlyphCache;
use aurora::render::stroke_geometry::adjust_stroke_geometry;
use aurora::scene::command::SceneCommand;
use aurora::text::TextContext;

/// Retained GPU raster image cache preventing per-frame pixel buffer allocations.
pub struct VelloImageCache {
    cache: FxHashMap<usize, Arc<PenikoImage>>,
    /// Last-use clock per entry; image handles are immutable + shareable,
    /// so eviction drops the map entry without touching live draws.
    used: FxHashMap<usize, u64>,
    clock: u64,
    bytes: usize,
}

/// Pressure cap: past this many distinct images the oldest-used entry is
/// evicted. Re-decode cost on re-encounter (~one copy + upload); pixels
/// identical either way.
const MAX_CACHED_VELLO_IMAGES: usize = 256;

/// Byte budget for retained image blobs: count caps alone admit 256×4K
/// uploads.
const MAX_VELLO_IMAGE_BYTES: usize = 32 << 20;

impl VelloImageCache {
    /// Constructs a clean image cache.
    pub fn new() -> Self {
        Self {
            cache: FxHashMap::default(),
            used: FxHashMap::default(),
            clock: 0,
            bytes: 0,
        }
    }

    fn touch(&mut self, key: usize) {
        self.clock += 1;
        self.used.insert(key, self.clock);
    }

    fn evict_oldest(&mut self) {
        let victim = self
            .used
            .iter()
            .min_by_key(|(_, t)| **t)
            .map(|(k, _)| *k);
        if let Some(k) = victim {
            if let Some(old) = self.cache.remove(&k) {
                self.bytes = self
                    .bytes
                    .saturating_sub(old.width as usize * old.height as usize * 4);
            }
            self.used.remove(&k);
            // Tripwire: eviction means sustained pressure (churn faster than
            // reuse). Warn once; the cap keeps RSS bounded regardless.
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!(
                    "VelloImageCache evicting under pressure: image churn exceeds reuse, audit allocation patterns (further warnings silent)"
                );
            }
        }
    }

    /// Obtains a retained Peniko image handle without cloning raw pixel memory.
    /// Keyed by (ptr, dimensions, byte length): a re-decoded identical image
    /// (new Arc) reuses only if dims+length match AND bytes match — checked
    /// lazily via stored length + first/last word to avoid full hash per frame.
    pub fn get_or_insert(&mut self, image: &aurora::scene::image::ImageSource) -> Arc<PenikoImage> {
        let ptr_key = Arc::as_ptr(&image.data) as usize;
        // Fast path: same allocation.
        if let Some(hit) = self.cache.get(&ptr_key) {
            let hit = hit.clone();
            self.touch(ptr_key);
            return hit;
        }
        // Slow path: same pixel content under a different allocation.
        // Compare length + dims first (cheap), then full bytes.
        let found = self
            .cache
            .iter()
            .find(|(_, v)| {
                v.width == image.width
                    && v.height == image.height
                    && (AsRef::<[u8]>::as_ref(&v.data) == image.data.as_slice())
            })
            .map(|(_, v)| v.clone());
        if let Some(hit) = found {
            // Alias path shares the existing Arc: no new bytes accounted
            // (eviction may then undercount while an alias survives — errs
            // toward a looser cap, never toward dropping live pixels).
            self.cache.insert(ptr_key, hit.clone());
            self.touch(ptr_key);
            return hit;
        }
        let inserted = Arc::new(PenikoImage::new(
            Blob::from(image.data.as_slice().to_vec()),
            PenikoFormat::Rgba8,
            image.width,
            image.height,
        ));
        while self.cache.len() >= MAX_CACHED_VELLO_IMAGES || self.bytes >= MAX_VELLO_IMAGE_BYTES
        {
            let before = self.cache.len();
            self.evict_oldest();
            if self.cache.len() == before {
                break;
            }
        }
        self.bytes += image.width as usize * image.height as usize * 4;
        self.cache.insert(ptr_key, inserted.clone());
        self.touch(ptr_key);
        inserted
    }
}

/// Snaps a pure-translation affine to device pixels.
///
/// Fractional translations fringe every axis-aligned edge (coverage splits
/// across two pixel rows); rounding the translation keeps straight borders
/// crisp. Scaled/rotated/skewed transforms pass through — fractional
/// coverage is correct there and motion smoothness outranks static snap.
fn snap_affine_translation(tx: Affine) -> Affine {
    let [a, b, c, d, e, f] = tx.as_coeffs();
    if a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0 {
        Affine::new([a, b, c, d, e.round(), f.round()])
    } else {
        tx
    }
}
#[builder]
pub fn compile_vello_chunk(
    commands: &[SceneCommand],
    scene: &mut VelloScene,
    bound: &KurboRect,
    tx_stack: &mut Vec<Affine>,
    shadow_cache: &mut VelloShadowCache,
    svg_cache: &mut VelloSvgCache,
    image_cache: &mut VelloImageCache,
    glyph_cache: &mut VelloGlyphCache,
    text_ctx: &TextContext,
) {
    let base_tx_len = tx_stack.len();
    tx_stack.push(tx_stack.last().copied().unwrap_or(Affine::IDENTITY));
    let mut opacity_noop: Vec<bool> = Vec::new();

    for cmd in commands {
        let cur_tx = *tx_stack.last().unwrap();
        match cmd {
            SceneCommand::PushOffset(p) => {
                tx_stack.push(cur_tx * Affine::translate((p.x as f64, p.y as f64)))
            }
            SceneCommand::PopOffset => {
                tx_stack.pop();
            }
            SceneCommand::PushTransform(t) => {
                let af = Affine::new([
                    t.a as f64,
                    t.b as f64,
                    t.c as f64,
                    t.d as f64,
                    t.tx as f64,
                    t.ty as f64,
                ]);
                tx_stack.push(cur_tx * af);
            }
            SceneCommand::PopTransform => {
                tx_stack.pop();
            }
            SceneCommand::PushOpacity(o) => {
                // No-op layer collapse: opacity ~1.0 adds a Vello blend
                // layer with no visual effect — skip both push and pop.
                let noop = *o >= 0.999;
                opacity_noop.push(noop);
                if !noop {
                    scene.push_layer(BlendMode::default(), *o as f32, Affine::IDENTITY, bound);
                }
            }
            SceneCommand::PopOpacity => {
                if !opacity_noop.pop().unwrap_or(false) {
                    scene.pop_layer();
                }
            }
            SceneCommand::PushClip { rect, radius } => {
                let local_shape = build_kurbo_rounded_rect(rect, *radius);
                scene.push_layer(BlendMode::default(), 1.0, cur_tx, &local_shape);
            }
            SceneCommand::PopClip => scene.pop_layer(),
            SceneCommand::DrawShadow {
                rect,
                radius,
                shadow,
                light,
            } => {
                shadow_cache.draw(scene, rect, *radius, shadow, light, cur_tx);
            }
            SceneCommand::DrawRect { rect, appearance } => {
                // Device-snapped translation: same rationale as the tiny-skia
                // snap — straight edges stay crisp, curves keep analytic Area
                // coverage on integral placement.
                let cur_tx = snap_affine_translation(cur_tx);
                let shape = build_kurbo_rounded_rect(rect, appearance.radius);
                if let Some(fill) = &appearance.fill {
                    if let Some(brush) = build_vello_brush(fill, appearance.opacity) {
                        scene.fill(VelloFillRule::NonZero, cur_tx, &brush, None, &shape);
                    }
                }
                if let Some(stroke) = &appearance.stroke {
                    if stroke.width > 0.0 {
                        if let Some(brush) = build_vello_brush(&stroke.fill, appearance.opacity) {
                            let (adj_rect, adj_radius) =
                                adjust_stroke_geometry(rect, appearance.radius, stroke);
                            let s_shape = build_kurbo_rounded_rect(&adj_rect, adj_radius);
                            scene.stroke(
                                &KurboStroke::new(stroke.width as f64),
                                cur_tx,
                                &brush,
                                None,
                                &s_shape,
                            );
                        }
                    }
                }
            }
            SceneCommand::DrawImage { rect, image } => {
                if image.width > 0 && image.height > 0 && rect.width > 0.0 && rect.height > 0.0 {
                    let peniko_img = image_cache.get_or_insert(image);
                    let (sx, sy) = (
                        rect.width as f64 / image.width as f64,
                        rect.height as f64 / image.height as f64,
                    );
                    let img_tx = cur_tx
                        * Affine::translate((rect.x as f64, rect.y as f64))
                        * Affine::scale_non_uniform(sx, sy);
                    scene.draw_image(peniko_img.as_ref(), img_tx);
                }
            }
            SceneCommand::DrawSvg { rect, graphic } => svg_cache.draw(scene, rect, graphic, cur_tx),
            SceneCommand::DrawText {
                origin,
                layout,
                color,
            } => {
                glyph_cache.render_text(scene, text_ctx, *origin, layout, *color, cur_tx);
            }
            SceneCommand::BeginLayer { rect, .. } => {
                let local_shape = build_kurbo_rect(rect);
                scene.push_layer(BlendMode::default(), 1.0, cur_tx, &local_shape);
            }
            SceneCommand::EndLayer { .. } => scene.pop_layer(),
        }
    }
    tx_stack.truncate(base_tx_len);
}
