// Single responsibility: Pluggable application extension trait contracts.

use crate::app::painter::OverlayPainter;
use crate::foundation::{Color, DamageRegion, ResolvedRect};
use crate::runtime::{Engine, FrameDiagnostics};
use crate::text::TextContext;
#[cfg(feature = "desktop")]
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
    /// Window clear color, so overlays can pick contrasting colors (e.g. light
    /// vs dark progress bars) without engine access in `render_overlay`.
    pub background: Color,
}

/// Extension hooks for events, telemetry, scene overlays, and presentation.
pub trait AppExtension {
    /// Handles native window input events; returns true if redraw requested.
    /// Desktop-only: headless embedders drive `Engine` directly.
    #[cfg(feature = "desktop")]
    fn on_event(&mut self, _event: &WindowEvent) -> bool {
        false
    }
    /// Runs before each frame with engine access; returns true if redraw requested.
    fn on_update(&mut self, _engine: &mut Engine) -> bool {
        false
    }
    /// Observes the live window each frame with engine access. The hot-reload
    /// host uses this to dress a hidden boot window (title/size/visibility)
    /// once the first UI library loads; window-less embedders never call it.
    #[cfg(feature = "desktop")]
    fn on_window(&mut self, _window: &winit::window::Window, _engine: &mut Engine) {}

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

    /// Downcast hook for the visual debugger, so event loops holding boxed
    /// extensions can reach its concrete telemetry and command API without a
    /// second registry. `None` for every other extension.
    fn as_debugger(&mut self) -> Option<&mut crate::app::debug::VisualDebugger> {
        None
    }
}
