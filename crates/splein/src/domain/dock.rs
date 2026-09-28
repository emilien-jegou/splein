// crates/splein/src/domain/dock.rs

use super::canvas::Rgba;
use super::geometry::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTool {
    Pointer,      // q
    Pen,          // w
    Highlighter,  // e
    Text,         // r
    SelectRegion, // t
    Line,         // y
    Rect,         // u
    Ellipse,      // i
    Eraser,       // o
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockInteraction {
    GripDragStart,
    ColorSelected,
    ToolSelected(ActiveTool),
    ClearRequested,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoveredTool {
    pub tool: ActiveTool,
    pub keybind: char,
    pub center_x: f32,
}

pub struct Dock {
    pub position: Vec2,
    pub width: f32,
    pub height: f32,
    pub current_tool: ActiveTool,
    pub current_color_idx: usize,
    pub colors: [Rgba; 5],
    pub is_dragging: bool,
    drag_anchor: Vec2,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            position: Vec2::new(0.0, 16.0),
            width: 902.0,
            height: 46.0,
            current_tool: ActiveTool::Pen,
            current_color_idx: 0,
            colors: [
                Rgba::from_rgba8(0x38, 0xE1, 0xEE, 255), // 1. #38E1EE (Cyan)
                Rgba::from_rgba8(0x67, 0xFC, 0x43, 255), // 2. #67FC43 (Green)
                Rgba::from_rgba8(0xF3, 0xDE, 0x00, 255), // 3. #F3DE00 (Yellow)
                Rgba::from_rgba8(0xF2, 0x3E, 0x48, 255), // 4. #F23E48 (Red)
                Rgba::from_rgba8(0xCF, 0x00, 0xFF, 255), // 5. #CF00FF (Purple)
            ],
            is_dragging: false,
            drag_anchor: Vec2::ZERO,
        }
    }
}

impl Dock {
    pub fn active_color(&self) -> Rgba {
        self.colors[self.current_color_idx]
    }

    pub fn set_tool(&mut self, tool: ActiveTool) {
        self.current_tool = tool;
    }

    pub fn set_color_idx(&mut self, idx: usize) {
        if idx < self.colors.len() {
            self.current_color_idx = idx;
            if matches!(self.current_tool, ActiveTool::Pointer | ActiveTool::Eraser) {
                self.current_tool = ActiveTool::Pen;
            }
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.position.x
            && p.x <= self.position.x + self.width
            && p.y >= self.position.y
            && p.y <= self.position.y + self.height
    }

    pub fn get_hovered_tool(&self, p: Vec2) -> Option<HoveredTool> {
        if !self.contains(p) {
            return None;
        }

        let rel_x = p.x - self.position.x;
        let tools_start_x = 327.0;
        let cell_step = 52.0; // 42px + 10px gap

        let tool_keys = [
            (ActiveTool::Pointer, 'q'),
            (ActiveTool::Pen, 'w'),
            (ActiveTool::Highlighter, 'e'),
            (ActiveTool::Text, 'r'),
            (ActiveTool::SelectRegion, 't'),
            (ActiveTool::Line, 'y'),
            (ActiveTool::Rect, 'u'),
            (ActiveTool::Ellipse, 'i'),
            (ActiveTool::Eraser, 'o'),
        ];

        for (idx, &(tool, key)) in tool_keys.iter().enumerate() {
            let cx = tools_start_x + (idx as f32 * cell_step) + 21.0;
            if (rel_x - cx).abs() <= 21.0 {
                return Some(HoveredTool {
                    tool,
                    keybind: key,
                    center_x: self.position.x + cx,
                });
            }
        }

        None
    }

    pub fn handle_pointer_down(&mut self, p: Vec2) -> Option<DockInteraction> {
        if !self.contains(p) {
            return None;
        }

        let rel_x = p.x - self.position.x;

        // 1. Drag Grip Handle [0..50px]
        if rel_x <= 50.0 {
            self.is_dragging = true;
            self.drag_anchor = Vec2::new(p.x - self.position.x, p.y - self.position.y);
            return Some(DockInteraction::GripDragStart);
        }

        // 2. Swatches (5 colors, 42x42 each, 10px gap) [56..316px]
        let colors_start_x = 56.0;
        let color_step = 52.0;
        for i in 0..5 {
            let cx = colors_start_x + (i as f32 * color_step) + 21.0;
            if (rel_x - cx).abs() <= 21.0 {
                self.set_color_idx(i);
                return Some(DockInteraction::ColorSelected);
            }
        }

        // 3. Tools (9 tools, 42x42 each, 10px gap) [327..795px]
        let tools_start_x = 327.0;
        let tool_keys = [
            ActiveTool::Pointer,
            ActiveTool::Pen,
            ActiveTool::Highlighter,
            ActiveTool::Text,
            ActiveTool::SelectRegion,
            ActiveTool::Line,
            ActiveTool::Rect,
            ActiveTool::Ellipse,
            ActiveTool::Eraser,
        ];

        for (idx, &tool) in tool_keys.iter().enumerate() {
            let cx = tools_start_x + (idx as f32 * 52.0) + 21.0;
            if (rel_x - cx).abs() <= 21.0 {
                self.set_tool(tool);
                return Some(DockInteraction::ToolSelected(tool));
            }
        }

        // 4. Trash (Clear Screen) button [806..848px]
        if rel_x >= 806.0 && rel_x <= 848.0 {
            return Some(DockInteraction::ClearRequested);
        }

        None
    }

    pub fn handle_pointer_move(&mut self, p: Vec2) {
        if self.is_dragging {
            self.position.x = (p.x - self.drag_anchor.x).max(0.0);
            self.position.y = (p.y - self.drag_anchor.y).max(0.0);
        }
    }

    pub fn handle_pointer_up(&mut self) {
        self.is_dragging = false;
    }
}
