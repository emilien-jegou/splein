// Single responsibility: Bounds commitment helper and software raster receipt.

use crate::foundation::{DamageRegion, ResolvedRect, Transform};
use crate::render::TinySkiaRenderer;
use crate::scene::Scene;
use crate::tree::{NodeId, TreeArena};

/// Execution receipt confirming raster execution and pixel commitment.
#[derive(Clone, Debug, Default)]
pub struct RasterReceipt {
    /// Total number of damage rectangles drawn into the persistent canvas.
    pub rects_rendered: usize,
    /// Whether any rasterization took place.
    pub executed: bool,
}

impl RasterReceipt {
    /// Commits the visual screen-space bounds of nodes to the arena.
    pub fn commit_painted_bounds(arena: &mut TreeArena, root: NodeId) {
        let root_clip = if arena.get(root).style.clip {
            Some(ResolvedRect::new(
                0.0,
                0.0,
                arena.get(root).resolved_rect.width,
                arena.get(root).resolved_rect.height,
            ))
        } else {
            None
        };

        commit_node_painted_bounds(arena, root, Transform::IDENTITY, root_clip);
    }

    /// Renders damaged areas using TinySkia and commits painted bounds.
    #[tracing::instrument(skip_all, fields(rect_count = damage.rects().len()))]
    pub fn execute(
        renderer: &mut TinySkiaRenderer,
        scene: &Scene,
        damage: &DamageRegion,
        arena: &mut TreeArena,
        root: NodeId,
    ) -> Result<Self, String> {
        if damage.is_empty() {
            return Ok(Self::default());
        }

        renderer.render_damage(scene, damage)?;
        Self::commit_painted_bounds(arena, root);

        Ok(Self {
            rects_rendered: damage.rects().len(),
            executed: true,
        })
    }
}

fn commit_node_painted_bounds(
    arena: &mut TreeArena,
    id: NodeId,
    parent_tx: Transform,
    active_clip: Option<ResolvedRect>,
) {
    if !arena.is_valid(id) {
        return;
    }

    let node = arena.get(id);
    let mut tx = parent_tx.multiply(&Transform::from_translation(
        node.resolved_rect.x,
        node.resolved_rect.y,
    ));
    if node.transform != Transform::IDENTITY {
        tx = tx.multiply(&node.transform);
    }

    let node_abs = tx.transform_point(crate::foundation::Point::new(0.0, 0.0));
    let visual_bounds = node.compute_visual_bounds(node_abs, active_clip);

    let next_clip = if node.style.clip {
        let box_rect = ResolvedRect::new(
            node_abs.x,
            node_abs.y,
            node.resolved_rect.width,
            node.resolved_rect.height,
        );
        Some(active_clip.map_or(box_rect, |c| c.intersect(&box_rect)))
    } else {
        active_clip
    };

    arena.get_mut(id).state.last_painted_bounds = Some(visual_bounds);

    let child_count = arena.children(id).len();
    for i in 0..child_count {
        let child = arena.children(id)[i];
        commit_node_painted_bounds(arena, child, tx, next_clip);
    }
}
