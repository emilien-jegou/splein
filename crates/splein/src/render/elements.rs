// crates/splein/src/render/elements.rs

use super::color::to_native_color;
use crate::domain::canvas::{DrawingElement, Rgba, Stroke as DomainStroke};
use crate::domain::geometry::{CubicBezierSegment, Vec2};
use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform};

pub struct ElementRenderer;

impl ElementRenderer {
    /// Strictly O(1): Renders exactly ONE Bézier segment onto the persistent canvas pixmap (0.004 ms)
    pub fn render_single_segment(
        &self,
        seg: &CubicBezierSegment,
        color: Rgba,
        base_width: f32,
        is_highlighter: bool,
        pixmap: &mut PixmapMut,
    ) {
        let mut pb = PathBuilder::new();
        pb.move_to(seg.start.x, seg.start.y);
        pb.cubic_to(seg.ctrl1.x, seg.ctrl1.y, seg.ctrl2.x, seg.ctrl2.y, seg.end.x, seg.end.y);

        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(to_native_color(color));
            paint.anti_alias = true;

            let stroke_opts = Stroke {
                width: base_width,
                // Contiguous Butt cap on intermediate joints prevents alpha buildup for highlighters
                line_cap: if is_highlighter { LineCap::Butt } else { LineCap::Round },
                line_join: LineJoin::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
        }
    }

    /// Strictly O(1): Renders ONLY the 2-point direct line segment to the cursor tip (0.003 ms)
    pub fn render_live_tip(
        &self,
        from: Vec2,
        to: Vec2,
        color: Rgba,
        base_width: f32,
        is_highlighter: bool,
        pixmap: &mut PixmapMut,
    ) {
        let mut pb = PathBuilder::new();
        pb.move_to(from.x, from.y);
        pb.line_to(to.x, to.y);

        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(to_native_color(color));
            paint.anti_alias = true;

            let stroke_opts = Stroke {
                width: base_width,
                line_cap: if is_highlighter { LineCap::Butt } else { LineCap::Round },
                line_join: LineJoin::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
        }
    }

    /// Used ONLY during explicit Undo, Eraser wipe, or Clear (never during live drawing)
    pub fn render_element(&self, elem: &DrawingElement, pixmap: &mut PixmapMut) {
        match elem {
            DrawingElement::Stroke(s) => self.render_stroke(s, pixmap, false),
            DrawingElement::Highlighter(s) => self.render_stroke(s, pixmap, true),
            DrawingElement::Line { start, end, color, width, .. } => {
                let mut pb = PathBuilder::new();
                pb.move_to(start.x, start.y);
                pb.line_to(end.x, end.y);
                if let Some(path) = pb.finish() {
                    let mut paint = Paint::default();
                    paint.set_color(to_native_color(*color));
                    paint.anti_alias = true;
                    let stroke_opts = Stroke { width: *width, line_cap: LineCap::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
                }
            }
            DrawingElement::Rect { min, max, color, width, .. } => {
                let w = max.x - min.x;
                let h = max.y - min.y;
                if w > 0.0 && h > 0.0 {
                    if let Some(r) = Rect::from_xywh(min.x, min.y, w, h) {
                        let mut paint = Paint::default();
                        paint.set_color(to_native_color(*color));
                        paint.anti_alias = true;
                        let pb = PathBuilder::from_rect(r);
                        let stroke_opts = Stroke { width: *width, line_join: LineJoin::Round, ..Default::default() };
                        pixmap.stroke_path(&pb, &paint, &stroke_opts, Transform::identity(), None);
                    }
                }
            }
            DrawingElement::Ellipse { center, rx, ry, color, width, .. } => {
                if *rx > 0.0 && *ry > 0.0 {
                    let kx = rx * 0.55228475;
                    let ky = ry * 0.55228475;
                    let (cx, cy) = (center.x, center.y);

                    let mut pb = PathBuilder::new();
                    pb.move_to(cx + rx, cy);
                    pb.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
                    pb.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
                    pb.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
                    pb.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
                    pb.close();

                    if let Some(path) = pb.finish() {
                        let mut paint = Paint::default();
                        paint.set_color(to_native_color(*color));
                        paint.anti_alias = true;
                        let stroke_opts = Stroke { width: *width, ..Default::default() };
                        pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
                    }
                }
            }
        }
    }

    fn render_stroke(&self, stroke: &DomainStroke, pixmap: &mut PixmapMut, is_highlighter: bool) {
        if stroke.segments.is_empty() {
            return;
        }

        let mut pb = PathBuilder::new();
        pb.move_to(stroke.segments[0].start.x, stroke.segments[0].start.y);
        for seg in &stroke.segments {
            pb.cubic_to(seg.ctrl1.x, seg.ctrl1.y, seg.ctrl2.x, seg.ctrl2.y, seg.end.x, seg.end.y);
        }

        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(to_native_color(stroke.color));
            paint.anti_alias = true;

            let stroke_opts = Stroke {
                width: stroke.base_width,
                line_cap: if is_highlighter { LineCap::Butt } else { LineCap::Round },
                line_join: LineJoin::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
        }
    }

    pub fn render_hover_halo(&self, elem: &DrawingElement, is_eraser: bool, pixmap: &mut PixmapMut) {
        let halo_color = if is_eraser {
            Color::from_rgba8(50, 50, 255, 120) // Red
        } else {
            Color::from_rgba8(255, 170, 0, 140) // Cyan/Blue
        };

        match elem {
            DrawingElement::Stroke(s) | DrawingElement::Highlighter(s) => {
                if s.segments.is_empty() { return; }
                let mut pb = PathBuilder::new();
                pb.move_to(s.segments[0].start.x, s.segments[0].start.y);
                for seg in &s.segments {
                    pb.cubic_to(seg.ctrl1.x, seg.ctrl1.y, seg.ctrl2.x, seg.ctrl2.y, seg.end.x, seg.end.y);
                }
                if let Some(path) = pb.finish() {
                    let mut paint = Paint::default();
                    paint.set_color(halo_color);
                    paint.anti_alias = true;
                    let stroke_opts = Stroke { width: s.base_width + 10.0, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
                }
            }
            DrawingElement::Line { start, end, width, .. } => {
                let mut pb = PathBuilder::new();
                pb.move_to(start.x, start.y);
                pb.line_to(end.x, end.y);
                if let Some(path) = pb.finish() {
                    let mut paint = Paint::default();
                    paint.set_color(halo_color);
                    paint.anti_alias = true;
                    let stroke_opts = Stroke { width: width + 10.0, line_cap: LineCap::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
                }
            }
            DrawingElement::Rect { min, max, width, .. } => {
                let w = max.x - min.x;
                let h = max.y - min.y;
                if w > 0.0 && h > 0.0 {
                    if let Some(r) = Rect::from_xywh(min.x, min.y, w, h) {
                        let mut paint = Paint::default();
                        paint.set_color(halo_color);
                        paint.anti_alias = true;
                        let pb = PathBuilder::from_rect(r);
                        let stroke_opts = Stroke { width: width + 10.0, line_join: LineJoin::Round, ..Default::default() };
                        pixmap.stroke_path(&pb, &paint, &stroke_opts, Transform::identity(), None);
                    }
                }
            }
            DrawingElement::Ellipse { center, rx, ry, width, .. } => {
                if *rx > 0.0 && *ry > 0.0 {
                    let kx = rx * 0.55228475;
                    let ky = ry * 0.55228475;
                    let (cx, cy) = (center.x, center.y);
                    let mut pb = PathBuilder::new();
                    pb.move_to(cx + rx, cy);
                    pb.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
                    pb.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
                    pb.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
                    pb.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
                    pb.close();
                    if let Some(path) = pb.finish() {
                        let mut paint = Paint::default();
                        paint.set_color(halo_color);
                        paint.anti_alias = true;
                        let stroke_opts = Stroke { width: width + 10.0, ..Default::default() };
                        pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
                    }
                }
            }
        }
    }
}
