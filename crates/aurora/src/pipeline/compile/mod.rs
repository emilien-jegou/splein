// Single responsibility: Compiles display list commands, tracking newly emitted vs cached work.

pub mod context;
pub mod emit;
pub mod hierarchy;
pub mod stacking;

use crate::foundation::{DamageRegion, Point, ResolvedRect};
use crate::pipeline::overlap::OverlapPlan;
use crate::scene::Scene;
use crate::text::TextContext;
use crate::tree::{NodeId, TreeArena};
use context::compile_stacking_context;
use stacking::{build_stacking_tree, StackingContext};

/// Result of display list compilation detailing new work vs cached work.
#[derive(Clone, Debug, Default)]
pub struct CompileResult {
    /// Newly compiled drawing commands emitted this frame.
    pub new_commands_emitted: usize,
    /// Cached display list scene chunks reused from prior frames.
    pub cached_chunks_reused: usize,
    /// Total stacking contexts evaluated.
    pub stacking_contexts_count: usize,
    /// Bounding rectangles of all stacking contexts.
    pub context_bounds: Vec<ResolvedRect>,
}

impl CompileResult {
    /// Compiles tree arena state into display list scene chunks.
    #[tracing::instrument(name = "compile_scene", skip_all)]
    pub fn compile(
        arena: &mut TreeArena,
        root: NodeId,
        scene: &mut Scene,
        text_ctx: &TextContext,
        damage: &DamageRegion,
        viewport: ResolvedRect,
        overlap: &OverlapPlan,
    ) -> Self {
        let (stacking_contexts_count, new_commands_emitted, cached_chunks_reused, context_bounds) =
            compile_scene_instrumented(arena, root, scene, text_ctx, damage, viewport, overlap);

        Self {
            new_commands_emitted,
            cached_chunks_reused,
            stacking_contexts_count,
            context_bounds,
        }
    }
}

/// Instrumented compilation executing stacking tree extraction and chunk generation.
pub fn compile_scene_instrumented(
    arena: &mut TreeArena,
    root: NodeId,
    scene: &mut Scene,
    text_ctx: &TextContext,
    damage: &DamageRegion,
    viewport: ResolvedRect,
    overlap: &OverlapPlan,
) -> (usize, usize, usize, Vec<ResolvedRect>) {
    scene.clear();
    let root_z = arena.get(root).style.z_index;
    let mut root_stack = StackingContext::new(root, root_z, Point::ZERO);
    let mut stacking_contexts: usize = 1;
    let mut context_bounds: Vec<ResolvedRect> = Vec::new();

    let root_tx = arena.get(root).effective_transform();
    build_stacking_tree(
        arena,
        root,
        &mut root_stack,
        root_tx,
        &mut stacking_contexts,
        &mut context_bounds,
    );
    let (mut new_commands, mut cached_chunks) = (0, 0);

    compile_stacking_context(
        arena,
        &root_stack,
        scene,
        text_ctx,
        damage,
        viewport,
        overlap,
        &mut new_commands,
        &mut cached_chunks,
    );
    (
        stacking_contexts,
        new_commands,
        cached_chunks,
        context_bounds,
    )
}
