// Single responsibility: Frame-driven ticker registry letting any owner advance once per frame.

use crate::reactive::id::BuildTag;

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
    entries: Vec<(u64, Option<BuildTag>, Box<dyn Ticker>)>,
}

impl TickerRegistry {
    /// Advances every ticker, retiring the ones that report completion.
    pub(crate) fn tick(&mut self, dt: f32) {
        if self.entries.is_empty() {
            return;
        }
        let mut index = 0;
        while index < self.entries.len() {
            let keep = self.entries[index].2.advance(dt);
            if keep {
                index += 1;
            } else {
                self.entries.swap_remove(index);
            }
        }
    }

    /// Registers a ticker under the given mount epoch, tagged with its reload build, if any.
    pub(crate) fn push(&mut self, epoch: u64, owner: Option<BuildTag>, ticker: Box<dyn Ticker>) {
        self.entries.push((epoch, owner, ticker));
    }

    /// Drops every ticker from an older mount epoch.
    ///
    /// Full-tree replacement (`Engine::switch_root`) retires the previous
    /// demo's clocks, timers and playback drivers: they belong to an
    /// unmounted tree and would otherwise tick (and request frames) forever.
    /// Tickers of the current epoch — including ones registered during the
    /// new tree's first frame — survive.
    pub(crate) fn retire_old(&mut self, epoch: u64) {
        self.entries.retain(|(e, _, _)| *e == epoch);
    }

    /// Settles one reload build: on commit keeps untagged tickers plus this build's own
    /// (older tagged ones belong to superseded builds); on abort drops exactly this
    /// build's registrations and keeps everything else.
    pub(crate) fn sweep_build(&mut self, gen: u64, commit: bool) {
        self.entries.retain(|(_, owner, _)| match owner {
            None => true,
            Some(tag) => {
                if commit {
                    tag.gen == gen
                } else {
                    tag.gen != gen
                }
            }
        });
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

    /// Number of registered tickers. Tests and diagnostics only.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether any registered ticker still wants frames.
    pub(crate) fn has_active(&self) -> bool {
        self.entries.iter().any(|(_, _, t)| t.is_active())
    }
}
