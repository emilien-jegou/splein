// Single responsibility: Executes boundary-isolated flex layouts and records cache telemetry.

use crate::foundation::ResolvedRect;

/// Layout execution result detailing exact work performed this frame.
#[derive(Clone, Debug, Default)]
pub struct LayoutResult {
    /// Screen-space bounds of nodes whose layout was freshly recomputed.
    pub recomputed_nodes: Vec<ResolvedRect>,
    /// Screen-space bounds of nodes served directly from cache.
    pub cached_nodes: Vec<ResolvedRect>,
    /// Screen-space bounds of independent layout boundaries that executed.
    pub boundary_nodes: Vec<ResolvedRect>,
    /// Total nodes inspected during the layout pass.
    pub total_nodes: usize,
    /// Whether any layout calculation was executed.
    pub laid_out: bool,
}

impl LayoutResult {
    /// Creates an empty layout result indicating zero recomputations.
    #[inline(always)]
    pub fn idle() -> Self {
        Self::default()
    }

    /// Records a node as recomputed in screen coordinates.
    #[inline]
    pub fn record_recomputed(&mut self, screen_rect: ResolvedRect) {
        self.recomputed_nodes.push(screen_rect);
        self.total_nodes += 1;
        self.laid_out = true;
    }

    /// Records a node as served from cache in screen coordinates.
    #[inline]
    pub fn record_cached(&mut self, screen_rect: ResolvedRect) {
        self.cached_nodes.push(screen_rect);
        self.total_nodes += 1;
    }

    /// Records an executed 2D layout boundary in screen coordinates.
    #[inline]
    pub fn record_boundary(&mut self, screen_rect: ResolvedRect) {
        self.boundary_nodes.push(screen_rect);
    }
}
