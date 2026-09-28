// crates/splein/src/render/tooltip.rs

use crate::domain::dock::HoveredTool;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

pub struct TooltipRenderer;

impl TooltipRenderer {
    pub fn render_tooltip(&self, pixmap: &mut PixmapMut, tooltip: &HoveredTool, dock_bottom_y: f32) {
        let cx = tooltip.center_x;
        let arrow_tip_y = dock_bottom_y + 4.0;
        let box_top = arrow_tip_y + 5.0;
        let box_w = 28.0;
        let box_h = 22.0;
        let box_r = 6.0;
        let box_left = cx - (box_w / 2.0);

        let mut pb = PathBuilder::new();
        pb.move_to(cx, arrow_tip_y);
        pb.line_to(cx - 5.0, box_top);
        pb.line_to(box_left + box_r, box_top);
        pb.quad_to(box_left, box_top, box_left, box_top + box_r);
        pb.line_to(box_left, box_top + box_h - box_r);
        pb.quad_to(box_left, box_top + box_h, box_left + box_r, box_top + box_h);
        pb.line_to(box_left + box_w - box_r, box_top + box_h);
        pb.quad_to(box_left + box_w, box_top + box_h, box_left + box_w, box_top + box_h - box_r);
        pb.line_to(box_left + box_w, box_top + box_r);
        pb.quad_to(box_left + box_w, box_top, box_left + box_w - box_r, box_top);
        pb.line_to(cx + 5.0, box_top);
        pb.close();

        if let Some(path) = pb.finish() {
            let mut bg_paint = Paint::default();
            bg_paint.set_color_rgba8(20, 18, 24, 245);
            bg_paint.anti_alias = true;
            pixmap.fill_path(&path, &bg_paint, FillRule::Winding, Transform::identity(), None);

            let mut border = Paint::default();
            border.set_color_rgba8(255, 255, 255, 35);
            border.anti_alias = true;
            pixmap.stroke_path(&path, &border, &Stroke { width: 1.0, ..Default::default() }, Transform::identity(), None);
        }

        let text_cy = box_top + (box_h / 2.0);
        self.render_keycap_letter(tooltip.keybind, cx, text_cy, pixmap);
    }

    fn render_keycap_letter(&self, ch: char, cx: f32, cy: f32, pixmap: &mut PixmapMut) {
        let mut p = Paint::default();
        p.set_color_rgba8(255, 255, 255, 240);
        p.anti_alias = true;
        let stroke = Stroke { width: 1.6, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };

        match ch {
            'Q' => {
                if let Some(c) = PathBuilder::from_circle(cx, cy - 0.5, 4.0) {
                    pixmap.stroke_path(&c, &p, &stroke, Transform::identity(), None);
                }
                let mut pb = PathBuilder::new();
                pb.move_to(cx + 1.5, cy + 1.5);
                pb.line_to(cx + 4.5, cy + 4.5);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            'W' => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 4.5, cy - 4.5);
                pb.line_to(cx - 2.5, cy + 4.5);
                pb.line_to(cx, cy - 0.5);
                pb.line_to(cx + 2.5, cy + 4.5);
                pb.line_to(cx + 4.5, cy - 4.5);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            'E' => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx + 3.5, cy - 4.5);
                pb.line_to(cx - 3.5, cy - 4.5);
                pb.line_to(cx - 3.5, cy + 4.5);
                pb.line_to(cx + 3.5, cy + 4.5);
                pb.move_to(cx - 3.5, cy);
                pb.line_to(cx + 2.0, cy);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            'R' => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 3.5, cy + 4.5);
                pb.line_to(cx - 3.5, cy - 4.5);
                pb.line_to(cx + 1.0, cy - 4.5);
                pb.cubic_to(cx + 4.0, cy - 4.5, cx + 4.0, cy, cx + 1.0, cy);
                pb.line_to(cx - 3.5, cy);
                pb.move_to(cx + 0.5, cy);
                pb.line_to(cx + 3.5, cy + 4.5);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            'T' => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 4.5, cy - 4.5);
                pb.line_to(cx + 4.5, cy - 4.5);
                pb.move_to(cx, cy - 4.5);
                pb.line_to(cx, cy + 4.5);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            'Y' => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 4.0, cy - 4.5);
                pb.line_to(cx, cy);
                pb.line_to(cx + 4.0, cy - 4.5);
                pb.move_to(cx, cy);
                pb.line_to(cx, cy + 4.5);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            'U' => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 3.5, cy - 4.5);
                pb.line_to(cx - 3.5, cy + 1.5);
                pb.quad_to(cx - 3.5, cy + 4.5, cx, cy + 4.5);
                pb.quad_to(cx + 3.5, cy + 4.5, cx + 3.5, cy + 1.5);
                pb.line_to(cx + 3.5, cy - 4.5);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            'I' => {
                let mut pb = PathBuilder::new();
                pb.move_to(cx - 2.5, cy - 4.5);
                pb.line_to(cx + 2.5, cy - 4.5);
                pb.move_to(cx, cy - 4.5);
                pb.line_to(cx, cy + 4.5);
                pb.move_to(cx - 2.5, cy + 4.5);
                pb.line_to(cx + 2.5, cy + 4.5);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            _ => {}
        }
    }
}
