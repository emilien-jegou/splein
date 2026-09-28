// crates/splein/src/platform/wayland/cursor.rs

use super::WaylandAppState;
use crate::domain::dock::ActiveTool;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::Shm;
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};
use wayland_client::protocol::{wl_pointer, wl_shm, wl_surface};
use wayland_client::QueueHandle;

const CURSOR_DIM: u32 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    DefaultArrow,
    Tool(ActiveTool),
}

pub struct CursorManager {
    surface: wl_surface::WlSurface,
    pool: SlotPool,
    last_pointer: Option<wl_pointer::WlPointer>,
    last_serial: Option<u32>,
    current_style: Option<CursorStyle>,
}

impl CursorManager {
    pub fn new(
        compositor: &CompositorState,
        shm: &Shm,
        qh: &QueueHandle<WaylandAppState>,
    ) -> eyre::Result<Self> {
        let surface = compositor.create_surface(qh);
        let pool_size = (CURSOR_DIM * CURSOR_DIM * 4 * 2) as usize;
        let pool = SlotPool::new(pool_size, shm)?;

        Ok(Self {
            surface,
            pool,
            last_pointer: None,
            last_serial: None,
            current_style: None,
        })
    }

    pub fn on_pointer_enter(&mut self, pointer: wl_pointer::WlPointer, serial: u32, tool: ActiveTool, is_over_dock: bool) {
        self.last_pointer = Some(pointer);
        self.last_serial = Some(serial);
        self.current_style = None;
        self.update_cursor(tool, is_over_dock);
    }

    pub fn update_cursor(&mut self, tool: ActiveTool, is_over_dock: bool) {
        let style = if is_over_dock || tool == ActiveTool::Pointer {
            CursorStyle::DefaultArrow
        } else {
            CursorStyle::Tool(tool)
        };

        if self.current_style == Some(style) {
            return;
        }

        let (Some(ref pointer), Some(serial)) = (&self.last_pointer, self.last_serial) else {
            return;
        };

        let stride = CURSOR_DIM * 4;
        let Ok((buffer, canvas)) = self.pool.create_buffer(
            CURSOR_DIM as i32,
            CURSOR_DIM as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
        ) else {
            return;
        };

        let Some(mut pixmap) = PixmapMut::from_bytes(canvas, CURSOR_DIM, CURSOR_DIM) else {
            return;
        };
        pixmap.fill(Color::TRANSPARENT);

        let (hotspot_x, hotspot_y) = match style {
            CursorStyle::DefaultArrow => {
                Self::render_arrow_cursor(&mut pixmap);
                (8, 8)
            }
            CursorStyle::Tool(t) => {
                Self::render_tool_cursor(&mut pixmap, t);
                (24, 24)
            }
        };

        for chunk in canvas.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }

        self.surface.attach(Some(buffer.wl_buffer()), 0, 0);
        self.surface.damage_buffer(0, 0, CURSOR_DIM as i32, CURSOR_DIM as i32);
        self.surface.commit();

        pointer.set_cursor(serial, Some(&self.surface), hotspot_x, hotspot_y);
        self.current_style = Some(style);
    }

    fn render_arrow_cursor(pixmap: &mut PixmapMut) {
        let ox = 8.0;
        let oy = 8.0;

        let mut pb = PathBuilder::new();
        pb.move_to(ox, oy);
        pb.line_to(ox, oy + 18.0);
        pb.line_to(ox + 4.5, oy + 13.5);
        pb.line_to(ox + 8.5, oy + 22.0);
        pb.line_to(ox + 11.5, oy + 20.5);
        pb.line_to(ox + 7.5, oy + 12.0);
        pb.line_to(ox + 13.5, oy + 12.0);
        pb.close();

        if let Some(path) = pb.finish() {
            let mut fill_paint = Paint::default();
            fill_paint.set_color_rgba8(255, 255, 255, 255);
            fill_paint.anti_alias = true;
            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, Transform::identity(), None);

            let mut stroke_paint = Paint::default();
            stroke_paint.set_color_rgba8(0, 0, 0, 255);
            stroke_paint.anti_alias = true;
            let stroke = Stroke { width: 1.5, line_join: LineJoin::Round, ..Default::default() };
            pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
        }
    }

    fn render_tool_cursor(pixmap: &mut PixmapMut, tool: ActiveTool) {
        let cx = 24.0;
        let cy = 24.0;

        let radius = match tool {
            ActiveTool::Highlighter => 10.0,
            ActiveTool::Eraser => 9.0,
            _ => 3.5,
        };

        let mut white_p = Paint::default();
        white_p.set_color_rgba8(255, 255, 255, 240);
        white_p.anti_alias = true;

        let mut black_p = Paint::default();
        black_p.set_color_rgba8(0, 0, 0, 255);
        black_p.anti_alias = true;

        // 1. Tool radius indicator ring
        if let Some(circle) = PathBuilder::from_circle(cx, cy, radius) {
            pixmap.stroke_path(&circle, &white_p, &Stroke { width: 2.2, ..Default::default() }, Transform::identity(), None);
            pixmap.stroke_path(&circle, &black_p, &Stroke { width: 1.0, ..Default::default() }, Transform::identity(), None);
        }

        // 2. Small center crosshair (+)
        if tool != ActiveTool::Eraser {
            let arm = 4.0;
            let mut pb = PathBuilder::new();
            pb.move_to(cx - arm, cy);
            pb.line_to(cx + arm, cy);
            pb.move_to(cx, cy - arm);
            pb.line_to(cx, cy + arm);

            if let Some(cross) = pb.finish() {
                pixmap.stroke_path(&cross, &white_p, &Stroke { width: 2.5, line_cap: LineCap::Round, ..Default::default() }, Transform::identity(), None);
                pixmap.stroke_path(&cross, &black_p, &Stroke { width: 1.0, line_cap: LineCap::Round, ..Default::default() }, Transform::identity(), None);
            }
        }
    }

    pub fn reset_to_default(&mut self) {
        self.current_style = None;
        if let (Some(ref pointer), Some(serial)) = (&self.last_pointer, self.last_serial) {
            pointer.set_cursor(serial, None, 0, 0);
        }
    }
}
