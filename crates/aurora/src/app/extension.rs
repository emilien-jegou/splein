// Single responsibility: Pluggable application extension trait contracts.

use crate::app::painter::OverlayPainter;
use crate::foundation::{DamageRegion, ResolvedRect};
use crate::runtime::{Engine, FrameDiagnostics};
use crate::text::TextContext;
use winit::event::WindowEvent;

/// Presentation capabilities an overlay must adapt itself to.
#[derive(Copy, Clone, Debug)]
pub struct OverlayCaps {
    /// Human-readable backend tag, e.g. `"CPU"` or `"GPU"`.
    pub backend: &'static str,
    /// Whether `on_present` will run, so scene-chunk overlays must cover the gap.
    pub scanline_overlays: bool,
    /// Full window bounds in scene coordinates, for overlay framing and damage.
    pub viewport: ResolvedRect,
}

/// Extension hooks for events, telemetry, scene overlays, and presentation.
pub trait AppExtension {
    /// Handles native window input events; returns true if redraw requested.
    fn on_event(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    /// Runs before each frame with engine access; returns true if redraw requested.
    fn on_update(&mut self, _engine: &mut Engine) -> bool {
        false
    }

    /// Observes telemetry snapshot for an executed frame.
    fn on_frame(&mut self, _diagnostics: &FrameDiagnostics) {}

    /// Polled during idle intervals; returns true if redraw requested.
    fn on_idle(&mut self) -> bool {
        false
    }

    /// Injects declarative overlay drawing into the frame display list.
    fn render_overlay(
        &mut self,
        _paint: &mut OverlayPainter<'_>,
        _text_ctx: &TextContext,
        _caps: &OverlayCaps,
        _diagnostics: &FrameDiagnostics,
    ) {
    }

    /// Damaged screen bounds requested by extension overlay.
    fn overlay_damage(&self) -> Option<ResolvedRect> {
        None
    }

    /// Software scanline post-processing hook, run only on backends supporting it.
    fn on_present(
        &mut self,
        _buffer: &mut [u32],
        _damage: &DamageRegion,
        _stride: usize,
        _height: usize,
    ) {
    }
}
