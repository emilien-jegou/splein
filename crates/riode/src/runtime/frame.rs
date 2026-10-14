// Single responsibility: Orchestrates the per-frame pass sequence: layout, damage, compile, and bounds commit.

use crate::foundation::{Constraints, DamageRegion, ResolvedRect};
use crate::layout::LayoutResult;
use crate::pipeline::{commit_painted_bounds, CompileResult, DamagePlan};
use crate::runtime::scheduler::FrameScheduler;
use crate::runtime::{
    CacheDiagnostics, CompileDiagnostics, FrameDiagnostics, FrameTimings, InvalidationDiagnostics,
    LayoutDiagnostics,
    SpatialDiagnostics,
};
use crate::scene::Scene;
use crate::tree::NodeId;

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
        crate::reactive::tick(&scheduler.runtime, step);
    }
    // Total time the tickers were given this frame. The post-layout phase instead gets the
    // frame's own scaled delta, so work that lives there (FLIP spring integration, scroll
    // resolution) advances on every present rather than only on whole-quantum frames. Tickers
    // stay quantized for determinism; a zero-step frame would otherwise publish a duplicate
    // scene, which reads as a freeze whenever nothing event-driven (e.g. pointer motion) is
    // masking it.
    //
    // Exception: a frame whose own delta is exactly zero (frozen clock) still honors the
    // tickers' quantum — that is the F8-freeze/`.`-step workflow advancing one quantum
    // while stopped. Without this, post-layout work would ignore single-steps while
    // ticker work obeys them, and a frozen FLIP would never move under inspection.
    let render_dt = scheduler.clock.last_delta();
    let ticked = step * steps as f32;
    let post_dt = if render_dt > 0.0 { render_dt } else { ticked };
    if steps == 0 {
        // Poll the tickers with no time passing. Adopting a new target is not a function of
        // elapsed time, so a frame too short to advance the clock still has to let a ticker
        // notice that its target moved. Without this, an animation triggered while time is
        // slowed or frozen is never observed as started, the engine reports itself idle, and
        // the animation simply never begins.
        crate::reactive::tick(&scheduler.runtime, 0.0);
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
    crate::reactive::run_post_layout(&scheduler.runtime, post_dt);
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
    // Epoch bump before damage: layout placements and post-layout paint
    // drains may have moved geometry without invalidating per-node, so every
    // absolute-transform query from here on re-walks once and memoizes for
    // the rest of the frame (damage → compile → spatial queries). O(1).
    scheduler.arena.bump_tx_frame();
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
        // Scroll shifts travel with the damage they were computed with: the
        // raster blit reads them from this same region. Rect unions
        // downstream (swapchain age) move rects only, so a shift never leaks
        // into another frame.
        for shift in plan.region.scroll_shifts() {
            frame_damage.push_scroll_shift(shift.clip, shift.delta);
        }
        // NOTE: no preserved-rect carve-out here, deliberately. A previous
        // revision subtracted the preserved overlap ("copied verbatim by the
        // present path"), which is true of the *renderer's* persistent canvas
        // (renderer.resize copies old pixels) but NOT of the *surface* buffer:
        // resize reallocates it, so its validity is governed solely by the
        // swapchain age, and a stale nonzero age leaves the "preserved"
        // region unpainted (tear). Engine::resize already pushes the full new
        // viewport; subtracting the overlap back out re-opens the hole.
        // `preserved` survives below for diagnostics only.
        plan
    };
    let t_spatial = t1.elapsed();

    let viewport = ResolvedRect::new(0.0, 0.0, w as f32, h as f32);
    let t2 = std::time::Instant::now();
    // [STAGE0 #19] Idle skip: no layout, no damage, past first frame → reuse scene.
    let idle = frame_counter > 0 && !stats.laid_out && frame_damage.is_empty() && !caret_visible;
    let comp = if idle {
        CompileResult {
            new_commands_emitted: 0,
            cached_chunks_reused: scene.chunks.len(),
            stacking_contexts_count: 0,
            context_bounds: Vec::new(),
        }
    } else {
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
        cache: {
            let s = scheduler.text_ctx.glyph_cache_stats();
            CacheDiagnostics {
                glyph_bytes: s.bytes,
                glyph_cap_bytes: s.cap_bytes,
                glyph_entries: s.entries,
                glyph_hits: s.hits,
                glyph_misses: s.misses,
                glyph_evictions: s.evictions,
            }
        },
    };

    (stats, diag)
}
