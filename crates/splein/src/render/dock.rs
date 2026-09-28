// crates/splein/src/render/dock.rs

use super::color::to_native_color;
use super::icons::{get_icons, render_cached_icon_21};
use crate::domain::dock::{ActiveTool, Dock};
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapMut, Stroke, Transform};

pub struct DockRenderer {
    cached_pixmap: Option<Pixmap>,
    last_key: Option<(ActiveTool, usize, Option<ActiveTool>)>,
}

impl Default for DockRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl DockRenderer {
    pub fn new() -> Self {
        Self {
            cached_pixmap: None,
            last_key: None,
        }
    }

    pub fn render_dock(
        &mut self,
        dock: &Dock,
        hovered_tool: Option<ActiveTool>,
        _is_clear_hovered: bool,
        pixmap: &mut PixmapMut,
    ) {
        let (dw, dh) = (dock.width as u32, dock.height as u32);
        let current_key = (dock.current_tool, dock.current_color_idx, hovered_tool);

        if self.cached_pixmap.is_none() || self.last_key != Some(current_key) {
            let mut cached = match Pixmap::new(dw, dh) {
                Some(p) => p,
                None => return,
            };
            cached.fill(Color::TRANSPARENT);

            self.rasterize_exact_design(dock, hovered_tool, &mut cached.as_mut());
            self.cached_pixmap = Some(cached);
            self.last_key = Some(current_key);
        }

        if let Some(ref cached) = self.cached_pixmap {
            pixmap.draw_pixmap(
                dock.position.x as i32,
                dock.position.y as i32,
                cached.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
    }

    fn rasterize_exact_design(
        &self,
        dock: &Dock,
        hovered_tool: Option<ActiveTool>,
        pixmap: &mut PixmapMut,
    ) {
        let (w, h) = (dock.width, dock.height);
        let icons = get_icons();

        // 1. Container: Pure #000000 with 8px radius
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(0, 0, 0, 255);
        bg_paint.anti_alias = true;

        let r = 8.0;
        let mut pb = PathBuilder::new();
        pb.move_to(r, 0.0);
        pb.line_to(w - r, 0.0);
        pb.quad_to(w, 0.0, w, r);
        pb.line_to(w, h - r);
        pb.quad_to(w, h, w - r, h);
        pb.line_to(r, h);
        pb.quad_to(0.0, h, 0.0, h - r);
        pb.line_to(0.0, r);
        pb.quad_to(0.0, 0.0, r, 0.0);

        if let Some(rounded) = pb.finish() {
            pixmap.fill_path(&rounded, &bg_paint, FillRule::Winding, Transform::identity(), None);
        }

        // 2. Drag Handle: 28x28 inside 42px height container, stroke #626262, width 0.9
        render_cached_icon_21(
            pixmap,
            &icons.drag_handle,
            29.0,
            h / 2.0,
            28.0,
            Color::from_rgba8(0x62, 0x62, 0x62, 255),
            0.9,
        );

        // 3. Colors Section: 5 Colors, 42x42 cells, 20px circles, 10px gap
        let colors_start_x = 56.0;
        let color_step = 52.0; // 42px + 10px gap
        for (i, &color) in dock.colors.iter().enumerate() {
            let cx = colors_start_x + (i as f32 * color_step) + 21.0;
            let cy = h / 2.0;

            let mut p = Paint::default();
            p.set_color(to_native_color(color));
            p.anti_alias = true;

            // 20px circle (10px radius)
            if let Some(circle) = PathBuilder::from_circle(cx, cy, 10.0) {
                pixmap.fill_path(&circle, &p, FillRule::Winding, Transform::identity(), None);
            }

            // Active selection ring
            if i == dock.current_color_idx && !matches!(dock.current_tool, ActiveTool::Pointer | ActiveTool::Eraser) {
                let mut ring = Paint::default();
                ring.set_color_rgba8(255, 255, 255, 220);
                ring.anti_alias = true;
                if let Some(rc) = PathBuilder::from_circle(cx, cy, 13.0) {
                    pixmap.stroke_path(&rc, &ring, &Stroke { width: 1.5, ..Default::default() }, Transform::identity(), None);
                }
            }
        }

        // "1-5" label at bottom right of colors group
        let label_15_x = colors_start_x + (5.0 * color_step) - 12.0;
        self.render_tiny_label("1-5", label_15_x, h - 4.0, pixmap);

        // 4. Divider 1: #FFFFFF14 0.5px
        let div1_x = 318.0;
        self.render_divider(div1_x, h, pixmap);

        // 5. Tools (q, w, e, r, t, y, u, i, o): 9 tools, 42x42 cells, 10px gap
        let tools_start_x = 327.0;
        let tool_items = [
            (ActiveTool::Pointer, 'q', &icons.tool_q, None),
            (ActiveTool::Pen, 'w', &icons.tool_w, None),
            (ActiveTool::Highlighter, 'e', &icons.tool_e, None),
            (ActiveTool::Text, 'r', &icons.tool_r, None),
            (ActiveTool::SelectRegion, 't', &icons.tool_t_1, Some(&icons.tool_t_2)),
            (ActiveTool::Line, 'y', &icons.tool_y, None),
            (ActiveTool::Rect, 'u', &icons.tool_u, None),
            (ActiveTool::Ellipse, 'i', &icons.tool_u, None),
            (ActiveTool::Eraser, 'o', &icons.tool_o, None),
        ];

        for (idx, &(tool, key_char, icon_path, icon_path_2)) in tool_items.iter().enumerate() {
            let cx = tools_start_x + (idx as f32 * 52.0) + 21.0;
            let cy = h / 2.0;

            let is_active = dock.current_tool == tool;
            let is_hovered = hovered_tool == Some(tool);

            // Hover / Active Background Highlight
            if is_active || is_hovered {
                let mut bg = Paint::default();
                bg.set_color_rgba8(255, 255, 255, if is_active { 42 } else { 20 });
                bg.anti_alias = true;
                if let Some(r) = tiny_skia::Rect::from_xywh(cx - 20.0, cy - 20.0, 40.0, 40.0) {
                    let mut b = PathBuilder::new();
                    b.push_rect(r);
                    if let Some(p) = b.finish() {
                        pixmap.fill_path(&p, &bg, FillRule::Winding, Transform::identity(), None);
                    }
                }
            }

            // Render exact SVG Tool Icon (28x28, stroke 0.9)
            if tool == ActiveTool::Ellipse {
                let scale = 28.0 / 21.0;
                let r = 5.625 * scale;
                if let Some(circle) = PathBuilder::from_circle(cx, cy, r) {
                    let mut p = Paint::default();
                    p.set_color_rgba8(255, 255, 255, 255);
                    p.anti_alias = true;
                    pixmap.stroke_path(&circle, &p, &Stroke { width: 0.9, ..Default::default() }, Transform::identity(), None);
                }
            } else {
                render_cached_icon_21(pixmap, icon_path, cx, cy, 28.0, Color::from_rgba8(255, 255, 255, 255), 0.9);
                if let Some(path2) = icon_path_2 {
                    render_cached_icon_21(pixmap, path2, cx, cy, 28.0, Color::from_rgba8(255, 255, 255, 255), 0.9);
                }
            }

            // Embedded #626262 10px key label in bottom right corner of cell
            self.render_tiny_char(key_char, cx + 15.0, h - 4.0, pixmap);
        }

        // 6. Divider 2: #FFFFFF14 0.5px
        let div2_x = 798.0;
        self.render_divider(div2_x, h, pixmap);

        // 7. Actions: Trash (#F66453) + More (#626262) - NO hover background on trashcan
        let trash_cx = 827.0;
        render_cached_icon_21(
            pixmap,
            &icons.action_trash,
            trash_cx,
            h / 2.0,
            28.0,
            Color::from_rgba8(0xF6, 0x64, 0x53, 255),
            0.9,
        );

        let more_cx = 873.0;
        render_cached_icon_21(
            pixmap,
            &icons.action_more,
            more_cx,
            h / 2.0,
            28.0,
            Color::from_rgba8(0x62, 0x62, 0x62, 255),
            0.9,
        );
    }

    fn render_divider(&self, x: f32, h: f32, pixmap: &mut PixmapMut) {
        let mut p = Paint::default();
        p.set_color_rgba8(255, 255, 255, 20); // #FFFFFF14
        let mut pb = PathBuilder::new();
        pb.move_to(x, 6.0);
        pb.line_to(x, h - 6.0);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &p, &Stroke { width: 0.5, ..Default::default() }, Transform::identity(), None);
        }
    }

    fn render_tiny_char(&self, ch: char, x: f32, y: f32, pixmap: &mut PixmapMut) {
        let mut p = Paint::default();
        p.set_color_rgba8(0x62, 0x62, 0x62, 255);
        p.anti_alias = true;
        let stroke = Stroke { width: 0.9, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };

        let mut pb = PathBuilder::new();
        match ch {
            'q' => {
                if let Some(c) = PathBuilder::from_circle(x - 2.5, y - 3.0, 2.5) {
                    pixmap.stroke_path(&c, &p, &stroke, Transform::identity(), None);
                }
                pb.move_to(x, y - 5.5);
                pb.line_to(x, y + 1.5);
            }
            'w' => {
                pb.move_to(x - 5.0, y - 5.5);
                pb.line_to(x - 3.0, y);
                pb.line_to(x - 1.5, y - 3.0);
                pb.line_to(x, y);
                pb.line_to(x + 2.0, y - 5.5);
            }
            'e' => {
                pb.move_to(x - 4.0, y - 3.0);
                pb.line_to(x + 1.5, y - 3.0);
                pb.line_to(x + 1.5, y - 5.5);
                pb.line_to(x - 4.0, y - 5.5);
                pb.line_to(x - 4.0, y);
                pb.line_to(x + 1.5, y);
            }
            'r' => {
                pb.move_to(x - 2.5, y);
                pb.line_to(x - 2.5, y - 5.0);
                pb.quad_to(x, y - 5.5, x + 1.5, y - 3.5);
            }
            't' => {
                pb.move_to(x - 1.5, y - 5.5);
                pb.line_to(x - 1.5, y);
                pb.move_to(x - 3.5, y - 4.0);
                pb.line_to(x + 0.5, y - 4.0);
            }
            'y' => {
                pb.move_to(x - 3.5, y - 5.5);
                pb.line_to(x - 1.0, y - 1.5);
                pb.line_to(x + 1.5, y - 5.5);
                pb.line_to(x - 2.5, y + 2.5);
            }
            'u' => {
                pb.move_to(x - 3.5, y - 5.5);
                pb.line_to(x - 3.5, y - 2.0);
                pb.quad_to(x - 1.0, y + 0.5, x + 1.5, y - 2.0);
                pb.line_to(x + 1.5, y - 5.5);
            }
            'i' => {
                pb.move_to(x, y - 4.0);
                pb.line_to(x, y);
                pb.move_to(x, y - 6.0);
                pb.line_to(x, y - 5.5);
            }
            'o' => {
                if let Some(c) = PathBuilder::from_circle(x - 1.5, y - 2.5, 2.5) {
                    pixmap.stroke_path(&c, &p, &stroke, Transform::identity(), None);
                }
            }
            _ => {}
        }
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
        }
    }

    fn render_tiny_label(&self, text: &str, x: f32, y: f32, pixmap: &mut PixmapMut) {
        let mut p = Paint::default();
        p.set_color_rgba8(0x62, 0x62, 0x62, 255);
        p.anti_alias = true;
        let stroke = Stroke { width: 0.9, line_cap: LineCap::Round, ..Default::default() };

        let mut pb = PathBuilder::new();
        if text == "1-5" {
            pb.move_to(x - 6.0, y - 4.0);
            pb.line_to(x - 5.0, y - 5.5);
            pb.line_to(x - 5.0, y);

            pb.move_to(x - 2.5, y - 3.0);
            pb.line_to(x, y - 3.0);

            pb.move_to(x + 4.0, y - 5.5);
            pb.line_to(x + 2.0, y - 5.5);
            pb.line_to(x + 2.0, y - 3.0);
            pb.line_to(x + 4.0, y - 3.0);
            pb.line_to(x + 4.0, y);
            pb.line_to(x + 2.0, y);
        }
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
        }
    }
}
