// Single responsibility: TinySkia mask stack management with affine transform projection and alpha intersection.

use crate::path::build_rounded_path;
use aurora::foundation::{Radius, ResolvedRect};
use tiny_skia::{FillRule, Mask, Transform};

/// Manages stacked clipping masks projected through coordinate transformations.
pub struct ClipStack {
    /// `None` entries are full-coverage clips: a square clip under a translation-only
    /// transform that already contains the whole damage tile needs no mask. Skipping
    /// the alloc + fill + intersect for those is the common case for page-level clips.
    masks: Vec<Option<Mask>>,
}

impl ClipStack {
    /// Creates an empty clipping mask stack.
    pub fn new() -> Self {
        Self { masks: Vec::new() }
    }

    /// Pushes a rectangular rounded mask into the stack with alpha intersection.
    ///
    /// `tile_w`/`tile_h` are the damage tile the coverage test runs against;
    /// `mask_w`/`mask_h` size the mask itself and MUST match the scratch pixmap:
    /// tiny-skia silently skips draws whose mask dimensions differ from the target.
    pub fn push(
        &mut self,
        rect: &ResolvedRect,
        radius: Radius,
        transform: Transform,
        tile_w: u32,
        tile_h: u32,
        mask_w: u32,
        mask_h: u32,
    ) {
        if clip_covers_tile(rect, radius, transform, tile_w, tile_h) {
            self.masks.push(None);
            return;
        }

        let mut mask = Mask::new(mask_w.max(1), mask_h.max(1)).expect("Mask allocation failed");
        // Same device snap as draws: a snapped border under an unsnapped
        // mask would be clipped up to half a pixel off its true edge.
        let snapped = crate::path::snap_rect_to_device(rect, transform);
        let path = build_rounded_path(&snapped, radius);
        // AA mask: the draw paints beneath use AA coverage (`Paint::default`),
        // so an aliased clip would scissor smooth border/text edges into
        // stair-steps wherever any clip is active (page/layer clips are the
        // common case, not the exception).
        mask.fill_path(&path, FillRule::Winding, true, transform);

        if let Some(prev) = self.masks.iter().rev().find_map(|m| m.as_ref()) {
            let prev_data = prev.data();
            for (dst, &src) in mask.data_mut().iter_mut().zip(prev_data.iter()) {
                *dst = ((*dst as u16 * src as u16) / 255) as u8;
            }
        }
        self.masks.push(Some(mask));
    }

    /// Pops the active clipping mask from the stack.
    ///
    /// Unbalanced pops are ignored rather than unwinding past the base, so a display list
    /// that somehow emits more pops than pushes degrades to "no clip" instead of
    /// silently clipping the rest of the frame.
    pub fn pop(&mut self) {
        if self.masks.pop().is_none() {
            tracing::warn!("clip stack underflow: PopClip without a matching PushClip");
        }
    }

    /// Discards every mask, returning how many were still active.
    ///
    /// Called between damage tiles. Any tile that ended with a mask still pushed would
    /// otherwise bleed that clip into the next, unrelated tile.
    pub fn reset(&mut self) -> usize {
        let depth = self.masks.len();
        if depth > 0 {
            tracing::warn!("clip stack left {depth} mask(es) open at tile end");
        }
        self.masks.clear();
        depth
    }

    /// Returns a reference to the active clipping mask, if present.
    ///
    /// Skipped full-coverage entries contribute nothing: an ancestor that covers the
    /// tile clips nothing away, so the nearest real mask below them is authoritative.
    pub fn current(&self) -> Option<&Mask> {
        self.masks.iter().rev().find_map(|m| m.as_ref())
    }
}

/// Whether a clip provably covers the whole `w`x`h` tile, making a mask pointless.
///
/// Conservative by design: square corners under a translation-only transform, with the
/// translated rect containing the tile. Anything else (rounded corners, scale, skew)
/// takes the mask path. A nested clip inside a skipped one stays correct: intersecting
/// with full coverage is the inner mask alone.
fn clip_covers_tile(
    rect: &ResolvedRect,
    radius: Radius,
    transform: Transform,
    w: u32,
    h: u32,
) -> bool {
    if !matches!(radius, Radius::Scalar(r) if r <= 0.0) {
        return false;
    }
    if transform.has_skew() || transform.has_scale() {
        return false;
    }
    let (x, y) = (rect.x + transform.tx, rect.y + transform.ty);
    x <= 0.0 && y <= 0.0 && x + rect.width >= w as f32 && y + rect.height >= h as f32
}

impl Default for ClipStack {
    fn default() -> Self {
        Self::new()
    }
}
