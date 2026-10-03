// Single responsibility: Orchestrates multi-stage frame layout, damage culling, and scene compilation.

use crate::foundation::{Constraints, DamageRegion, ResolvedRect};
use crate::pipeline::{
    commit_painted_bounds, CompileResult, DamagePlan, LayoutResult, OverlapPlan,
};
use crate::runtime::scheduler::FrameScheduler;
use crate::runtime::{
    CompileDiagnostics, FrameDiagnostics, FrameTimings, InvalidationDiagnostics, LayoutDiagnostics,
    SpatialDiagnostics,
};
use crate::scene::Scene;
use crate::tree::NodeId;

/// Runs stages 1-5 across arena state, emitting compiled scene and telemetry.
#[tracing::instrument(name = "Engine::frame", skip_all)]
pub fn execute_frame_stages(
    scheduler: &mut FrameScheduler,
    root: NodeId,
    scene: &mut Scene,
    w: u32,
    h: u32,
    frame_counter: u64,
    frame_damage: &mut DamageRegion,
    preserved: Option<ResolvedRect>,
) -> (crate::runtime::FrameStats, FrameDiagnostics) {
    let t_start = std::time::Instant::now();

    let t0 = std::time::Instant::now();
    let mut layout_res = LayoutResult::default();
    let stats = {
        let _span = tracing::info_span!("Stage1_2::LayoutPass").entered();
        scheduler.commit_frame(Constraints::tight(w as f32, h as f32), &mut layout_res)
    };
    let t_layout = t0.elapsed();

    let t1 = std::time::Instant::now();
    let removed = scheduler.arena.drain_removed_damage();
    let _plan = {
        let _span = tracing::info_span!("Stage3::DamageAnalysis").entered();
        let plan = DamagePlan::compute(
            &mut scheduler.arena,
            root,
            removed.clone(),
            &scheduler.dirty_nodes_this_frame,
            stats.laid_out,
        );
        for rect in plan.region.rects() {
            frame_damage.push(*rect);
        }
        plan
    };
    let t_spatial = t1.elapsed();

    let overlap = {
        let _span = tracing::info_span!("Stage4::OverlapCulling").entered();
        OverlapPlan::compute(&scheduler.arena, root, frame_damage)
    };

    let viewport = ResolvedRect::new(0.0, 0.0, w as f32, h as f32);
    let t2 = std::time::Instant::now();
    let comp = {
        let _span = tracing::info_span!("Stage5::DisplayListCompile").entered();
        CompileResult::compile(
            &mut scheduler.arena,
            root,
            scene,
            &scheduler.text_ctx,
            frame_damage,
            viewport,
            &overlap,
        )
    };
    let t_compile = t2.elapsed();

    if stats.laid_out || !frame_damage.is_empty() {
        commit_painted_bounds(&mut scheduler.arena, root);
    }

    let diag = FrameDiagnostics {
        frame_index: frame_counter,
        timings: FrameTimings {
            total: t_start.elapsed(),
            reactivity: std::time::Duration::ZERO,
            layout: t_layout,
            spatial: t_spatial,
            compile: t_compile,
            raster: std::time::Duration::ZERO,
            present: std::time::Duration::ZERO,
        },
        invalidation: InvalidationDiagnostics {
            flags: stats.flags,
            nodes_dirtied: stats.nodes_dirtied,
        },
        layout: LayoutDiagnostics {
            recomputed_nodes: layout_res.recomputed_nodes,
            cached_nodes: layout_res.cached_nodes,
            boundary_nodes: layout_res.boundary_nodes,
            total_nodes: layout_res.total_nodes,
            laid_out: stats.laid_out,
        },
        spatial: SpatialDiagnostics {
            damaged_rects: frame_damage.rects().iter().copied().collect(),
            removed_rects: removed,
            preserved_canvas_rect: preserved,
        },
        compile: CompileDiagnostics {
            chunks_emitted: scene.chunks.len(),
            cached_chunks: comp.cached_chunks_reused,
            stacking_contexts: comp.stacking_contexts_count,
            commands_emitted: comp.new_commands_emitted,
            context_bounds: comp.context_bounds,
        },
    };

    (stats, diag)
}
