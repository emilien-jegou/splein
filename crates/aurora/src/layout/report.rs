// Single responsibility: Layout pass execution report recording recomputed, cached, and boundary work.

use aurora_foundation::ResolvedRect;

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

    /// Marks that layout calculation ran this frame, without attributing it to a node.
    ///
    /// The root always recomputes on window resize, so reporting it would drown the
    /// signal this report exists for ("what below the root recomputed"). Callers skip
    /// the root's own box and call this instead.
    #[inline]
    pub fn mark_laid_out(&mut self) {
        self.laid_out = true;
    }

    /// Records a node as recomputed in screen coordinates.
    ///
    /// Keeps only outermost boxes: a node that recomputed subsumes its descendants'
    /// reports (children perform before their parent records, so the parent arrives
    /// last and evicts what it contains). Strict containment only — identical boxes
    /// are all kept, so same-geometry siblings never collapse into one.
    #[inline]
    pub fn record_recomputed(&mut self, screen_rect: ResolvedRect) {
        self.recomputed_nodes
            .retain(|r| !(rect_contains(screen_rect, *r) && screen_rect != *r));
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

/// Whether `outer` strictly contains `inner` as axis-aligned boxes.
fn rect_contains(outer: ResolvedRect, inner: ResolvedRect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x + outer.width >= inner.x + inner.width
        && outer.y + outer.height >= inner.y + inner.height
}
