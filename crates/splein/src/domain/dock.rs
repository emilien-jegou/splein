// Models dock domain state and tool selection intents without legacy palettes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTool {
    Pointer,
    Pen,
    Highlighter,
    Text,
    SelectRegion,
    Line,
    Rect,
    Ellipse,
    Eraser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    SelectTool(ActiveTool),
    ClearCanvas,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockModel {
    pub tool: ActiveTool,
}

impl Default for DockModel {
    fn default() -> Self {
        Self { tool: ActiveTool::Pen }
    }
}

impl DockModel {
    pub fn reduce(&self, intent: Intent) -> Self {
        match intent {
            Intent::SelectTool(tool) => Self { tool },
            Intent::ClearCanvas | Intent::Undo => self.clone(),
        }
    }
}
