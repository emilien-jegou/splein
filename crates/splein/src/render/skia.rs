use super::{RenderEngine, Scene};
use crate::domain::canvas::{Rgba, Stroke as DomainStroke};
use crate::domain::dock::ActiveTool;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform,
};

pub struct TinySkiaRenderer;

impl TinySkiaRenderer {
    pub fn new() -> Self {
        Self
    }

    fn to_skia_color(c: Rgba) -> Color {
        Color::from_rgba(c.r, c.g, c.b, c.a).unwrap_or(Color::BLACK)
    }

    fn render_stroke(&self, stroke: &DomainStroke, pixmap: &mut PixmapMut) {
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
            paint.set_color(Self::to_skia_color(stroke.color));
            paint.anti_alias = true;

            let stroke_opts = Stroke {
                width: stroke.base_width,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };

            pixmap.stroke_path(&path, &paint, &stroke_opts, Transform::identity(), None);
        }
    }
}

impl RenderEngine for TinySkiaRenderer {
    fn render(&mut self, scene: &Scene, buffer: &mut [u8], width: u32, height: u32) {
        let mut pixmap = match PixmapMut::from_bytes(buffer, width, height) {
            Some(p) => p,
            None => return,
        };

        pixmap.fill(Color::TRANSPARENT);

        if !scene.is_overlay_active {
            return;
        }

        for stroke in scene.strokes {
            self.render_stroke(stroke, &mut pixmap);
        }

        if let Some(active) = scene.active_stroke {
            self.render_stroke(active, &mut pixmap);
        }

        let dock = scene.dock;
        let r = 22.0;

        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(14, 14, 18, 235);
        bg_paint.anti_alias = true;

        let mut pb = PathBuilder::new();
        let x = dock.position.x;
        let y = dock.position.y;
        let w = dock.width;
        let h = dock.height;

        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + r);
        pb.quad_to(x, y, x + r, y);

        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &bg_paint, FillRule::Winding, Transform::identity(), None);

            let mut border = Paint::default();
            border.set_color_rgba8(255, 255, 255, 25);
            border.anti_alias = true;
            pixmap.stroke_path(&path, &border, &Stroke { width: 1.0, ..Default::default() }, Transform::identity(), None);
        }

        let swatch_y = y + (h / 2.0);
        for (i, &color) in dock.colors.iter().enumerate() {
            let cx = x + 42.0 + (i as f32 * 28.0);
            let mut p = Paint::default();
            p.set_color(Self::to_skia_color(color));
            p.anti_alias = true;

            if let Some(circle) = PathBuilder::from_circle(cx, swatch_y, 8.5) {
                pixmap.fill_path(&circle, &p, FillRule::Winding, Transform::identity(), None);
            }

            if i == dock.current_color_idx && dock.current_tool == ActiveTool::Pen {
                let mut ring = Paint::default();
                ring.set_color_rgba8(255, 255, 255, 220);
                ring.anti_alias = true;
                if let Some(rc) = PathBuilder::from_circle(cx, swatch_y, 11.5) {
                    pixmap.stroke_path(&rc, &ring, &Stroke { width: 2.0, ..Default::default() }, Transform::identity(), None);
                }
            }
        }

        let pen_active = dock.current_tool == ActiveTool::Pen;
        let eraser_active = dock.current_tool == ActiveTool::Eraser;
        Self::draw_tool_badge(&mut pixmap, x + 235.0, swatch_y, pen_active);
        Self::draw_tool_badge(&mut pixmap, x + 270.0, swatch_y, eraser_active);
    }
}

impl TinySkiaRenderer {
    fn draw_tool_badge(pixmap: &mut PixmapMut, cx: f32, cy: f32, active: bool) {
        let mut p = Paint::default();
        p.set_color_rgba8(255, 255, 255, if active { 45 } else { 12 });
        p.anti_alias = true;
        if let Some(c) = PathBuilder::from_circle(cx, cy, 11.0) {
            pixmap.fill_path(&c, &p, FillRule::Winding, Transform::identity(), None);
        }
    }
}
