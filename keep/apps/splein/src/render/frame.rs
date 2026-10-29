// Defines the render frame contract and provides declarative scene construction.

use super::dock::animation::SlideMotion;
use crate::domain::canvas::{Canvas, DrawingElement, Rgba};
use crate::domain::dock::ActiveTool;
use crate::domain::geometry::{Aabb, CubicBezierSegment, Vec2};
use crate::ui::dock::geometry::DockGeometry;

pub struct CanvasScene<'a> {
    pub elements: &'a [DrawingElement],
    pub is_overlay_active: bool,
    pub hovered_idx: Option<usize>,
    pub is_eraser_hover: bool,
    pub needs_full_rebuild: bool,
    pub new_segments: &'a [(CubicBezierSegment, Rgba, f32)],
    pub live_tip: Option<(Vec2, Vec2, Rgba, f32, bool)>,
    /// Element rendered fresh each frame: shape previews and the active highlighter.
    pub active_element: Option<&'a DrawingElement>,
    /// Indices and delta of elements being moved.
    pub drag: Option<(&'a [usize], Vec2)>,
    /// Indices and base/target rects while resizing.
    pub resize: Option<(&'a [usize], Aabb, Aabb)>,
    /// Selection box drawn for resize handles, above the canvas.
    pub selection: Option<Aabb>,
    /// Region of an active rubber-band selection drag.
    pub band: Option<Aabb>,
}

pub struct DockScene<'a> {
    pub geometry: &'a DockGeometry,
    pub motion: &'a SlideMotion,
    pub active_tool: ActiveTool,
    pub active_shape: usize,
    pub submenu_open: bool,
}

/// Per-frame canvas inputs that vary independently of committed elements.
pub struct CanvasDynamics<'a> {
    pub tool: ActiveTool,
    pub segments: &'a [(CubicBezierSegment, Rgba, f32)],
    pub drag: Option<(&'a [usize], Vec2)>,
    pub resize: Option<(&'a [usize], Aabb, Aabb)>,
    pub selection: Option<Aabb>,
    pub band: Option<Aabb>,
    pub active: bool,
    pub hovered: Option<usize>,
}

/// Dock inputs needed to render the dock scene.
pub struct DockInputs<'a> {
    pub geometry: &'a DockGeometry,
    pub motion: &'a SlideMotion,
    pub shape: usize,
    pub submenu_open: bool,
}

pub struct Frame<'a> {
    pub canvas: CanvasScene<'a>,
    pub dock: DockScene<'a>,
}

impl<'a> Frame<'a> {
    /// Builds a render frame from committed canvas state plus per-frame dynamics.
    pub fn assemble(
        canvas: &'a Canvas,
        dynamics: CanvasDynamics<'a>,
        dock: DockInputs<'a>,
    ) -> Self {
        let live_tip = match canvas.active_element() {
            Some(DrawingElement::Stroke(s)) => s
                .live_tip
                .map(|(f, t)| (f, t, s.color, s.base_width, false)),
            Some(DrawingElement::Highlighter(s)) => {
                s.live_tip.map(|(f, t)| (f, t, s.color, s.base_width, true))
            }
            _ => None,
        };
        // The highlighter renders as one path per frame so its alpha never builds up on itself.
        let active_element = match canvas.active_element() {
            Some(
                s @ (DrawingElement::Highlighter(_)
                | DrawingElement::Line { .. }
                | DrawingElement::Rect { .. }
                | DrawingElement::Ellipse { .. }),
            ) => Some(s),
            _ => None,
        };
        Self {
            canvas: CanvasScene {
                elements: canvas.elements(),
                is_overlay_active: dynamics.active,
                hovered_idx: dynamics.hovered,
                is_eraser_hover: dynamics.tool == ActiveTool::Eraser,
                needs_full_rebuild: canvas.needs_full_rebuild,
                new_segments: dynamics.segments,
                live_tip,
                active_element,
                drag: dynamics.drag,
                resize: dynamics.resize,
                selection: dynamics.selection,
                band: dynamics.band,
            },
            dock: DockScene {
                geometry: dock.geometry,
                motion: dock.motion,
                active_tool: dynamics.tool,
                active_shape: dock.shape,
                submenu_open: dock.submenu_open,
            },
        }
    }
}
