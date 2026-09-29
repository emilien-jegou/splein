// Single responsibility: Multi-slot layout constraint memoization and cache validation.

use crate::foundation::{Constraints, IntrinsicSize, ResolvedRect};

/// Multi-slot MRU layout cache preventing multi-pass flexbox eviction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutCache {
    pub intrinsic: Option<(Constraints, IntrinsicSize)>,
    layout_slots: [Option<(Constraints, ResolvedRect)>; 2],
    /// Retained constraints provided by parent during the most recent layout pass.
    pub last_constraints: Option<Constraints>,
}

impl LayoutCache {
    /// Retrieves memoized intrinsic bounding size if constraints match.
    pub fn get_intrinsic(&self, constraints: &Constraints) -> Option<IntrinsicSize> {
        self.intrinsic
            .and_then(|(c, s)| if &c == constraints { Some(s) } else { None })
    }

    /// Retrieves memoized layout bounds matching constraints.
    pub fn get_layout(&self, constraints: &Constraints) -> Option<ResolvedRect> {
        for slot in &self.layout_slots {
            if let Some((c, r)) = slot {
                if c == constraints {
                    return Some(*r);
                }
            }
        }
        None
    }

    /// Stores intrinsic measurement computation for given constraints.
    pub fn store_intrinsic(&mut self, constraints: Constraints, size: IntrinsicSize) {
        self.intrinsic = Some((constraints, size));
    }

    /// Stores layout calculation in MRU slot 0, shifting slot 0 to slot 1.
    pub fn store_layout(&mut self, constraints: Constraints, rect: ResolvedRect) {
        self.last_constraints = Some(constraints);
        if self.layout_slots[0].as_ref().map(|(c, _)| c) == Some(&constraints) {
            self.layout_slots[0] = Some((constraints, rect));
            return;
        }
        self.layout_slots[1] = self.layout_slots[0];
        self.layout_slots[0] = Some((constraints, rect));
    }

    /// Clears layout computation slots while preserving parent constraints for relayout.
    pub fn clear_layout(&mut self) {
        self.layout_slots = [None, None];
    }

    /// Clears intrinsic measurements.
    pub fn clear_intrinsic(&mut self) {
        self.intrinsic = None;
    }

    /// Clears both intrinsic measurements and layout calculations.
    pub fn clear(&mut self) {
        self.clear_intrinsic();
        self.clear_layout();
    }

    /// Complete cache eviction on node detaching or re-parenting.
    pub fn clear_all(&mut self) {
        self.clear();
        self.last_constraints = None;
    }
}
