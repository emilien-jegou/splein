// Single responsibility: Layout footprint a node presents to its parent.

/// Layout footprint a node presents to its parent.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LayoutPresence {
    /// Occupies its full layout footprint.
    #[default]
    Present,
    /// Contributes no footprint, as if the view had collapsed to nothing.
    Absent,
    /// Holds its footprint while its visuals are rendered elsewhere.
    Placeholder,
}

impl LayoutPresence {
    /// Whether the node contributes any footprint to its parent.
    #[inline(always)]
    pub fn occupies_space(self) -> bool {
        self != Self::Absent
    }
}
