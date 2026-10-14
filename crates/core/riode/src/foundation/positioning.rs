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

impl Anchor {
    /// Client-side offset of a `child_w × child_h` box inside a
    /// `parent_w × parent_h` box under this anchor, in content-space pixels.
    ///
    /// The engine's out-of-flow machinery (exclusion from flex/min-size,
    /// padding-box resolution in `allocate_absolute`) stays in the solver —
    /// that is mechanism. But a preset like "center" is pure arithmetic over
    /// already-known sizes, so user code can position with `absolute()` plus
    /// this helper and never touch the `Anchor` wire format. That is the
    /// client-side path for anchor presets.
    pub fn offset_in(
        self,
        parent_w: f32,
        parent_h: f32,
        child_w: f32,
        child_h: f32,
    ) -> (f32, f32) {
        let x = match self {
            Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => 0.0,
            Anchor::Top | Anchor::Center | Anchor::Bottom => {
                ((parent_w - child_w) * 0.5).max(0.0)
            }
            Anchor::TopRight | Anchor::Right | Anchor::BottomRight => {
                (parent_w - child_w).max(0.0)
            }
        };
        let y = match self {
            Anchor::TopLeft | Anchor::Top | Anchor::TopRight => 0.0,
            Anchor::Left | Anchor::Center | Anchor::Right => {
                ((parent_h - child_h) * 0.5).max(0.0)
            }
            Anchor::BottomLeft | Anchor::Bottom | Anchor::BottomRight => {
                (parent_h - child_h).max(0.0)
            }
        };
        (x, y)
    }
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
