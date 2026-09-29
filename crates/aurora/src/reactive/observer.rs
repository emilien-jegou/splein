// Single responsibility: Target observers subscribing to reactive dependency tracking.

use crate::reactive::id::{SignalId, SubscriberId};

/// Observer target that tracks reactive signal dependencies.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Observer {
    /// Active effect callback identifier.
    Effect(SubscriberId),
    /// Downstream memoized computation identifier.
    Derived(SignalId),
    /// Generic invalidation subscriber handle.
    Subscriber(SubscriberId),
}
