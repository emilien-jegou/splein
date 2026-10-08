// Single responsibility: Bounded memoization cache for rasterized glyph bitmaps.
//
// Shaping keys ((font, glyph, size, variation)) are stable, but swash
// re-rasterizes outlines on every call. This cache turns the per-frame
// per-glyph cost into a hash lookup; the cap adapts within [min, max] on
// hit/eviction feedback so steady text stays resident while animated-size
// churn cannot grow memory without bound.

use crate::text::glyph::GlyphKey;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

/// Floor of the adaptive cap: steady-state text is never evicted below this.
pub const MIN_BUDGET_BYTES: usize = 4 << 20;
/// Ceiling of the adaptive cap: worst-case memory stays provably bounded.
pub const MAX_BUDGET_BYTES: usize = 64 << 20;
/// Lookups per controller window: hit/eviction rates are evaluated at this cadence.
const WINDOW_OPS: u64 = 2048;
/// Evictions per window that justify growing the cap (working set exceeds it).
const GROW_EVICTIONS: u64 = 128;
/// Minimum window hit rate for growth: one-shot scans (all misses) must not
/// inflate the cap — nothing would hit the added room. Requires evidence of
/// reuse before spending memory.
const GROW_MIN_HIT_RATE: f64 = 0.5;
/// Consecutive quiet windows (no evictions, under half full) before shrinking.
const SHRINK_WINDOWS: u32 = 4;
/// Eviction scan bound: full-map LRU scans per eviction turn churn bursts
/// quadratic, so each eviction samples at most this many entries and takes
/// the oldest (Redis-style sampled LRU: approximate, O(1) worst case).
const EVICT_SAMPLES: usize = 64;

/// Owned rasterized bitmap plus placement, as produced by swash.
#[derive(Clone, Debug)]
pub(crate) struct CachedBitmap {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl CachedBitmap {
    /// Constructs a bitmap entry (recency clock is assigned on store).
    pub(crate) fn new(left: i32, top: i32, width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            left,
            top,
            width,
            height,
            data,
        }
    }

    /// Heap footprint of the entry for budget accounting.
    fn bytes(&self) -> usize {
        self.data.len() + std::mem::size_of::<Self>()
    }
}

/// Adaptive budget policy for the glyph bitmap cache.
#[derive(Copy, Clone, Debug)]
pub struct GlyphCacheBudget {
    /// Floor: the cap never shrinks below this.
    pub min_bytes: usize,
    /// Ceiling: the cap never grows above this.
    pub max_bytes: usize,
}

impl Default for GlyphCacheBudget {
    fn default() -> Self {
        Self {
            min_bytes: MIN_BUDGET_BYTES,
            max_bytes: MAX_BUDGET_BYTES,
        }
    }
}

/// Point-in-time telemetry snapshot for HUD/debug consumption.
#[derive(Copy, Clone, Debug, Default)]
pub struct GlyphCacheStats {
    /// Current resident bytes.
    pub bytes: usize,
    /// Current adaptive cap.
    pub cap_bytes: usize,
    /// Resident entries.
    pub entries: usize,
    /// Cumulative hits.
    pub hits: u64,
    /// Cumulative misses.
    pub misses: u64,
    /// Cumulative evictions.
    pub evictions: u64,
}

/// Mutable cache state guarded by a single mutex.
struct Inner {
    entries: FxHashMap<(GlyphKey, u8), Arc<CachedBitmap>>,
    /// Last-use clock per entry; kept beside the bitmaps so shared handles
    /// stay immutable and cheaply cloneable.
    recency: FxHashMap<(GlyphKey, u8), u64>,
    bytes: usize,
    cap: usize,
    clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    // Controller window state.
    window_ops: u64,
    window_hits: u64,
    window_evictions: u64,
    quiet_windows: u32,
}

/// Retained thread-safe bitmap cache with an adaptive memory cap.
#[derive(Clone)]
pub struct GlyphBitmapCache {
    inner: Arc<Mutex<Inner>>,
    budget: GlyphCacheBudget,
}

impl GlyphBitmapCache {
    /// Constructs a clean cache under the default budget.
    pub fn new() -> Self {
        Self::with_budget(GlyphCacheBudget::default())
    }

    /// Constructs a clean cache under an explicit or fixed budget.
    ///
    /// A `min == max` budget pins the cap (deterministic/embedded targets).
    pub fn with_budget(budget: GlyphCacheBudget) -> Self {
        let cap = budget.min_bytes.max(1).min(budget.max_bytes.max(1));
        Self {
            inner: Arc::new(Mutex::new(Inner {
                entries: FxHashMap::default(),
                recency: FxHashMap::default(),
                bytes: 0,
                cap,
                clock: 0,
                hits: 0,
                misses: 0,
                evictions: 0,
                window_ops: 0,
                window_hits: 0,
                window_evictions: 0,
                quiet_windows: 0,
            })),
            budget,
        }
    }

    /// Looks up a bitmap for one subpixel phase, marking it most-recently-used.
    ///
    /// Returns a shared handle (one atomic bump, no data copy): the caller
    /// holds it across compositing instead of the cache lock.
    pub(crate) fn get(&self, key: &GlyphKey, phase: u8) -> Option<Arc<CachedBitmap>> {
        let id = (*key, phase);
        let mut inner = self.inner.lock().unwrap();
        let handle = Arc::clone(inner.entries.get(&id)?);
        inner.clock += 1;
        let clock = inner.clock;
        inner.recency.insert(id, clock);
        inner.hits += 1;
        inner.window_ops += 1;
        inner.window_hits += 1;
        // Settle inline while the lock is held: one acquisition instead of
        // lock/drop/relock, and no window is ever observed half-reset.
        Self::settle_locked(&mut inner, &self.budget);
        Some(handle)
    }

    /// Records a miss for controller accounting when shaping yields no raster.
    pub(crate) fn note_miss(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.misses += 1;
        inner.window_ops += 1;
        Self::settle_locked(&mut inner, &self.budget);
    }

    /// Stores a freshly rasterized phase bitmap, evicting least-recently-used
    /// entries while over the cap.
    pub(crate) fn store(&self, key: GlyphKey, phase: u8, bitmap: CachedBitmap) {
        let id = (key, phase);
        let mut inner = self.inner.lock().unwrap();
        let bytes = bitmap.bytes();
        // Oversized single entries (huge display type) bypass the cache
        // rather than evicting the working set for one glyph.
        if bytes > self.budget.max_bytes {
            return;
        }
        inner.clock += 1;
        let clock = inner.clock;
        if let Some(old) = inner.entries.remove(&id) {
            inner.bytes = inner.bytes.saturating_sub(old.bytes());
            inner.recency.remove(&id);
        }
        inner.bytes += bytes;
        inner.entries.insert(id, Arc::new(bitmap));
        inner.recency.insert(id, clock);
        while inner.bytes > inner.cap {
            if !inner.evict_lru() {
                break;
            }
            inner.evictions += 1;
            inner.window_evictions += 1;
        }
    }

    /// Shrinks toward the floor (e.g. on large viewport shrink).
    ///
    /// Manual hook for embedders: the controller already decays unused room
    /// via quiet windows, so this is only needed when memory must drop
    /// immediately rather than over the next few windows.
    pub fn trim(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cap = self.budget.min_bytes.max(1);
        while inner.bytes > inner.cap {
            if !inner.evict_lru() {
                break;
            }
            inner.evictions += 1;
        }
    }

    /// Returns a telemetry snapshot.
    pub fn stats(&self) -> GlyphCacheStats {
        let inner = self.inner.lock().unwrap();
        GlyphCacheStats {
            bytes: inner.bytes,
            cap_bytes: inner.cap,
            entries: inner.entries.len(),
            hits: inner.hits,
            misses: inner.misses,
            evictions: inner.evictions,
        }
    }

    /// Runs the adaptive controller once per window of lookups.
    ///
    /// Called with the lock held; all window accounting settles in place.
    fn settle_locked(inner: &mut Inner, budget: &GlyphCacheBudget) {
        if inner.window_ops < WINDOW_OPS {
            return;
        }
        let ops = inner.window_ops.max(1);
        let hit_rate = inner.window_hits as f64 / ops as f64;
        let evictions = inner.window_evictions;
        inner.window_ops = 0;
        inner.window_hits = 0;
        inner.window_evictions = 0;
        // Growth needs both pressure (evictions) and evidence of reuse (hit
        // rate): a one-shot scan evicts plenty but would never hit added room.
        if evictions >= GROW_EVICTIONS && hit_rate >= GROW_MIN_HIT_RATE {
            inner.quiet_windows = 0;
            inner.cap = (inner.cap + inner.cap / 4)
                .max(inner.cap + (1 << 20))
                .min(budget.max_bytes);
            return;
        }
        // Quiet window: shrink only after several consecutive ones and only
        // while well under the cap, with hysteresis.
        if evictions == 0 && inner.bytes * 2 < inner.cap {
            inner.quiet_windows += 1;
        } else {
            inner.quiet_windows = 0;
        }
        if inner.quiet_windows < SHRINK_WINDOWS {
            return;
        }
        inner.quiet_windows = 0;
        inner.cap = (inner.bytes * 2)
            .max(budget.min_bytes)
            .min(inner.cap);
        while inner.bytes > inner.cap {
            if !inner.evict_lru() {
                break;
            }
            inner.evictions += 1;
        }
    }
}

impl Default for GlyphBitmapCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Inner {
    /// Removes the least-recently-used entry; false when empty.
    ///
    /// Samples at most `EVICT_SAMPLES` entries instead of scanning the whole
    /// map: eviction storms (many over-cap inserts in a row) stay O(1) per
    /// victim instead of going quadratic over the resident set.
    fn evict_lru(&mut self) -> bool {
        let victim = self
            .recency
            .iter()
            .take(EVICT_SAMPLES)
            .min_by_key(|(_, used)| **used)
            .map(|(k, _)| *k);
        match victim {
            Some(key) => {
                if let Some(old) = self.entries.remove(&key) {
                    self.bytes = self.bytes.saturating_sub(old.bytes());
                }
                self.recency.remove(&key);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(w: u32, h: u32) -> CachedBitmap {
        CachedBitmap::new(0, 0, w, h, vec![128u8; (w * h) as usize])
    }

    #[test]
    fn store_and_hit() {
        let cache = GlyphBitmapCache::new();
        assert!(cache.get(&GlyphKey(1), 0).is_none());
        cache.note_miss();
        cache.store(GlyphKey(1), 0, bitmap(4, 4));
        let hit = cache.get(&GlyphKey(1), 0).expect("must hit");
        assert_eq!(hit.data.len(), 16);
        // Phases are distinct entries: same glyph, other phase misses.
        assert!(cache.get(&GlyphKey(1), 1).is_none());
        let stats = cache.stats();
        assert_eq!((stats.hits, stats.misses), (1, 1));
    }

    #[test]
    fn fixed_budget_evicts_lru() {
        // Entries cost data + struct overhead (~56 B for 4x4); size the cap
        // so two fit but three force exactly one eviction.
        let cache = GlyphBitmapCache::with_budget(GlyphCacheBudget {
            min_bytes: 176,
            max_bytes: 176,
        });
        cache.store(GlyphKey(1), 0, bitmap(4, 4));
        cache.store(GlyphKey(2), 0, bitmap(4, 4));
        // Advance the clock on key 1 so key 2 is the victim.
        cache.get(&GlyphKey(1), 0);
        cache.store(GlyphKey(3), 0, bitmap(8, 8));
        assert!(cache.get(&GlyphKey(1), 0).is_some());
        assert!(cache.get(&GlyphKey(3), 0).is_some());
        let stats = cache.stats();
        assert!(stats.bytes <= 176, "must respect the cap");
        assert!(stats.evictions >= 1);
    }

    #[test]
    fn oversized_entry_bypasses_cache() {
        let cache = GlyphBitmapCache::with_budget(GlyphCacheBudget {
            min_bytes: 64,
            max_bytes: 128,
        });
        cache.store(GlyphKey(9), 0, bitmap(64, 64));
        assert_eq!(cache.stats().entries, 0);
    }
}
