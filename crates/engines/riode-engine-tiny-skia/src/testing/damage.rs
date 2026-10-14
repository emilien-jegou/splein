//! Damage parity: does the retained canvas equal a from-scratch repaint?
//!
//! Aurora's damage tracking is the part of the pipeline with the least observable behaviour:
//! a bad damage region produces no crash, no wrong geometry and no failed assertion — it
//! produces a handful of stale pixels that a user notices days later. The only reliable check
//! is differential, and that check is subtle enough to be worth having exactly once.
//!
//! Two canvases are compared:
//!
//! - the **retained** one, which is what the window actually shows, accumulated over frames
//!   and only ever partially redrawn;
//! - a **truth** one, rendered from the current scene into a fresh renderer over the whole
//!   viewport.
//!
//! Mismatches are then split by whether the pixel fell inside this frame's damage rects.
//! The split is the entire point: a mismatch *inside* a redrawn tile means the tile was
//! redrawn with different content, which is a painting bug; a mismatch *outside* every tile
//! means the tile was never redrawn when it should have been, which is a damage bug. A single
//! undifferentiated pixel count cannot tell you which one you have.

use riode::foundation::{DamageRegion, ResolvedRect};
use riode::runtime::Engine;

use crate::renderer::TinySkiaRenderer;

/// A pixel-by-pixel comparison of the retained canvas against a full repaint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageParity {
    /// Pixels that differ and fell inside a tile this frame redrew.
    pub covered: u32,
    /// Pixels that differ and fell outside every tile this frame redrew.
    pub uncovered: u32,
}

impl DamageParity {
    /// Fails unless the damage left at most `tolerance` changed pixels outside the tiles it
    /// redrew.
    ///
    /// A small non-zero residue is normal: the boundary of a clip-collapsed record leaves a
    /// few anti-aliased corner pixels that round in one direction in each renderer. The
    /// failures this catches are orders of magnitude larger — an uncleared drag trail was
    /// 25632 pixels — so the tolerance separates a rounding artefact from a missed tile.
    pub fn assert_no_strays(&self, tolerance: u32, subject: &str) {
        assert!(
            self.uncovered <= tolerance,
            "{subject} left {} changed pixels outside the tiles it redrew (tolerance {tolerance}, \
             {} inside)",
            self.uncovered,
            self.covered,
        );
    }
}

/// Whether a pixel at `(x, y)` fell inside a tile this frame's damage asked to be redrawn.
///
/// Mirrors `render_damage`'s floor/ceil tile, which is the region actually redrawn. Testing
/// against the raw rect instead would credit pixels the renderer never touched and quietly
/// turn a missed tile into a pass.
pub fn redrawn_at(x: u32, y: u32, damage: &DamageRegion) -> bool {
    damage.rects().iter().any(|d| {
        x as f32 >= d.x.floor()
            && (x + 1) as f32 <= d.right().ceil()
            && y as f32 >= d.y.floor()
            && (y + 1) as f32 <= d.bottom().ceil()
    })
}

/// Compares the retained canvas against a full repaint of the current scene.
///
/// Does not disturb the engine: the reference is rendered into a throwaway renderer, so the
/// next `frame()` behaves exactly as it would have.
///
/// Must be the SOLE painter of the frame it checks: call after
/// `engine.frame()`, never after `step()`/`canvas()` for the same frame.
/// Tile repaints are idempotent, but scroll blits are moves — painting the
/// same damage twice shifts pixels twice and reports phantom strays.
pub fn parity(engine: &mut Engine, raster: &mut crate::raster::HeadlessRaster) -> DamageParity {
    let size = engine.logical_size();
    let text_ctx = engine.text_context();
    let damage = engine.current_damage().clone();
    let retained = raster
        .draw(crate::raster::HeadlessFrame {
            scene: engine.scene(),
            damage: &damage,
            size,
            text_ctx: &text_ctx,
        })
        .clone();
    let (w, h) = size;

    let mut truth = TinySkiaRenderer::new();
    truth.set_text_context(engine.text_context());
    truth.resize(w, h);
    let mut full = DamageRegion::new();
    full.push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
    truth
        .render_damage(engine.scene(), &full)
        .expect("full repaint of the current scene");
    let truth = truth.canvas();

    let mut out = DamageParity {
        covered: 0,
        uncovered: 0,
    };
    for y in 0..h {
        for x in 0..w {
            if retained.pixel(x, y) == truth.pixel(x, y) {
                continue;
            }
            if redrawn_at(x, y, &damage) {
                out.covered += 1;
            } else {
                out.uncovered += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(rects: &[(f32, f32, f32, f32)]) -> DamageRegion {
        let mut r = DamageRegion::new();
        for (x, y, w, h) in rects {
            r.push(ResolvedRect::new(*x, *y, *w, *h));
        }
        r
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn an_exact_rect_counts_its_own_pixels() {
        let r = region(&[(10.0, 20.0, 30.0, 40.0)]);
        assert!(redrawn_at(10, 20, &r), "top-left corner is inside");
        assert!(redrawn_at(39, 59, &r), "bottom-right corner is inside");
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn a_partial_rect_expands_to_the_tile_the_renderer_actually_touches() {
        // A fractional rect means the renderer paints whole pixels. A pixel straddling the
        // edge is redrawn, so the parity check must count it as covered rather than as a
        // missed tile -- otherwise every fractional damage rect reports a false failure.
        let r = region(&[(10.5, 20.5, 30.0, 40.0)]);
        assert!(
            redrawn_at(10, 20, &r),
            "the pixel the fractional edge lands in is part of the redrawn tile"
        );
        assert!(
            !redrawn_at(9, 20, &r),
            "a pixel entirely before the edge is not"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn pixels_outside_every_rect_are_not_credited() {
        let r = region(&[(0.0, 0.0, 8.0, 8.0), (100.0, 100.0, 8.0, 8.0)]);
        assert!(redrawn_at(4, 4, &r));
        assert!(redrawn_at(104, 104, &r));
        assert!(
            !redrawn_at(50, 50, &r),
            "the gap between two damage rects must count as a miss, or a real missed tile \
             between them would be classified as covered and go unnoticed"
        );
    }

    #[test]
    #[ntest::timeout(30_000)]
    fn an_empty_damage_region_credits_nothing() {
        let r = region(&[]);
        assert!(!redrawn_at(0, 0, &r));
    }
}
