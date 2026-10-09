// Single responsibility: Visual inspector state and telemetry store for the debugger extension.

use std::cell::Cell;
use std::time::Instant;
use winit::event::WindowEvent;

use crate::app::debug_hud::draw_hud_panel;
use crate::app::debug_layout::hud_panel;
use crate::app::debug_overlay::draw_inspector_overlay;
use crate::app::debug_scene::{emit_hud_overlay, emit_inspector_overlay};
use crate::app::debug_toggle::handle_debug_key;
use crate::app::extension::{AppExtension, OverlayCaps};
use crate::app::painter::OverlayPainter;
use crate::app::telemetry::TelemetryWorker;
use aurora_foundation::{DamageRegion, ResolvedRect};
use crate::runtime::{Engine, FrameDiagnostics};
use crate::text::TextContext;

/// Visual inspection modes, one per frame pass whose output they visualize.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InspectorMode {
    /// No visual inspector overlays.
    Off,
    /// Visualizes damaged tiles and preserved canvas geometry.
    DamageHeatmap,
    /// Visualizes flexbox layout boundaries and recomputed nodes.
    LayoutAndBoundaries,
    /// Visualizes stacking contexts and z-index ordering.
    StackingAndHierarchy,
}

/// Interactive in-engine visual debugger overlaying pipeline diagnostics.
pub struct VisualDebugger {
    /// Active visual overlay inspection mode.
    pub mode: InspectorMode,
    /// Whether the on-screen diagnostics HUD is rendered.
    pub show_hud: bool,
    /// Human-readable backend name displayed in telemetry badge.
    pub backend_name: String,
    /// Rate the engine's animation clock runs at, driven from the keyboard.
    ///
    /// Held here rather than read back from the engine so the shortcut that starts a slow
    /// motion and the badge that reports it cannot disagree.
    time_scale: f32,
    /// Whether a single-frame step was requested since the last update.
    step_requested: bool,
    /// Whether the next presented frame should be written out as a PNG.
    capture_pending: bool,
    /// How many captures have been taken, used to number the files.
    captures: u32,
    /// Display list recorded at capture time, written out beside the frame.
    scene_dump: Option<String>,
    last_diag: FrameDiagnostics,
    viewport: ResolvedRect,
    last_frame_instant: Option<Instant>,
    smoothed_fps: f32,
    is_idle: bool,
    prev_show_hud: bool,
    prev_mode: InspectorMode,
    /// Overlay footprint drawn last frame.
    ///
    /// Scanline overlays tint the OS surface buffer directly, and damage
    /// tracking never covers overlay pixels — so without repainting where
    /// the overlay drew last frame, stale tint/wireframe pixels persist
    /// forever and red boxes accumulate. This footprint is included in
    /// `overlay_damage` every frame so old overlay pixels are replaced
    /// with clean canvas pixels before the new overlay draws.
    prev_overlay: Cell<Option<ResolvedRect>>,
    worker: TelemetryWorker,
}

impl Default for VisualDebugger {
    fn default() -> Self {
        Self {
            mode: InspectorMode::Off,
            show_hud: true,
            backend_name: "CPU".into(),
            time_scale: 1.0,
            step_requested: false,
            capture_pending: false,
            captures: 0,
            scene_dump: None,
            last_diag: FrameDiagnostics::default(),
            viewport: ResolvedRect::ZERO,
            last_frame_instant: None,
            smoothed_fps: 60.0,
            is_idle: true,
            prev_show_hud: true,
            prev_mode: InspectorMode::Off,
            prev_overlay: Cell::new(None),
            worker: TelemetryWorker::spawn(),
        }
    }
}

impl VisualDebugger {
    /// Constructs a visual debugger with default overlays enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// The rate this debugger is driving the engine's animation clock at.
    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }

    /// Sets the rate to drive the engine's animation clock at.
    ///
    /// The keyboard shortcuts go through here too, so an embedder that starts the app slowed
    /// down and an embedder that presses a key afterwards end up in the same state rather than
    /// in two rates that quietly fight each other on the next frame.
    pub fn set_time_scale(&mut self, scale: f32) {
        self.time_scale = scale.max(0.0);
    }

    /// Writes the presented pixels to `/tmp/aurora-frame-N.png`.
    ///
    /// The engine's headless raster and the window backend are separate retained canvases fed
    /// the same scene, so a bug in retention or presentation can be invisible to a headless
    /// capture and obvious on screen. When that happens the only way to see the real thing is
    /// to save the real thing, which is what this does.
    fn write_capture(&mut self, buf: &[u32], stride: usize, height: usize) {
        let Some(mut pm) = tiny_skia::Pixmap::new(stride as u32, height as u32) else {
            tracing::error!("could not allocate a {stride}x{height} capture");
            return;
        };
        {
            let px = pm.pixels_mut();
            for y in 0..height {
                for x in 0..stride {
                    let v = buf[y * stride + x];
                    // Presented pixels are 0x00RRGGBB words; the alpha byte is unused.
                    px[y * stride + x] = tiny_skia::ColorU8::from_rgba(
                        ((v >> 16) & 0xFF) as u8,
                        ((v >> 8) & 0xFF) as u8,
                        (v & 0xFF) as u8,
                        255,
                    )
                    .premultiply();
                }
            }
        }
        self.captures += 1;
        let path = capture_path(self.captures, "png");        match pm.save_png(&path) {
            Ok(()) => tracing::info!(%path, "wrote frame capture"),
            Err(e) => tracing::error!(%path, error = %e, "could not write frame capture"),
        }
        self.write_scene_dump(&path);
    }

    /// Writes a pending scene dump next to a capture path, if one was taken.
    ///
    /// Split out of [`Self::write_capture`] because the GPU backend has no presented
    /// pixels to save: pressing `s` there must still produce the scene dump, which is
    /// the part that explains a wrong frame.
    fn write_scene_dump(&mut self, capture_path: &str) {
        if let Some(dump) = self.scene_dump.take() {
            let list = format!("{}.txt", capture_path.trim_end_matches(".png"));
            match std::fs::write(&list, dump) {
                Ok(()) => tracing::info!(path = %list, "wrote scene dump"),
                Err(e) => tracing::error!(path = %list, error = %e, "could not write scene dump"),
            }
        }
    }
}

/// Numbered capture path under `$AURORA_CAPTURE_DIR` (default `/tmp`).
fn capture_path(captures: u32, ext: &str) -> String {
    let dir = std::env::var("AURORA_CAPTURE_DIR").unwrap_or_else(|_| "/tmp".into());
    format!(
        "{}/aurora-frame-{}.{}",
        dir.trim_end_matches('/'),
        captures,
        ext
    )
}

impl AppExtension for VisualDebugger {
    fn on_event(&mut self, event: &WindowEvent) -> bool {
        let handled = handle_debug_key(
            event,
            &mut self.mode,
            &mut self.show_hud,
            &mut self.time_scale,
            &mut self.step_requested,
        );
        if !handled {
            // Kept separate from the bindings above because a capture is a debugging action on
            // the presented pixels rather than a change to any debug state, and because it has
            // to survive the case where every other binding is spoken for.
            if let winit::event::WindowEvent::KeyboardInput { event: key, .. } = event {
                if key.state == winit::event::ElementState::Pressed
                    && key.logical_key == winit::keyboard::Key::Character("s".into())
                {
                    self.capture_pending = true;
                    return true;
                }
            }
        }
        handled
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        let mut redraw = false;
        if self.step_requested {
            self.step_requested = false;
            engine.step_animation();
            // A step has to be followed by a frame, or the engine would advance while stopped
            // and never present the result.
            redraw = true;
        }
        // Only write when it actually differs. `set_time_scale` on every frame would be
        // harmless today but would keep re-scaling a clock that is mid-accumulator, and the
        // comparison is the cheapest statement that says "nothing to do".
        if (engine.time_scale() - self.time_scale).abs() >= 1e-6 {
            engine.set_time_scale(self.time_scale);
            // Ask for the frame that shows the new rate: a shortcut that changed the clock but
            // left the last frame on screen would look like it had done nothing.
            redraw = true;
        }
        redraw
    }

    fn on_frame(&mut self, diag: &FrameDiagnostics) {
        self.last_diag = diag.clone();
        self.prev_show_hud = self.show_hud;
        self.prev_mode = self.mode;
        let now = Instant::now();
        if let Some(prev) = self.last_frame_instant {
            let dt = (now - prev).as_secs_f32();
            self.is_idle = dt >= 0.20;
            if dt > 0.0001 && !self.is_idle {
                self.smoothed_fps = self.smoothed_fps * 0.85 + (1.0 / dt) * 0.15;
            }
        } else {
            self.is_idle = true;
        }
        self.last_frame_instant = Some(now);
        self.worker.send(diag.clone());
    }

    fn on_idle(&mut self) -> bool {
        if !self.is_idle {
            self.is_idle = true;
            return true;
        }
        false
    }

    fn overlay_damage(&self) -> Option<ResolvedRect> {
        // The overlay draws with last frame's diagnostics, so what it is
        // about to draw (last_diag footprint) and what it drew last frame
        // (prev_overlay) must BOTH be repainted: the presenter copies damage
        // tiles from the clean canvas, then the overlay tints on top.
        // Only the fresh footprint is stored — otherwise a stale region
        // would be re-damaged every idle frame forever. Stale pixels get
        // exactly one cleanup repaint, then silence.
        if self.mode != self.prev_mode {
            self.prev_overlay.set(None);
            return Some(self.viewport);
        }
        let mut fresh: Option<ResolvedRect> = None;
        let mut grow = |rect: &ResolvedRect| {
            fresh = Some(match fresh {
                Some(acc) => acc.union(rect),
                None => *rect,
            });
        };
        if self.mode != InspectorMode::Off {
            for rect in &self.last_diag.spatial.damaged_rects {
                grow(rect);
            }
            for rect in &self.last_diag.spatial.removed_rects {
                grow(rect);
            }
        }
        if self.show_hud || self.show_hud != self.prev_show_hud {
            grow(&hud_panel());
        }
        let out = match (self.prev_overlay.get(), fresh) {
            (Some(a), Some(b)) => Some(a.union(&b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
        self.prev_overlay.set(fresh);
        out
    }

    fn render_overlay(
        &mut self,
        paint: &mut OverlayPainter<'_>,
        text_ctx: &TextContext,
        caps: &OverlayCaps,
        _d: &FrameDiagnostics,
    ) {
        self.backend_name = caps.backend.into();
        self.viewport = caps.viewport;
        if self.capture_pending {
            // Recorded before any overlay chunk is added, so the dump is the engine's own
            // display list. A frame that looks wrong needs the list that produced it.
            self.scene_dump = Some(dump_scene(paint.scene()));
            if !caps.scanline_overlays {
                // No on_present pass will run on this backend, so consume the
                // capture here: the PNG half has no pixels to save, but the
                // scene dump — the part that explains a wrong frame — is written.
                self.capture_pending = false;
                self.captures += 1;
                let path = capture_path(self.captures, "png");
                self.write_scene_dump(&path);
            }
        }
        if caps.scanline_overlays {
            return;
        }
        if self.mode != InspectorMode::Off {
            paint.record(caps.viewport, |chunk| {
                emit_inspector_overlay(chunk, self.mode, &self.last_diag);
            });
        }
        if self.show_hud {
            paint.record(hud_panel(), |chunk| {
                emit_hud_overlay(
                    chunk,
                    self.is_idle,
                    self.smoothed_fps.round() as u32,
                    caps.backend,
                    &self.last_diag,
                    self.mode,
                    self.time_scale,
                    text_ctx,
                );
            });
        }
    }

    fn on_present(&mut self, buf: &mut [u32], _dmg: &DamageRegion, stride: usize, h: usize) {
        // Taken before the overlays are drawn, so the file is the frame as the engine produced
        // it. A rendering bug that the overlays would hide is exactly what this is for.
        if self.capture_pending {
            self.capture_pending = false;
            self.write_capture(buf, stride, h);
        }
        draw_inspector_overlay(buf, self.mode, &self.last_diag, stride, h);
        if self.show_hud {
            draw_hud_panel(
                buf,
                self.is_idle,
                self.smoothed_fps.round() as u32,
                &self.backend_name,
                &self.last_diag,
                self.mode,
                self.time_scale,
                stride,
                h,
            );
        }
    }
}

/// Renders the scene's chunk list and the geometry-bearing commands as text.
///
/// Only the commands that decide *where* something lands are included: offsets, clips, layers
/// and rects. A frame that draws the wrong shape is explained by one of these, and text diffs
/// between two frames far better than screenshots do.
fn dump_scene(scene: &crate::scene::Scene) -> String {
    use aurora_foundation::Radius;
    use crate::scene::SceneCommand;
    let mut out = String::new();
    for (i, c) in scene.chunks.iter().enumerate() {
        out.push_str(&format!(
            "chunk {i} origin=({:.1},{:.1}) placement={:?} fresh={} bounds=({:.1},{:.1}) {:.1}x{:.1}\n",
            c.abs_origin.x, c.abs_origin.y, c.placement, c.fresh,
            c.bounds.x, c.bounds.y, c.bounds.width, c.bounds.height
        ));
        for clip in c.inherited_clips.iter() {
            out.push_str(&format!(
                "    inherited clip ({:.1},{:.1}) {:.1}x{:.1} r{}\n",
                clip.rect.x,
                clip.rect.y,
                clip.rect.width,
                clip.rect.height,
                if clip.radius == Radius::ZERO {
                    "0".to_string()
                } else {
                    format!("{:?}", clip.radius)
                }
            ));
        }
        for cmd in c.commands.iter() {
            match cmd {
                SceneCommand::PushOffset(p) => {
                    out.push_str(&format!("    PushOffset ({:.1},{:.1})\n", p.x, p.y));
                }
                SceneCommand::PushClip { rect, radius } => {
                    out.push_str(&format!(
                        "    PushClip ({:.1},{:.1}) {:.1}x{:.1} r{}\n",
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                        if *radius == Radius::ZERO {
                            "0".to_string()
                        } else {
                            format!("{:?}", radius)
                        }
                    ));
                }
                SceneCommand::PushOpacity(o) => {
                    out.push_str(&format!("    PushOpacity {o:.3}\n"));
                }
                SceneCommand::PushTransform(t) => {
                    out.push_str(&format!("    PushTransform {t:?}\n"));
                }
                SceneCommand::BeginLayer { rect, .. } => {
                    out.push_str(&format!(
                        "    BeginLayer ({:.1},{:.1}) {:.1}x{:.1}\n",
                        rect.x, rect.y, rect.width, rect.height
                    ));
                }
                SceneCommand::DrawRect { rect, appearance } => {
                    out.push_str(&format!(
                        "    DrawRect ({:.1},{:.1}) {:.1}x{:.1} r{:?} fill={:?} opacity={:.3}\n",
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                        appearance.radius,
                        appearance.fill,
                        appearance.opacity
                    ));
                }
                SceneCommand::DrawText { .. } => {
                    out.push_str("    DrawText\n");
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::extension::AppExtension;

    fn diag_with(rects: &[(f32, f32, f32, f32)]) -> FrameDiagnostics {
        let mut diag = FrameDiagnostics::default();
        for &(x, y, w, h) in rects {
            diag.spatial.damaged_rects.push(ResolvedRect::new(x, y, w, h));
        }
        diag
    }

    fn covers(damage: Option<ResolvedRect>, x: f32, y: f32, w: f32, h: f32) -> bool {
        match damage {
            Some(r) => r.x <= x && r.y <= y && r.right() >= x + w && r.bottom() >= y + h,
            None => false,
        }
    }

    /// Overlay pixels persist in the OS surface buffer, so the previous
    /// frame's footprint must be re-damaged even after damage moves
    /// elsewhere — otherwise stale red boxes accumulate forever.
    #[test]
    fn overlay_damage_covers_previous_footprint() {
        let mut dbg = VisualDebugger::new();
        dbg.mode = InspectorMode::DamageHeatmap;
        // HUD off: isolate the heatmap footprint (the HUD panel has its own
        // always-damage path and would mask the assertions below).
        dbg.show_hud = false;

        // First frame after enabling: footprint covers the damage (on_frame
        // already synced prev_mode, so this is the steady path, not the
        // transition viewport).
        dbg.on_frame(&diag_with(&[(10.0, 10.0, 50.0, 50.0)]));
        let first = dbg.overlay_damage();
        assert!(covers(first, 10.0, 10.0, 50.0, 50.0));

        // Damage moves to B: footprint must cover B (about to draw).
        dbg.on_frame(&diag_with(&[(300.0, 300.0, 40.0, 40.0)]));
        let moved = dbg.overlay_damage();
        assert!(covers(moved, 300.0, 300.0, 40.0, 40.0));

        // Damage moves to C: footprint must still cover B (drawn last
        // frame) as well as C, or B's tint pixels survive.
        dbg.on_frame(&diag_with(&[(500.0, 100.0, 20.0, 20.0)]));
        let moved_again = dbg.overlay_damage();
        assert!(covers(moved_again, 300.0, 300.0, 40.0, 40.0));
        assert!(covers(moved_again, 500.0, 100.0, 20.0, 20.0));
    }

    /// Once overlay content stops changing, cleanup repaints exactly once
    /// and then goes quiet — stale regions must not be re-damaged forever.
    #[test]
    fn overlay_damage_goes_quiet_after_cleanup() {
        let mut dbg = VisualDebugger::new();
        dbg.mode = InspectorMode::DamageHeatmap;
        dbg.show_hud = false;

        dbg.on_frame(&diag_with(&[(10.0, 10.0, 50.0, 50.0)]));
        let _ = dbg.overlay_damage();
        dbg.on_frame(&diag_with(&[(300.0, 300.0, 40.0, 40.0)]));
        let _ = dbg.overlay_damage();

        // Empty damage: one cleanup repaint covering the stale footprint…
        dbg.on_frame(&diag_with(&[]));
        let cleanup = dbg.overlay_damage();
        assert!(covers(cleanup, 300.0, 300.0, 40.0, 40.0));

        // …then silence.
        dbg.on_frame(&diag_with(&[]));
        assert!(dbg.overlay_damage().is_none());
    }
}
