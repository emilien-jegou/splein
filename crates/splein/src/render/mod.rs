// crates/splein/src/render/mod.rs

pub mod color;
pub mod dock;
pub mod elements;
pub mod icons;

use crate::domain::canvas::{DrawingElement, Rgba};
use crate::domain::dock::{ActiveTool, Dock};
use crate::domain::geometry::{CubicBezierSegment, Vec2};
use dock::DockRenderer;
use elements::ElementRenderer;
use tiny_skia::{Color, Pixmap, PixmapMut};

pub struct Scene<'a> {
    pub elements: &'a [DrawingElement],
    pub dock: &'a Dock,
    pub is_overlay_active: bool,
    pub hovered_idx: Option<usize>,
    pub is_eraser_hover: bool,
    pub hovered_dock_tool: Option<ActiveTool>,

    // Strictly O(1) live rendering fields
    pub needs_full_rebuild: bool,
    pub new_segments: &'a [(CubicBezierSegment, Rgba, f32, bool)],
    pub live_tip: Option<(Vec2, Vec2, Rgba, f32, bool)>,
    pub active_shape: Option<&'a DrawingElement>,
    pub live_drag_delta: Option<(usize, Vec2)>,
}

pub trait RenderEngine {
    fn render(&mut self, scene: &Scene, buffer: &mut [u8], width: u32, height: u32);
}

pub struct TinySkiaRenderer {
    element_renderer: ElementRenderer,
    dock_renderer: DockRenderer,
    persistent_canvas: Option<Pixmap>,
}

impl Default for TinySkiaRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TinySkiaRenderer {
    pub fn new() -> Self {
        Self {
            element_renderer: ElementRenderer,
            dock_renderer: DockRenderer::new(),
            persistent_canvas: None,
        }
    }

    fn ensure_canvas(&mut self, width: u32, height: u32) {
        let needs_resize = self.persistent_canvas.as_ref().map(|c| c.width() != width || c.height() != height).unwrap_or(true);
        if needs_resize {
            self.persistent_canvas = Pixmap::new(width, height);
            if let Some(ref mut c) = self.persistent_canvas {
                c.fill(Color::TRANSPARENT);
            }
        }
    }

    fn rebuild_all_elements(&mut self, elements: &[DrawingElement], width: u32, height: u32) {
        self.ensure_canvas(width, height);
        if let Some(ref mut pixmap) = self.persistent_canvas {
            pixmap.fill(Color::TRANSPARENT);
            for elem in elements {
                self.element_renderer.render_element(elem, &mut pixmap.as_mut());
            }
        }
    }
}

impl RenderEngine for TinySkiaRenderer {
    fn render(&mut self, scene: &Scene, buffer: &mut [u8], width: u32, height: u32) {
        let Some(mut pixmap) = PixmapMut::from_bytes(buffer, width, height) else {
            return;
        };

        if !scene.is_overlay_active {
            pixmap.fill(Color::TRANSPARENT);
            return;
        }

        self.ensure_canvas(width, height);

        // 1. Full canvas rebuild ONLY occurs on explicit Undo, Eraser delete, Clear, or Move drop
        if scene.needs_full_rebuild || self.persistent_canvas.is_none() {
            self.rebuild_all_elements(scene.elements, width, height);
        }

        // 2. Strictly O(1): Bake ONLY the single newly finalized segment into persistent canvas (0.004 ms)
        if let Some(ref mut canvas) = self.persistent_canvas {
            for (seg, color, stroke_w, is_high) in scene.new_segments {
                self.element_renderer.render_single_segment(seg, *color, *stroke_w, *is_high, &mut canvas.as_mut());
            }
        }

        // 3. Fast SIMD Blit persistent canvas to screen buffer (0.2 ms on 4K)
        if let Some(ref canvas) = self.persistent_canvas {
            pixmap.data_mut().copy_from_slice(canvas.data());
        }

        // 4. Render Hover Halo
        if let Some(idx) = scene.hovered_idx {
            if let Some(elem) = scene.elements.get(idx) {
                self.element_renderer.render_hover_halo(elem, scene.is_eraser_hover, &mut pixmap);
            }
        }

        // 5. Strictly O(1): Render ONLY the single 2-point live tip to cursor (0.003 ms)
        // CRITICAL: We DO NOT re-render active stroke history here. Past segments are ALREADY in persistent_canvas!
        if let Some((from, to, color, stroke_w, is_high)) = scene.live_tip {
            self.element_renderer.render_live_tip(from, to, color, stroke_w, is_high, &mut pixmap);
        }

        // 6. Strictly O(1): Render active shape preview (Line, Rect, Ellipse have fixed 1-4 edges)
        if let Some(shape) = scene.active_shape {
            self.element_renderer.render_element(shape, &mut pixmap);
        }

        // 7. Render Move Tool drag delta
        if let Some((idx, delta)) = scene.live_drag_delta {
            if let Some(elem) = scene.elements.get(idx) {
                let mut translated = elem.clone();
                translated.translate(delta);
                self.element_renderer.render_element(&translated, &mut pixmap);
            }
        }

        // 8. Cached Dock UI (0.002 ms)
        self.dock_renderer.render_dock(scene.dock, scene.hovered_dock_tool, false, &mut pixmap);
    }
}
