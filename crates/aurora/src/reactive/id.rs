// Single responsibility: Strongly-typed identifiers for reactive signals and subscribers.

/// Unique identifier for a reactive signal storage slot.
///
/// Generational: `index` names the slot, `generation` names the era. A slot
/// freed and reused bumps its generation, so a handle held across disposal
/// fails loudly on next access instead of silently aliasing an unrelated value.
/// Fresh slots start at generation 0, which always validates — the common case
/// pays one predictable compare.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SignalId {
    /// Slot index into the runtime's value table.
    pub index: usize,
    /// Era of the slot contents; bumped on every reuse.
    pub generation: u32,
}

impl SignalId {
    /// Builds an identifier for a slot era. `pub(crate)`: only the runtime mints these.
    pub(crate) fn new(index: usize, generation: u32) -> Self {
        Self { index, generation }
    }
}

/// Unique handle identifier for an active reactive dependency subscriber.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriberId(pub usize);
