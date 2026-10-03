// Single responsibility: Multi-stage visual inspector and telemetry extension state manager.

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
use crate::foundation::{DamageRegion, ResolvedRect};
use crate::runtime::FrameDiagnostics;
use crate::text::TextContext;

/// Visual inspection modes mapping to pipeline stages.
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
    last_diag: FrameDiagnostics,
    viewport: ResolvedRect,
    last_frame_instant: Option<Instant>,
    smoothed_fps: f32,
    is_idle: bool,
    prev_show_hud: bool,
    prev_mode: InspectorMode,
    worker: TelemetryWorker,
}

impl Default for VisualDebugger {
    fn default() -> Self {
        Self {
            mode: InspectorMode::Off,
            show_hud: true,
            backend_name: "CPU".into(),
            last_diag: FrameDiagnostics::default(),
            viewport: ResolvedRect::ZERO,
            last_frame_instant: None,
            smoothed_fps: 60.0,
            is_idle: true,
            prev_show_hud: true,
            prev_mode: InspectorMode::Off,
            worker: TelemetryWorker::spawn(),
        }
    }
}

impl VisualDebugger {
    /// Constructs a visual debugger with default overlays enabled.
    pub fn new() -> Self {
        Self::default()
    }
}

impl AppExtension for VisualDebugger {
    fn on_event(&mut self, event: &WindowEvent) -> bool {
        handle_debug_key(event, &mut self.mode, &mut self.show_hud)
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
        if self.mode != self.prev_mode {
            Some(self.viewport)
        } else if self.show_hud || self.show_hud != self.prev_show_hud {
            Some(hud_panel())
        } else {
            None
        }
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
                    text_ctx,
                );
            });
        }
    }

    fn on_present(&mut self, buf: &mut [u32], _dmg: &DamageRegion, stride: usize, h: usize) {
        draw_inspector_overlay(buf, self.mode, &self.last_diag, stride, h);
        if self.show_hud {
            draw_hud_panel(
                buf,
                self.is_idle,
                self.smoothed_fps.round() as u32,
                &self.backend_name,
                &self.last_diag,
                self.mode,
                stride,
                h,
            );
        }
    }
}
