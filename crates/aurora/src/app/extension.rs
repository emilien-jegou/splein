// Single responsibility: Pluggable application extension trait contracts.

use crate::foundation::{DamageRegion, ResolvedRect};
use crate::runtime::FrameDiagnostics;
use crate::scene::Scene;
use crate::text::TextContext;
use winit::event::WindowEvent;

/// Extension hooks for events, telemetry, scene overlays, and presentation.
pub trait AppExtension {
    /// Handles native window input events; returns true if redraw requested.
    fn on_event(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    /// Observes telemetry snapshot for an executed frame.
    fn on_frame(&mut self, _diagnostics: &FrameDiagnostics) {}

    /// Polled during idle intervals; returns true if redraw requested.
    fn on_idle(&mut self) -> bool {
        false
    }

    /// Injects declarative overlay scene chunks directly into the display list.
    fn render_overlay(
        &mut self,
        _scene: &mut Scene,
        _text_ctx: &TextContext,
        _backend_name: &str,
        _diagnostics: &FrameDiagnostics,
    ) {
    }

    /// Damaged screen bounds requested by extension overlay.
    fn overlay_damage(&self) -> Option<ResolvedRect> {
        None
    }

    /// Software scanline post-processing hook for CPU framebuffers.
    fn on_present(
        &mut self,
        _buffer: &mut [u32],
        _damage: &DamageRegion,
        _stride: usize,
        _height: usize,
    ) {
    }
}
