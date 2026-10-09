// Absolute anchoring and edge constraint derivation.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Anchors {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

impl Anchors {
    pub fn derive_size(&self, parent_width: f32, parent_height: f32) -> (Option<f32>, Option<f32>) {
        let width = match (self.left, self.right) {
            (Some(l), Some(r)) => Some((parent_width - l - r).max(0.0)),
            _ => None,
        };
        let height = match (self.top, self.bottom) {
            (Some(t), Some(b)) => Some((parent_height - t - b).max(0.0)),
            _ => None,
        };
        (width, height)
    }
}
