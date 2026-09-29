// Single responsibility: Strongly-typed identifiers for reactive signals and subscribers.

/// Unique index identifier for a reactive signal storage slot.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SignalId(pub usize);

/// Unique handle identifier for an active reactive dependency subscriber.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriberId(pub usize);
