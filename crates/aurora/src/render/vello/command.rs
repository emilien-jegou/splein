// Single responsibility: Translates abstract SceneCommand primitives into Vello GPU commands.

use bon::builder;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use vello::kurbo::{Affine, Rect as KurboRect, Stroke as KurboStroke};
use vello::peniko::{BlendMode, Blob, Fill as VelloFillRule, Format as PenikoFormat, Image as PenikoImage};
use vello::Scene as VelloScene;

use crate::render::vello::path::{build_kurbo_rect, build_kurbo_rounded_rect};
use crate::render::vello::shader::build_vello_brush;
use crate::render::vello::shadow::VelloShadowCache;
use crate::render::vello::stroke::adjust_stroke_geometry;
use crate::render::vello::svg::VelloSvgCache;
use crate::render::vello::text::VelloGlyphCache;
use crate::scene::command::SceneCommand;
use crate::text::TextContext;

/// Retained GPU raster image cache preventing per-frame pixel buffer allocations.
#[derive(Default)]
pub struct VelloImageCache {
    cache: FxHashMap<usize, Arc<PenikoImage>>,
}

impl VelloImageCache {
    /// Constructs a clean image cache.
    pub fn new() -> Self { Self::default() }

    /// Obtains a retained Peniko image handle without cloning raw pixel memory.
    pub fn get_or_insert(&mut self, image: &crate::scene::image::ImageSource) -> Arc<PenikoImage> {
        let ptr_key = Arc::as_ptr(&image.data) as usize;
        self.cache.entry(ptr_key).or_insert_with(|| {
            Arc::new(PenikoImage::new(
                Blob::from(image.data.as_slice().to_vec()),
                PenikoFormat::Rgba8,
                image.width,
                image.height,
            ))
        }).clone()
    }
}

/// Emits an entire sequence of scene commands into the target Vello compute scene.
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

    for cmd in commands {
        let cur_tx = *tx_stack.last().unwrap();
        match cmd {
            SceneCommand::PushOffset(p) => tx_stack.push(cur_tx * Affine::translate((p.x as f64, p.y as f64))),
            SceneCommand::PopOffset => { tx_stack.pop(); }
            SceneCommand::PushTransform(t) => {
                let af = Affine::new([t.a as f64, t.b as f64, t.c as f64, t.d as f64, t.tx as f64, t.ty as f64]);
                tx_stack.push(cur_tx * af);
            }
            SceneCommand::PopTransform => { tx_stack.pop(); }
            SceneCommand::PushOpacity(o) => { scene.push_layer(BlendMode::default(), *o as f32, Affine::IDENTITY, bound); }
            SceneCommand::PopOpacity => scene.pop_layer(),
            SceneCommand::PushClip { rect, radius } => {
                let local_shape = build_kurbo_rounded_rect(rect, *radius);
                scene.push_layer(BlendMode::default(), 1.0, cur_tx, &local_shape);
            }
            SceneCommand::PopClip => scene.pop_layer(),
            SceneCommand::DrawShadow { rect, radius, shadow } => {
                shadow_cache.draw(scene, rect, *radius, shadow, cur_tx);
            }
            SceneCommand::DrawRect { rect, appearance } => {
                let shape = build_kurbo_rounded_rect(rect, appearance.radius);
                if let Some(fill) = &appearance.fill {
                    if let Some(brush) = build_vello_brush(fill, appearance.opacity) {
                        scene.fill(VelloFillRule::NonZero, cur_tx, &brush, None, &shape);
                    }
                }
                if let Some(stroke) = &appearance.stroke {
                    if stroke.width > 0.0 {
                        if let Some(brush) = build_vello_brush(&stroke.fill, appearance.opacity) {
                            let (adj_rect, adj_radius) = adjust_stroke_geometry(rect, appearance.radius, stroke);
                            let s_shape = build_kurbo_rounded_rect(&adj_rect, adj_radius);
                            scene.stroke(&KurboStroke::new(stroke.width as f64), cur_tx, &brush, None, &s_shape);
                        }
                    }
                }
            }
            SceneCommand::DrawImage { rect, image } => {
                if image.width > 0 && image.height > 0 && rect.width > 0.0 && rect.height > 0.0 {
                    let peniko_img = image_cache.get_or_insert(image);
                    let (sx, sy) = (rect.width as f64 / image.width as f64, rect.height as f64 / image.height as f64);
                    let img_tx = cur_tx * Affine::translate((rect.x as f64, rect.y as f64)) * Affine::scale_non_uniform(sx, sy);
                    scene.draw_image(peniko_img.as_ref(), img_tx);
                }
            }
            SceneCommand::DrawSvg { rect, graphic } => svg_cache.draw(scene, rect, graphic, cur_tx),
            SceneCommand::DrawText { origin, layout, color } => {
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
