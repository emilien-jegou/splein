// Single responsibility: Intersecting scissor-rect clip stack.

use riode::foundation::{Radius, ResolvedRect};

/// Intersecting square-clip stack. Rounded clips are approximated by their
/// bounding box here; true rounded masking lands with the SDF pipeline.
#[derive(Default)]
pub struct ClipStack {
    stack: Vec<ResolvedRect>,
}

impl ClipStack {
    pub fn reset(&mut self) {
        self.stack.clear();
    }

    pub fn push(&mut self, rect: &ResolvedRect, _radius: Radius) {
        let next = match self.stack.last() {
            Some(top) => top.intersect(rect),
            None => *rect,
        };
        self.stack.push(next);
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn current(&self) -> Option<ResolvedRect> {
        self.stack.last().copied()
    }
}
