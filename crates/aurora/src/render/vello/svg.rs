// Single responsibility: Native GPU vector SVG compilation and scene segment caching for Vello.

use resvg::usvg::{self, tiny_skia_path};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use vello::kurbo::{Affine, BezPath, PathEl, Point as KurboPoint, Stroke as KurboStroke};
use vello::peniko::Fill as VelloFillRule;
use vello::Scene as VelloScene;

use crate::foundation::ResolvedRect;
use crate::render::vello::svg_paint::convert_usvg_paint;
use crate::scene::vector::VectorGraphic;

const MAX_CACHED_SVGS: usize = 128;

/// Retained GPU vector cache compiling USVG trees to native Kurbo vector paths.
#[derive(Default)]
pub struct VelloSvgCache {
    cache: FxHashMap<usize, Arc<VelloScene>>,
}

impl VelloSvgCache {
    /// Constructs a clean native vector SVG cache.
    pub fn new() -> Self { Self::default() }

    /// Blits compiled vector SVG compute scenes transformed through the affine stack.
    pub fn draw(&mut self, scene: &mut VelloScene, rect: &ResolvedRect, graphic: &VectorGraphic, tx: Affine) {
        if rect.width <= 0.0 || rect.height <= 0.0 || graphic.width <= 0.0 || graphic.height <= 0.0 { return; }
        let ptr_key = Arc::as_ptr(&graphic.tree) as usize;

        if !self.cache.contains_key(&ptr_key) && self.cache.len() >= MAX_CACHED_SVGS {
            if let Some(&first) = self.cache.keys().next() { self.cache.remove(&first); }
        }

        let compiled = self.cache.entry(ptr_key).or_insert_with(|| {
            let mut sub_scene = VelloScene::new();
            compile_usvg_group(&graphic.tree.root(), &mut sub_scene, Affine::IDENTITY);
            Arc::new(sub_scene)
        });

        let sx = rect.width as f64 / graphic.width as f64;
        let sy = rect.height as f64 / graphic.height as f64;
        let transform = tx * Affine::translate((rect.x as f64, rect.y as f64)) * Affine::scale_non_uniform(sx, sy);
        scene.append(compiled.as_ref(), Some(transform));
    }
}

fn compile_usvg_group(group: &usvg::Group, scene: &mut VelloScene, parent_tx: Affine) {
    let t = group.transform();
    let group_tx = parent_tx * Affine::new([t.sx as f64, t.ky as f64, t.kx as f64, t.sy as f64, t.tx as f64, t.ty as f64]);

    for child in group.children() {
        match child {
            usvg::Node::Group(ref g) => compile_usvg_group(g, scene, group_tx),
            usvg::Node::Path(ref path) => {
                let mut bez = BezPath::new();
                for seg in path.data().segments() {
                    match seg {
                        tiny_skia_path::PathSegment::MoveTo(p) => bez.push(PathEl::MoveTo(KurboPoint::new(p.x as f64, p.y as f64))),
                        tiny_skia_path::PathSegment::LineTo(p) => bez.push(PathEl::LineTo(KurboPoint::new(p.x as f64, p.y as f64))),
                        tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                            bez.push(PathEl::QuadTo(KurboPoint::new(p1.x as f64, p1.y as f64), KurboPoint::new(p2.x as f64, p2.y as f64)));
                        }
                        tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                            bez.push(PathEl::CurveTo(
                                KurboPoint::new(p1.x as f64, p1.y as f64), KurboPoint::new(p2.x as f64, p2.y as f64), KurboPoint::new(p3.x as f64, p3.y as f64),
                            ));
                        }
                        tiny_skia_path::PathSegment::Close => bez.push(PathEl::ClosePath),
                    }
                }

                if let Some(ref fill) = path.fill() {
                    if let Some(brush) = convert_usvg_paint(fill.paint(), fill.opacity().get()) {
                        scene.fill(VelloFillRule::NonZero, group_tx, &brush, None, &bez);
                    }
                }

                if let Some(ref stroke) = path.stroke() {
                    if let Some(brush) = convert_usvg_paint(stroke.paint(), stroke.opacity().get()) {
                        let k_stroke = KurboStroke::new(stroke.width().get() as f64);
                        scene.stroke(&k_stroke, group_tx, &brush, None, &bez);
                    }
                }
            }
            _ => {}
        }
    }
}
