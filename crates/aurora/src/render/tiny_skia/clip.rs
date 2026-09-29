// Single responsibility: TinySkia mask stack management with affine transform projection and alpha intersection.

use tiny_skia::{FillRule, Mask, Transform};
use crate::foundation::{Radius, ResolvedRect};
use crate::render::tiny_skia::path::build_rounded_path;

/// Manages stacked clipping masks projected through coordinate transformations.
pub struct ClipStack {
    masks: Vec<Mask>,
}

impl ClipStack {
    /// Creates an empty clipping mask stack.
    pub fn new() -> Self {
        Self { masks: Vec::new() }
    }

    /// Pushes a rectangular rounded mask into the stack with alpha intersection.
    pub fn push(&mut self, rect: &ResolvedRect, radius: Radius, transform: Transform, w: u32, h: u32) {
        let mut mask = Mask::new(w, h).expect("Mask allocation failed");
        let path = build_rounded_path(rect, radius);
        mask.fill_path(&path, FillRule::Winding, false, transform);

        if let Some(prev) = self.masks.last() {
            let prev_data = prev.data();
            for (dst, &src) in mask.data_mut().iter_mut().zip(prev_data.iter()) {
                *dst = ((*dst as u16 * src as u16) / 255) as u8;
            }
        }
        self.masks.push(mask);
    }

    /// Pops the active clipping mask from the stack.
    pub fn pop(&mut self) {
        self.masks.pop();
    }

    /// Returns a reference to the active clipping mask, if present.
    pub fn current(&self) -> Option<&Mask> {
        self.masks.last()
    }
}

impl Default for ClipStack {
    fn default() -> Self {
        Self::new()
    }
}
