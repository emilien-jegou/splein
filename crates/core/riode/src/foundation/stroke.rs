// Single responsibility: Border outline definition and geometric stroke alignment.

use crate::foundation::color::Color;
use crate::foundation::edges::Edges;
use crate::foundation::fill::Fill;

/// Geometric alignment of a stroke outline relative to boundary edge.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

/// Border outline specification including stroke width, fill, and alignment.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// Uniform width. For per-edge strokes this mirrors the widest edge,
    /// so engines without per-edge support degrade to uniform-max instead
    /// of dropping the border. Prefer [`Stroke::widths`] when reading.
    pub width: f32,
    pub fill: Fill,
    pub align: StrokeAlign,
    /// Per-edge widths (CSS `border-*-width`); overrides `width` when set.
    pub per_edge: Option<Edges<f32>>,
}

impl Stroke {
    /// Creates a new stroke specification.
    pub fn new(width: f32, fill: Fill, align: StrokeAlign) -> Self {
        Self {
            width,
            fill,
            align,
            per_edge: None,
        }
    }

    /// Constructs an interior stroke with a solid color.
    pub fn inside(width: f32, color: Color) -> Self {
        Self {
            width,
            fill: Fill::solid(color),
            align: StrokeAlign::Inside,
            per_edge: None,
        }
    }

    /// Constructs a centered border stroke with a solid color.
    pub fn center(width: f32, color: Color) -> Self {
        Self {
            width,
            fill: Fill::solid(color),
            align: StrokeAlign::Center,
            per_edge: None,
        }
    }

    /// Constructs an exterior stroke with a solid color.
    pub fn outside(width: f32, color: Color) -> Self {
        Self {
            width,
            fill: Fill::solid(color),
            align: StrokeAlign::Outside,
            per_edge: None,
        }
    }

    /// Replaces the uniform width with per-edge widths (solid fill kept).
    /// `width` is updated to the widest edge for legacy readers.
    pub fn per_edge(mut self, edges: Edges<f32>) -> Self {
        self.width = self.width.max(edges.max_width());
        self.per_edge = Some(edges);
        self
    }

    /// Effective per-edge widths: the per-edge override when present,
    /// else the uniform width repeated on all four edges.
    pub fn widths(&self) -> Edges<f32> {
        self.per_edge.unwrap_or_else(|| Edges::all(self.width))
    }
}
