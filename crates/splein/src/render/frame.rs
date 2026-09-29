// Defines the render frame contract and provides declarative scene construction.

use super::dock::animation::SlideMotion;
use crate::domain::canvas::{Canvas, DrawingElement, Rgba};
use crate::domain::dock::ActiveTool;
use crate::domain::geometry::{CubicBezierSegment, Vec2};
use crate::ui::dock::geometry::DockGeometry;

pub struct CanvasScene<'a> {
    pub elements: &'a [DrawingElement],
    pub is_overlay_active: bool,
    pub hovered_idx: Option<usize>,
    pub is_eraser_hover: bool,
    pub needs_full_rebuild: bool,
    pub new_segments: &'a [(CubicBezierSegment, Rgba, f32, bool)],
    pub live_tip: Option<(Vec2, Vec2, Rgba, f32, bool)>,
    pub active_shape: Option<&'a DrawingElement>,
    pub live_drag_delta: Option<(usize, Vec2)>,
}

pub struct DockScene<'a> {
    pub geometry: &'a DockGeometry,
    pub motion: &'a SlideMotion,
    pub active_tool: ActiveTool,
    pub active_shape: usize,
    pub submenu_open: bool,
}

pub struct Frame<'a> {
    pub canvas: CanvasScene<'a>,
    pub dock: DockScene<'a>,
}

impl<'a> Frame<'a> {
    pub fn assemble(
        canvas: &'a Canvas,
        geom: &'a DockGeometry,
        motion: &'a SlideMotion,
        tool: ActiveTool,
        shape: usize,
        sub_open: bool,
        segs: &'a [(CubicBezierSegment, Rgba, f32, bool)],
        drag: Option<(usize, Vec2)>,
        active: bool,
        hover: Option<usize>,
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
        let active_shape = match canvas.active_element() {
            Some(
                s @ (DrawingElement::Line { .. }
                | DrawingElement::Rect { .. }
                | DrawingElement::Ellipse { .. }),
            ) => Some(s),
            _ => None,
        };
        Self {
            canvas: CanvasScene {
                elements: canvas.elements(),
                is_overlay_active: active,
                hovered_idx: hover,
                is_eraser_hover: tool == ActiveTool::Eraser,
                needs_full_rebuild: canvas.needs_full_rebuild,
                new_segments: segs,
                live_tip,
                active_shape,
                live_drag_delta: drag,
            },
            dock: DockScene {
                geometry: geom,
                motion,
                active_tool: tool,
                active_shape: shape,
                submenu_open: sub_open,
            },
        }
    }
}
