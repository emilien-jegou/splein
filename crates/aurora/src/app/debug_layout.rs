// Single responsibility: Shared scene-space geometry for visual debugger overlay panels.

use crate::foundation::ResolvedRect;

/// Bounding rect of the diagnostics HUD panel drawn by every backend.
pub fn hud_panel() -> ResolvedRect {
    ResolvedRect::new(12.0, 12.0, 310.0, 48.0)
}
