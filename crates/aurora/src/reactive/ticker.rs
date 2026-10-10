// Single responsibility: Frame-driven ticker registry letting any owner advance once per frame.

/// A unit of work that runs once per frame while the runtime is ticking.
///
/// Implementors live outside the reactive layer (animation adapters, clocks, timers) and
/// only ever touch signals, so the registry stays independent of the UI tree.
pub trait Ticker: 'static {
    /// Advances by `dt` seconds, returning whether this ticker should be retained.
    ///
    /// Returning `false` retires the ticker after this call; the frame loop then stops
    /// asking for redraws on its behalf.
    fn advance(&mut self, dt: f32) -> bool;

    /// Whether this ticker still wants frames.
    fn is_active(&self) -> bool {
        true
    }
}

/// Ordered collection of tickers advanced together each frame.
#[derive(Default)]
pub(crate) struct TickerRegistry {
    entries: Vec<(u64, Box<dyn Ticker>)>,
}

impl TickerRegistry {
    /// Advances every ticker, retiring the ones that report completion.
    pub(crate) fn tick(&mut self, dt: f32) {
        if self.entries.is_empty() {
            return;
        }
        let mut index = 0;
        while index < self.entries.len() {
            let keep = self.entries[index].1.advance(dt);
            if keep {
                index += 1;
            } else {
                self.entries.swap_remove(index);
            }
        }
    }

    /// Registers a ticker under the given mount epoch.
    pub(crate) fn push(&mut self, epoch: u64, ticker: Box<dyn Ticker>) {
        self.entries.push((epoch, ticker));
    }

    /// Drops every ticker from an older mount epoch.
    ///
    /// Full-tree replacement (`Engine::switch_root`) retires the previous
    /// demo's clocks, timers and playback drivers: they belong to an
    /// unmounted tree and would otherwise tick (and request frames) forever.
    /// Tickers of the current epoch — including ones registered during the
    /// new tree's first frame — survive.
    pub(crate) fn retire_old(&mut self, epoch: u64) {
        self.entries.retain(|(e, _)| *e == epoch);
    }

    /// Moves `other`'s tickers to the front of this registry, preserving their order.
    pub(crate) fn prepend(&mut self, other: TickerRegistry) {
        if other.entries.is_empty() {
            return;
        }
        let mut merged = other.entries;
        merged.append(&mut self.entries);
        self.entries = merged;
    }

    /// Whether the registry holds no tickers.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether any registered ticker still wants frames.
    pub(crate) fn has_active(&self) -> bool {
        self.entries.iter().any(|(_, t)| t.is_active())
    }
}
