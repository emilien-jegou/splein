// Single responsibility: Stacking context traversal and cached chunk culling against damage.

use crate::foundation::{DamageRegion, ResolvedRect, Transform};
use crate::pipeline::compile::hierarchy::compile_node_hierarchy;
use crate::pipeline::compile::stacking::StackingContext;
use crate::pipeline::overlap::OverlapPlan;
use crate::scene::{Scene, SceneChunk};
use crate::text::TextContext;
use crate::tree::{DirtyFlags, TreeArena};

/// Recursively compiles hierarchical stacking contexts in strict painter's z-order.
pub fn compile_stacking_context(
    arena: &mut TreeArena,
    ctx: &StackingContext,
    scene: &mut Scene,
    text_ctx: &TextContext,
    damage: &DamageRegion,
    viewport: ResolvedRect,
    overlap: &OverlapPlan,
    new_commands: &mut usize,
    cached_chunks: &mut usize,
) {
    let node_state = &arena.get(ctx.node_id).state;
    let is_clean = !node_state.dirty.contains(DirtyFlags::SUBTREE_DIRTY);
    let touches_damage = damage
        .rects()
        .iter()
        .any(|d| node_state.subtree_bounds.intersects(d));

    if is_clean && (!touches_damage || damage.is_empty()) {
        if let Some(cached) = &node_state.retained_chunk {
            if cached.abs_origin == ctx.abs_origin && cached.bounds.overlaps(&viewport) {
                scene.push_chunk(cached.clone());
                *cached_chunks += 1;
                return;
            }
        }
    }

    for neg in &ctx.negative_z {
        compile_stacking_context(
            arena,
            neg,
            scene,
            text_ctx,
            damage,
            viewport,
            overlap,
            new_commands,
            cached_chunks,
        );
    }

    let mut chunk = SceneChunk::new(arena.get(ctx.node_id).state.subtree_bounds, ctx.abs_origin);
    let cmd_start = chunk.commands.len();

    compile_node_hierarchy(
        arena,
        ctx.node_id,
        &mut chunk,
        text_ctx,
        viewport,
        Transform::from_translation(ctx.abs_origin.x, ctx.abs_origin.y),
        true,
    );
    *new_commands += chunk.commands.len() - cmd_start;

    arena.get_mut(ctx.node_id).state.retained_chunk = Some(chunk.clone());
    if chunk.bounds.overlaps(&viewport) {
        scene.push_chunk(chunk);
    }

    for pos in &ctx.positive_z {
        compile_stacking_context(
            arena,
            pos,
            scene,
            text_ctx,
            damage,
            viewport,
            overlap,
            new_commands,
            cached_chunks,
        );
    }
}
