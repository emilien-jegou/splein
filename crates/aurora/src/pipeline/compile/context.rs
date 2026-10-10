// Single responsibility: Stacking context traversal and cached chunk culling against damage.

use crate::foundation::{DamageRegion, ResolvedRect, Transform};
use crate::pipeline::compile::hierarchy::compile_node_hierarchy;
use crate::pipeline::compile::stacking::StackingContext;
use crate::scene::{Scene, SceneChunk};
use crate::text::TextContext;
use crate::tree::DirtyFlags;
use crate::tree::TreeArena;

/// Recursively compiles hierarchical stacking contexts in strict painter's z-order.
pub fn compile_stacking_context(
    arena: &mut TreeArena,
    ctx: &StackingContext,
    scene: &mut Scene,
    text_ctx: &TextContext,
    damage: &DamageRegion,
    viewport: ResolvedRect,
    new_commands: &mut usize,
    cached_chunks: &mut usize,
    caret_visible: bool,
) {
    let node_state = &arena.get(ctx.node_id).state;
    let is_clean = !node_state.core.dirty.contains(DirtyFlags::SUBTREE_DIRTY);

    // A chunk replayed from cache carries the ancestor clips it was compiled with, so a
    // parent whose box moved or resized invalidates it even when this subtree is clean.
    let clips_match = node_state
        .scene
        .retained_chunk
        .as_ref()
        .is_some_and(|c| c.inherited_clips == ctx.inherited_clips);

    // Damage overlap deliberately does NOT gate re-encoding: damage decides
    // what gets re-rasterized downstream, while content validity is fully
    // described by the conditions below (docs/retention.md §4). Recompiling
    // byte-identical commands because pixels got dirty is the conflation
    // this branch exists to prevent.
    //
    // A replayed own chunk must NOT skip the negative_z/positive_z
    // recursion below: stacking descendants live in separate chunks that
    // only those loops emit. Returning early here drops them from the
    // frame's scene entirely — harmless for retained-canvas damage-tile
    // rendering (stale pixels persist), but a hole for any backend that
    // repaints the scene (fullscreen shade, or clear-on-full-damage).
    let replay = if is_clean && clips_match {
        node_state
            .scene
            .retained_chunk
            .as_ref()
            .map(|cached| {
                // Content is origin-relative, so any origin replays. Cull and
                // emit with the LIVE subtree bounds (recomputed by the damage
                // pass just before compile), never the retained ones, which
                // describe where the chunk was last frame.
                let mut replay = cached.clone();
                replay.abs_origin = ctx.abs_origin;
                replay.bounds = node_state.paint.subtree_bounds;
                replay.fresh = false;
                replay
            })
            .filter(|replay| replay.bounds.overlaps(&viewport))
    } else {
        None
    };

    for neg in &ctx.negative_z {
        compile_stacking_context(
            arena,
            neg,
            scene,
            text_ctx,
            damage,
            viewport,
            new_commands,
            cached_chunks,
            caret_visible,
        );
    }

    match replay {
        Some(replay) => {
            scene.push_chunk(replay);
            *cached_chunks += 1;
        }
        None => {
            let mut chunk = SceneChunk::new(
                arena.get(ctx.node_id).state.paint.subtree_bounds,
                ctx.abs_origin,
            );
            let cmd_start = chunk.commands.len();

            // Placement-time clips, not content: the chunk stream is origin-relative,
            // so ancestor clips are stored absolute and applied by the consumer around
            // the placed stream. This is what lets a moved chunk reuse its encoding.
            chunk.inherited_clips = ctx.inherited_clips.clone();

            compile_node_hierarchy(
                arena,
                ctx.node_id,
                &mut chunk,
                text_ctx,
                viewport,
                Transform::from_translation(ctx.abs_origin.x, ctx.abs_origin.y),
                true,
                caret_visible,
            );

            // Rebase to chunk-local: the root offset becomes (0,0), placement travels
            // in `abs_origin` + `placement` instead of the stream.
            chunk.rebase_to_local();
            chunk.fresh = true;
            // Drawing commands only: chunk framing (push/pop state) is per-chunk overhead,
            // not work, and must not move when a subtree is culled.
            *new_commands += chunk.commands[cmd_start..]
                .iter()
                .filter(|c| c.is_drawing())
                .count();

            arena.get_mut(ctx.node_id).state.scene.retained_chunk = Some(chunk.clone());
            if chunk.bounds.overlaps(&viewport) {
                scene.push_chunk(chunk);
            }
        }
    }

    for pos in &ctx.positive_z {
        compile_stacking_context(
            arena,
            pos,
            scene,
            text_ctx,
            damage,
            viewport,
            new_commands,
            cached_chunks,
            caret_visible,
        );
    }
}
