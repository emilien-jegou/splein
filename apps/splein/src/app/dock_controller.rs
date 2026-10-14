// Coordinates dock position, Taffy layout computation, hover states, and drag gestures.

use crate::domain::dock::{ActiveTool, Intent};
use crate::domain::geometry::Vec2;
use crate::render::dock::animation::SlideMotion;
use crate::ui::dock::geometry::DockGeometry;
use crate::ui::dock::layout_tree::compute_dock_layout;

pub struct DockController {
    pub position: Vec2,
    pub geometry: DockGeometry,
    pub motion: SlideMotion,
    pub active_tool: ActiveTool,
    pub active_shape: usize,
    pub submenu_open: bool,
    hover_idx: Option<usize>,
    drag_anchor: Option<Vec2>,
}

impl Default for DockController {
    fn default() -> Self {
        let pos = Vec2::new(0.0, 16.0);
        let geometry = compute_dock_layout(pos.x, pos.y, false).unwrap_or_default();
        Self {
            position: pos,
            geometry,
            motion: SlideMotion::default(),
            active_tool: ActiveTool::Pen,
            active_shape: 2,
            submenu_open: false,
            hover_idx: None,
            drag_anchor: None,
        }
    }
}

impl DockController {
    pub fn current_tool(&self) -> ActiveTool {
        self.active_tool
    }
    pub fn is_animating(&self) -> bool {
        self.motion.is_animating()
    }
    pub fn is_dragging(&self) -> bool {
        self.drag_anchor.is_some()
    }
    pub fn is_over(&self, p: Vec2) -> bool {
        self.geometry.container.contains(p)
            || self.geometry.submenu.map_or(false, |s| s.contains(p))
    }

    pub fn set_screen_width(&mut self, sw: u32) {
        self.position.x = ((sw as f32) - self.geometry.container.w).max(0.0) * 0.5;
        self.recompute();
    }

    pub fn on_down(&mut self, pos: Vec2) -> Option<Intent> {
        if self.geometry.grip.contains(pos) {
            self.drag_anchor = Some(Vec2::new(pos.x - self.position.x, pos.y - self.position.y));
            return None;
        }
        if self.submenu_open {
            if let Some(sub) = self.geometry.submenu {
                if sub.contains(pos) {
                    for (i, b) in self.geometry.submenu_shapes.iter().enumerate() {
                        if b.contains(pos) {
                            self.active_shape = i;
                            self.submenu_open = false;
                            let tool = match i {
                                0 | 1 => ActiveTool::Line,
                                3 => ActiveTool::Ellipse,
                                _ => ActiveTool::Rect,
                            };
                            self.active_tool = tool;
                            self.recompute();
                            return Some(Intent::SelectTool(tool));
                        }
                    }
                }
            }
        }
        for (i, b) in self.geometry.tools.iter().enumerate() {
            if b.contains(pos) {
                let tool = match i {
                    0 => ActiveTool::Pointer,
                    1 => ActiveTool::Pen,
                    2 => ActiveTool::Highlighter,
                    3 => match self.active_shape {
                        0 | 1 => ActiveTool::Line,
                        3 => ActiveTool::Ellipse,
                        _ => ActiveTool::Rect,
                    },
                    4 => ActiveTool::Text,
                    _ => ActiveTool::Eraser,
                };
                self.active_tool = tool;
                if i == 3 {
                    self.submenu_open = !self.submenu_open;
                } else {
                    self.submenu_open = false;
                }
                self.recompute();
                return Some(Intent::SelectTool(tool));
            }
        }
        None
    }

    pub fn on_move(&mut self, pos: Vec2) -> bool {
        if let Some(anchor) = self.drag_anchor {
            self.position = Vec2::new((pos.x - anchor.x).max(0.0), (pos.y - anchor.y).max(0.0));
            self.recompute();
            return true;
        }
        let prev_hover = self.hover_idx;
        self.hover_idx = self.geometry.tools.iter().position(|b| b.contains(pos));
        let changed = prev_hover != self.hover_idx;
        if changed {
            let target_i = self.hover_idx.unwrap_or(match self.active_tool {
                ActiveTool::Pointer => 0,
                ActiveTool::Pen => 1,
                ActiveTool::Highlighter => 2,
                ActiveTool::Rect | ActiveTool::Line | ActiveTool::Ellipse => 3,
                ActiveTool::Text => 4,
                _ => 5,
            });
            let rel_x = self.geometry.tools[target_i].x - self.geometry.tools[0].x;
            self.motion
                .set_target(rel_x, self.geometry.tools[target_i].w);
        }
        changed
    }

    pub fn on_up(&mut self) {
        self.drag_anchor = None;
    }
    pub fn tick(&mut self, dt_secs: f32) -> bool {
        self.motion.tick(dt_secs)
    }

    fn recompute(&mut self) {
        if let Ok(g) = compute_dock_layout(self.position.x, self.position.y, self.submenu_open) {
            self.geometry = g;
        }
    }
}
