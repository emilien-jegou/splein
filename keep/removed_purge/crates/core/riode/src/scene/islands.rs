// Single responsibility: scene → compositor-island extraction (Phase 2a).
//
// A compositor island is one scene effect layer carrying a material: the
// unit the Wayland backend promotes to a synchronized subsurface, each with
// not islands — the plugin never touches them, which is how merely-
// transparent groups stay plain.
//
// Pure function over `&Scene`: no Wayland, no protocol, no client API.
// Rects come out in surface coordinates (chunk placement applied, AABB'd),
// in paint order (document order = subsurface stacking order). Nested effect
// layers yield nested islands, each recorded.

use crate::foundation::{Material, Point, ResolvedRect, Transform};
use crate::scene::sink::{chunk_base, lower_chunk, PlacedLayer, SceneSink};
use crate::scene::Scene;

/// One compositor effect island: a scene effect layer resolved to surface
/// coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    /// Layer rect in surface coordinates (chunk placement applied).
    pub rect: ResolvedRect,
    /// The material chain (the single-pass card) for this layer, cloned
    /// from the placed layer.
    pub material: Option<Material>,
    /// Layout-px overscan the card output can reach past `rect`
    /// (lens displacement, frost reach, AA fringe). The backend expands
    pub pad: f32,
}

impl Island {
    pub fn region(&self) -> ResolvedRect {
        let r = self.rect;
        ResolvedRect::new(
            r.x - self.pad,
            r.y - self.pad,
            r.width + 2.0 * self.pad,
            r.height + 2.0 * self.pad,
        )
    }
}

/// Maps a chunk-local rect through an absolute transform, AABB'ing the
/// four mapped corners. Translation (the common case: placements, scroll
/// offsets) stays exact; rotation/scale conservatively bound.
fn map_rect(t: &Transform, r: ResolvedRect) -> ResolvedRect {
    let p0 = t.transform_point(Point::new(r.x, r.y));
    let p1 = t.transform_point(Point::new(r.right(), r.y));
    let p2 = t.transform_point(Point::new(r.x, r.bottom()));
    let p3 = t.transform_point(Point::new(r.right(), r.bottom()));
    let min_x = p0.x.min(p1.x).min(p2.x).min(p3.x);
    let max_x = p0.x.max(p1.x).max(p2.x).max(p3.x);
    let min_y = p0.y.min(p1.y).min(p2.y).min(p3.y);
    let max_y = p0.y.max(p1.y).max(p2.y).max(p3.y);
    ResolvedRect::new(min_x, min_y, (max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
}

struct Collector {
    islands: Vec<Island>,
}

impl SceneSink for Collector {
    fn begin_layer(&mut self, layer: &PlacedLayer) {
        // A material chain is the single-pass card: the island carries the
        // chain + resolved reach. Plain isolation layers (no material) are
        // not islands.
        let Some(material) = layer.material else {
            return;
        };
        let rect = map_rect(&layer.transform, layer.rect);
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        // Resolve the card's outward reach against its own logical
        // geometry (pre-transform box, square corners) — layout px,
        // mirroring the visual-bounds pad.
        let geo = crate::foundation::Geo::new(layer.rect.width, layer.rect.height, [0.0; 4], 0.0);
        let pad = material.resolve(&geo, 1.0).pad;
        self.islands.push(Island {
            rect,
            material: Some(material.clone()),
            pad,
        });
    }
}

/// Extracts compositor islands from a scene, in paint order.
///
/// Walks every chunk through the single shared stack walk
/// (`lower_chunk`): chunk-local rects are placed via `chunk_base`, so the
/// caller never touches chunk coordinates. Deterministic: same scene →
/// same islands, which is what lets the backend diff island lists across
/// frames to sync its subsurface pool.
pub fn extract_islands(scene: &Scene) -> Vec<Island> {
    let mut collector = Collector {
        islands: Vec::new(),
    };
    for chunk in scene.chunks.iter() {
        let base = chunk_base(chunk);
        lower_chunk(&chunk.commands, &base, &mut collector);
    }
    collector.islands
}
