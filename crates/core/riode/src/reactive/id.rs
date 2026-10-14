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

/// Ownership tag for a registration made while a reload build ran.
///
/// `instance` is the innermost component scope that was ambient at registration time
/// (forensics and per-subtree sweeps); `gen` is the reload build that produced it, and
/// is what commit/abort sweeps match on; `stable` records whether hook state filed under
/// the instance can ever be recalled (anonymous scopes cannot — their registrations are
/// always safe to sweep). Registrations made outside any reload build carry no tag and
/// are never swept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildTag {
    /// Component scope ambient at registration (`module_path!()::Name`, possibly anonymous).
    pub instance: std::rc::Rc<str>,
    /// Reload build generation that produced this registration.
    pub gen: u64,
    /// Whether the scope's hook state survives rebuilds.
    pub stable: bool,
}
