// Orchestrates tiny-skia canvas rendering and native Paper dock drawing.

use super::dock::{render_dock, TextEngine};
use super::elements::ElementRenderer;
use super::frame::Frame;
use crate::domain::dock::ActiveTool;
use tiny_skia::{Color, Pixmap, PixmapMut};

pub trait RenderEngine {
    fn render(&mut self, frame: &Frame, buffer: &mut [u8], width: u32, height: u32);
}

pub struct TinySkiaRenderer {
    element_renderer: ElementRenderer,
    text_engine: TextEngine,
    persistent_canvas: Option<Pixmap>,
}

impl Default for TinySkiaRenderer {
    fn default() -> Self {
        Self {
            element_renderer: ElementRenderer,
            text_engine: TextEngine::default(),
            persistent_canvas: None,
        }
    }
}

impl TinySkiaRenderer {
    fn prepare_canvas(&mut self, w: u32, h: u32) {
        if self
            .persistent_canvas
            .as_ref()
            .map_or(true, |c| c.width() != w || c.height() != h)
        {
            self.persistent_canvas = Pixmap::new(w, h);
            if let Some(ref mut c) = self.persistent_canvas {
                c.fill(Color::TRANSPARENT);
            }
        }
    }
}

impl RenderEngine for TinySkiaRenderer {
    fn render(&mut self, frame: &Frame, buffer: &mut [u8], width: u32, height: u32) {
        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };
        if !frame.canvas.is_overlay_active {
            pixmap.fill(Color::TRANSPARENT);
            return;
        }

        self.prepare_canvas(width, height);
        let drag = frame.canvas.drag;
        let resize = frame.canvas.resize;
        // A live move or resize rebuilds every frame so the ghost renders at its offset.
        if frame.canvas.needs_full_rebuild || drag.is_some() || resize.is_some() {
            if let Some(ref mut c) = self.persistent_canvas {
                c.fill(Color::TRANSPARENT);
                for (i, elem) in frame.canvas.elements.iter().enumerate() {
                    let ghost = resize
                        .filter(|(idxs, _, _)| idxs.contains(&i))
                        .map(|(_, base, target)| {
                            let mut mapped = elem.clone();
                            mapped.map_bounds(base, target);
                            mapped
                        })
                        .or_else(|| {
                            drag.filter(|(idxs, _)| idxs.contains(&i)).map(|(_, delta)| {
                                let mut moved = elem.clone();
                                moved.translate(delta);
                                moved
                            })
                        });
                    self.element_renderer
                        .render_element(ghost.as_ref().unwrap_or(elem), &mut c.as_mut());
                }
            }
        }

        if let Some(ref mut c) = self.persistent_canvas {
            for (seg, col, w) in frame.canvas.new_segments {
                self.element_renderer
                    .render_single_segment(seg, *col, *w, &mut c.as_mut());
            }
            pixmap.data_mut().copy_from_slice(c.data());
        }

        if frame.canvas.drag.is_none()
            && frame.canvas.resize.is_none()
            && frame.canvas.band.is_none()
        {
            if let Some(idx) = frame.canvas.hovered_idx {
                if let Some(elem) = frame.canvas.elements.get(idx) {
                    self.element_renderer.render_hover_halo(
                        elem,
                        frame.canvas.is_eraser_hover,
                        &mut pixmap,
                    );
                }
            }
        }
        if let Some((from, to, col, w, h)) = frame.canvas.live_tip {
            self.element_renderer
                .render_live_tip(from, to, col, w, h, &mut pixmap);
        }
        if let Some(elem) = frame.canvas.active_element {
            self.element_renderer.render_element(elem, &mut pixmap);
        }
        if let Some(rect) = frame.canvas.selection {
            self.element_renderer.render_handles(rect, &mut pixmap);
        }
        if let Some(rect) = frame.canvas.band {
            self.element_renderer.render_band(rect, &mut pixmap);
        }

        let active_i = match frame.dock.active_tool {
            ActiveTool::Pointer => 0,
            ActiveTool::Pen => 1,
            ActiveTool::Highlighter => 2,
            ActiveTool::Rect | ActiveTool::Line | ActiveTool::Ellipse => 3,
            ActiveTool::Text => 4,
            _ => 5,
        };
        render_dock(
            frame.dock.geometry,
            frame.dock.motion,
            &mut self.text_engine,
            active_i,
            frame.dock.active_shape,
            frame.dock.submenu_open,
            &mut pixmap,
        );
    }
}
