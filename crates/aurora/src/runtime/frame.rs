// Single responsibility: Orchestrates the per-frame pass sequence: layout, damage, compile, and bounds commit.

use aurora_foundation::{Constraints, DamageRegion, ResolvedRect};
use crate::layout::LayoutResult;
use crate::pipeline::{commit_painted_bounds, CompileResult, DamagePlan};
use crate::runtime::scheduler::FrameScheduler;
use crate::runtime::{
    CompileDiagnostics, FrameDiagnostics, FrameTimings, InvalidationDiagnostics, LayoutDiagnostics,
    SpatialDiagnostics,
};
use crate::scene::Scene;
use crate::tree::NodeId;

/// Decomposes `rect` minus `cut` into up to four non-overlapping bands.
///
/// Empties (no exposure on that side) are dropped by `push`, so callers pass all four
/// unconditionally.
fn subtract_rect(rect: ResolvedRect, cut: ResolvedRect) -> Vec<ResolvedRect> {
    vec![
        // Left strip, full height.
        ResolvedRect::new(rect.x, rect.y, cut.x - rect.x, rect.height),
        // Right strip, full height.
        ResolvedRect::new(
            cut.x + cut.width,
            rect.y,
            rect.x + rect.width - (cut.x + cut.width),
            rect.height,
        ),
        // Top strip, between the cut edges.
        ResolvedRect::new(cut.x, rect.y, cut.width, cut.y - rect.y),
        // Bottom strip, between the cut edges.
        ResolvedRect::new(
            cut.x,
            cut.y + cut.height,
            cut.width,
            rect.y + rect.height - (cut.y + cut.height),
        ),
    ]
}

/// Runs the frame passes over arena state, emitting the compiled scene and telemetry.
#[tracing::instrument(name = "Engine::frame", skip_all)]
pub fn run_frame_passes(
    scheduler: &mut FrameScheduler,
    root: NodeId,
    scene: &mut Scene,
    w: u32,
    h: u32,
    frame_counter: u64,
    frame_damage: &mut DamageRegion,
    preserved: Option<ResolvedRect>,
    caret_visible: bool,
) -> (crate::runtime::FrameStats, FrameDiagnostics) {
    let t_start = std::time::Instant::now();

    // The clock already ticked in `Engine::frame`, before input dispatch, so
    // queued gestures measure against this frame's own delta. Ticking again
    // here would consume a second delta per frame and starve forced test
    // deltas one frame early.
    //
    // One clock feeds everything time-driven this frame: the reactive tickers that drive
    // userland animation.
    let t_react0 = std::time::Instant::now();
    let (steps, step) = scheduler.clock.animation_steps();
    for _ in 0..steps {
        aurora_reactive::tick(&scheduler.runtime, step);
    }
    // Total time the tickers were given this frame, handed on to the post-layout phase so a
    // transition that lives there integrates at exactly the same rate as one that lives here.
    let ticked = step * steps as f32;
    if steps == 0 {
        // Poll the tickers with no time passing. Adopting a new target is not a function of
        // elapsed time, so a frame too short to advance the clock still has to let a ticker
        // notice that its target moved. Without this, an animation triggered while time is
        // slowed or frozen is never observed as started, the engine reports itself idle, and
        // the animation simply never begins.
        aurora_reactive::tick(&scheduler.runtime, 0.0);
    }

    let t0 = std::time::Instant::now();
    let mut layout_res = LayoutResult::default();

    // Forget last frame's geometry *before* layout writes this frame's, so a handle read after
    // publication can tell "this frame put it here" from "nothing has measured it yet". Doing it
    // in the other order would leave a stale entry readable for the whole layout pass.
    scheduler.runtime.borrow_mut().begin_geometry_frame();

    let stats = {
        let _span = tracing::info_span!("Pass::Layout").entered();
        scheduler.commit_frame(Constraints::tight(w as f32, h as f32), &mut layout_res)
    };
    let t_layout = t0.elapsed();

    // Publish watched geometry after layout and before anything reads it. This is a table rather
    // than signals, so there is no invalidation to reason about: a handle that reads it is asking
    // a question, and the answer cannot dirty anything.
    if scheduler.runtime.borrow().geometry_watched() {
        let watched = scheduler.runtime.borrow().watched_geometry();
        let snapshot = scheduler.runtime.borrow().snapshot().clone();
        // The runtime hands back opaque packed identities; unpacking restores the
        // generational ids so publication still skips stale (purged/reused) nodes.
        let watched: Vec<NodeId> = watched.iter().map(|&p| NodeId::unpack(p)).collect();
        scheduler
            .arena
            .publish_watched(&watched, |index, rect| snapshot.put(index, rect));
    }

    // Client work that has to see this frame's geometry and still reach this frame's transform.
    //
    // Placed after publication and before damage, so a callback that measures sees final boxes
    // and a value it writes is picked up by this frame's compile rather than the next one. The
    // only thing downstream is damage, and a callback cannot dirty a node — it writes signals,
    // which the reactive layer resolves below — so the region computed here is the one this
    // frame's compile will actually use.
    aurora_reactive::run_post_layout(&scheduler.runtime, ticked);
    let t_react = t_react0.elapsed();

    // Whatever those callbacks wrote into a bound signal is applied here, in the same frame.
    // Without this the value would be picked up next frame, and a FLIP would show the row at its
    // destination for one frame before jumping back to animate.
    let (paint_flags, paint_passes) = scheduler.drain_paint_after_layout();
    let _ = paint_flags;
    // Marks recorded while the commit ran (drain, reconcile, layout, and the
    // paint drain above) are already represented through explicit pushes; only
    // between-frame writes may accumulate from here on. See
    // `FrameScheduler::discard_commit_marks`.
    scheduler.discard_commit_marks();

    let t1 = std::time::Instant::now();
    let removed = scheduler.arena.drain_removed_damage();
    let _plan = {
        let _span = tracing::info_span!("Pass::Damage").entered();
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
        // Preserved canvas pixels are copied verbatim by the present path, so they
        // are never damage: carve the preserved rect out. On resize this is what
        // keeps stationary content undamaged while exposed strips (already in the
        // region, disjoint from preserved) stand.
        if let Some(preserved_rect) = preserved {
            let kept: Vec<ResolvedRect> = frame_damage
                .rects()
                .iter()
                .flat_map(|r| subtract_rect(*r, preserved_rect))
                .collect();
            frame_damage.clear();
            for rect in kept {
                frame_damage.push(rect);
            }
        }
        plan
    };
    let t_spatial = t1.elapsed();

    let viewport = ResolvedRect::new(0.0, 0.0, w as f32, h as f32);
    let t2 = std::time::Instant::now();
    let comp = {
        let _span = tracing::info_span!("Pass::Compile").entered();
        CompileResult::compile(
            &mut scheduler.arena,
            root,
            scene,
            &scheduler.text_ctx,
            frame_damage,
            viewport,
            caret_visible,
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
            reactivity: t_react,
            layout: t_layout,
            spatial: t_spatial,
            compile: t_compile,
            raster: std::time::Duration::ZERO,
            present: std::time::Duration::ZERO,
        },
        invalidation: InvalidationDiagnostics {
            flags: stats.flags,
            nodes_dirtied: stats.nodes_dirtied,
            drain_passes: stats.drain_passes,
            paint_passes,
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
