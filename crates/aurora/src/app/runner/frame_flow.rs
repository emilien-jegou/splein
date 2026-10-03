// Single responsibility: Per-frame engine tick, overlay injection, and backend presentation sequencing.

use smallvec::SmallVec;
use std::time::{Duration, Instant};

use crate::app::extension::{AppExtension, OverlayCaps};
use crate::app::painter::OverlayPainter;
use crate::app::runner::adapters::{EngineDamage, ExtensionScanline};
use crate::app::runner::backend::{Backend, FramePostProcess, Present, SurfaceRequest};
use crate::foundation::ResolvedRect;
use crate::runtime::Engine;

/// Ticks the engine, injects extension overlays, and presents exactly one frame.
///
/// `resize_to` overrides the viewport when the window geometry changed. Returns the
/// wall-clock duration of the frame so the caller can keep pacing its frame budget.
pub fn present_one(
    engine: &mut Engine,
    backend: &mut dyn Backend,
    extensions: &mut [Box<dyn AppExtension>],
    resize_to: Option<(u32, u32)>,
) -> Duration {
    let t_start = Instant::now();
    let (w, h) = resize_to.unwrap_or_else(|| engine.logical_size());
    if (w, h) != engine.logical_size() {
        backend.resize(w, h);
        engine.resize(w, h);
    }

    let (_stats, _layers, mut diag) = engine.frame();
    let text_ctx = engine.text_context();
    let (lw, lh) = engine.logical_size();
    let caps = OverlayCaps {
        backend: backend.tag(),
        scanline_overlays: backend.supports_scanline(),
        viewport: ResolvedRect::new(0.0, 0.0, lw as f32, lh as f32),
    };

    for ext in extensions.iter_mut() {
        ext.render_overlay(
            &mut OverlayPainter::new(engine.scene_mut()),
            &text_ctx,
            &caps,
            &diag,
        );
    }

    let extra_damage: SmallVec<[ResolvedRect; 4]> = extensions
        .iter()
        .filter_map(|ext| ext.overlay_damage())
        .collect();
    let damage = engine.current_damage().clone();
    let report = {
        let mut resolver = EngineDamage::new(&engine);
        let mut scanline = caps
            .scanline_overlays
            .then(|| ExtensionScanline::new(extensions));
        let frame = Present {
            scene: engine.scene(),
            damage: &damage,
            extra_damage: &extra_damage,
            surface: SurfaceRequest {
                background: engine.background(),
                width: w,
                height: h,
            },
            damage_resolver: &mut resolver,
            post_process: scanline
                .as_mut()
                .map(|pass| pass as &mut dyn FramePostProcess),
        };
        backend.present(frame)
    };
    match report {
        Ok(report) => {
            diag.timings.raster = report.raster;
            diag.timings.present = report.present;
        }
        Err(error) => tracing::error!(%error, "frame presentation failed"),
    }

    let compute_dur = t_start.elapsed();
    diag.timings.total = compute_dur;
    for ext in extensions.iter_mut() {
        ext.on_frame(&diag);
    }
    compute_dur
}
