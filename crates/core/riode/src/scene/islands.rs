// Single responsibility: scene → compositor-island extraction (Phase 2a).
//
// A compositor island is one scene effect layer (`BeginLayer` carrying a
// liquid-glass material and/or an element filter): the unit the Wayland
// backend promotes to a synchronized subsurface, each with its own blur
// region + item hint. Plain isolation layers (neither glass nor filter)
// are not islands — the plugin never touches them, which is how
// merely-transparent groups stay plain.
//
// Pure function over `&Scene`: no Wayland, no protocol, no client API.
// Rects come out in surface coordinates (chunk placement applied, corners
// mapped, AABB'd), in paint order (document order = subsurface stacking
// order). Nested effect layers yield nested islands, each recorded.

use crate::foundation::{
    EffectStack, GlassGroup, GlassLayer, GlassMember, Point, Radius, ResolvedRect, Transform,
};
use crate::scene::sink::{chunk_base, lower_chunk, PlacedLayer, SceneSink};
use crate::scene::Scene;

/// One compositor effect island: a scene effect layer resolved to surface
/// coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    /// Layer rect in surface coordinates (chunk placement applied).
    pub rect: ResolvedRect,
    /// Liquid-glass layer content (`None` for filter-only islands, which
    /// the compositor renders as plain blur until Phase 3 owns both
    /// effects). Group members are already mapped to surface coordinates
    /// here (the compositor packs straight from them); `Glass.radius`
    /// still carries the node's declared corners (stamped at emit).
    pub glass: Option<GlassLayer>,
    /// Ordered effect stack (bottom → top); `None` when the layer
    /// declares no `effects()`.
    pub effects: Option<EffectStack>,
    /// Layout-px overscan the effect output can reach past `rect`
    /// (lens displacement, frost reach and the in-shader shadow). The backend
    /// expands the blur region by this; mirrors the visual-bounds pad.
    /// `0.0` for filter-only islands until their spread is modelled.
    pub pad: f32,
}

impl Island {
    /// Corner shape for the item hint. Single islands report the glass
    /// silhouette; groups report square (the union hint is a plain rect —
    /// member rects in the packed wire are authoritative for optics).
    pub fn radius(&self) -> Radius {
        match &self.glass {
            Some(GlassLayer::Single(g, _, _, _, _)) => g.radius,
            _ => Radius::ZERO,
        }
    }

    /// Blur-region rect: `rect` expanded by `pad`, in surface coordinates.
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
        let Some(stack) = layer.effects else {
            return;
        };
        let Some(glass_layer) = stack.glass_layer() else {
            return;
        };
        // Groups collapse to one island: members mapped to surface
        // coordinates (the compositor packs straight from them), the
        // island rect is their union. The packed wire carries the full
        // member list; the union rect + first-member material below are
        // only the blur-region fallback the old wire used.
        let (rect, glass) = match glass_layer {
            GlassLayer::Single(g, b, s, gl, r) => {
                (
                    map_rect(&layer.transform, layer.rect),
                    GlassLayer::Single(g, b, s, gl, r),
                )
            }
            GlassLayer::Group(group) => {
                // Members and clip ride the same surface mapping as the
                // rect; the press touch undergoes the identical point
                // transform (it is group-local, like the member rects).
                // The packed wire then needs no further mapping.
                let members: Vec<GlassMember> = group
                    .members
                    .iter()
                    .map(|m| GlassMember {
                        rect: map_rect(&layer.transform, m.rect),
                        glass: m.glass,
                        backdrop: m.backdrop,
                        light: m.light,
                        radius_scale: m.radius_scale,
                    })
                    .collect();
                let clip = group.clip.map(|c| map_rect(&layer.transform, c));
                let press_touch = group.press_touch.map(|(local, glow)| {
                    let p = layer.transform.transform_point(local);
                    (Point::new(p.x, p.y), glow)
                });
                let mut union = ResolvedRect::ZERO;
                for m in &members {
                    union = if union.is_empty() {
                        m.rect
                    } else if m.rect.is_empty() {
                        union
                    } else {
                        union.union(&m.rect)
                    };
                }
                (
                    union,
                    GlassLayer::Group(GlassGroup {
                        members,
                        smoothing: group.smoothing,
                        clip,
                        opacity: group.opacity,
                        press_touch,
                        spectral: group.spectral,
                        glow: group.glow,
                        refraction: group.refraction,
                    }),
                )
            }
        };
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let pad = glass.damage_pad();
        self.islands.push(Island {
            rect,
            glass: Some(glass),
            effects: Some(stack.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::Effect;
    use crate::foundation::{Backdrop, Blur, Glass, GlassEffect, Glow, Refraction, Spectral};
    use crate::scene::command::LayerId;
    use crate::scene::{SceneChunk, SceneCommand};

    fn chunk_with(commands: Vec<SceneCommand>, origin: Point) -> SceneChunk {
        let mut chunk = SceneChunk::with_commands(
            ResolvedRect::new(0.0, 0.0, 800.0, 600.0),
            origin,
            commands,
        );
        chunk.placement = Transform::IDENTITY;
        chunk
    }

    fn glass_layer(rect: ResolvedRect) -> Vec<SceneCommand> {
        vec![
            SceneCommand::PushOffset(Point::ZERO),
            SceneCommand::BeginLayer {
                id: LayerId(1),
                rect,
                effects: Some(EffectStack::from(vec![GlassEffect::single(
                    Glass::lens(1.0),
                    Backdrop::NONE,
                    Spectral::NONE,
                    Glow::NONE,
                    Refraction::NONE,
                )
                .boxed()])),
                shadow_pad: 0.0,
            },
            SceneCommand::EndLayer { id: LayerId(1) },
        ]
    }

    fn blur_layer(rect: ResolvedRect) -> Vec<SceneCommand> {
        vec![
            SceneCommand::PushOffset(Point::ZERO),
            SceneCommand::BeginLayer {
                id: LayerId(2),
                rect,
                effects: Some(EffectStack::from(vec![Blur::new(8.0).boxed()])),
                shadow_pad: 0.0,
            },
            SceneCommand::EndLayer { id: LayerId(2) },
        ]
    }

    #[test]
    fn skips_plain_layers_and_collects_glass_in_paint_order() {
        let mut scene = Scene::new();
        // Plain isolation layer: no effects → not an island.
        scene.push_chunk(chunk_with(
            vec![
                SceneCommand::PushOffset(Point::ZERO),
                SceneCommand::BeginLayer {
                    id: LayerId(0),
                    rect: ResolvedRect::new(0.0, 0.0, 100.0, 100.0),
                    effects: None,
                    shadow_pad: 0.0,
                },
                SceneCommand::EndLayer { id: LayerId(0) },
            ],
            Point::ZERO,
        ));
        scene.push_chunk(chunk_with(
            glass_layer(ResolvedRect::new(10.0, 20.0, 200.0, 100.0)),
            Point::ZERO,
        ));
        // Effect layer without glass: renders in-engine, not an island.
        scene.push_chunk(chunk_with(
            blur_layer(ResolvedRect::new(300.0, 40.0, 120.0, 80.0)),
            Point::ZERO,
        ));

        let islands = extract_islands(&scene);
        assert_eq!(islands.len(), 1);
        assert_eq!(
            islands[0].rect,
            ResolvedRect::new(10.0, 20.0, 200.0, 100.0)
        );
        assert!(islands[0].glass.is_some());
        assert!(islands[0].effects.is_some());
        assert!(islands[0].pad > 0.0);
    }

    #[test]
    fn applies_chunk_placement_to_island_rects() {
        let mut scene = Scene::new();
        // Realistic chunk: stacking-root offset + rect in absolute coords,
        // rebased to local at finalization; the island must come out back
        // in surface coordinates.
        let origin = Point::new(50.0, 60.0);
        let mut chunk = chunk_with(
            vec![
                SceneCommand::PushOffset(origin),
                SceneCommand::BeginLayer {
                    id: LayerId(1),
                    rect: ResolvedRect::new(0.0, 0.0, 200.0, 100.0),
                    effects: Some(EffectStack::from(vec![GlassEffect::single(
                        Glass::lens(1.0),
                        Backdrop::NONE,
                        Spectral::NONE,
                        Glow::NONE,
                        Refraction::NONE,
                    )
                    .boxed()])),
                    shadow_pad: 0.0,
                },
                SceneCommand::EndLayer { id: LayerId(1) },
            ],
            origin,
        );
        chunk.rebase_to_local();
        scene.push_chunk(chunk);

        let islands = extract_islands(&scene);
        assert_eq!(islands.len(), 1);
        assert_eq!(
            islands[0].rect,
            ResolvedRect::new(50.0, 60.0, 200.0, 100.0)
        );
    }

    #[test]
    fn drops_degenerate_islands() {
        let mut scene = Scene::new();
        scene.push_chunk(chunk_with(
            glass_layer(ResolvedRect::new(10.0, 10.0, 0.0, 50.0)),
            Point::ZERO,
        ));
        assert!(extract_islands(&scene).is_empty());
    }

    #[test]
    fn region_expands_rect_by_pad() {
        let island = Island {
            rect: ResolvedRect::new(10.0, 20.0, 200.0, 100.0),
            glass: Some(GlassLayer::Single(Glass::lens(1.0), Backdrop::NONE, Spectral::NONE, Glow::NONE, Refraction::NONE)),
            effects: None,
            pad: 10.0,
        };
        assert_eq!(
            island.region(),
            ResolvedRect::new(0.0, 10.0, 220.0, 120.0)
        );
        assert_eq!(island.radius(), Radius::ZERO);
    }
}
