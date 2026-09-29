// Single responsibility: Offscreen backing store cache with stability heuristics and LRU eviction.

use rustc_hash::FxHashMap;
use tiny_skia::Pixmap;
use crate::scene::command::LayerId;

const MAX_LAYER_CACHE_BYTES: usize = 64 * 1024 * 1024; // 64 MB cap
const STABILITY_THRESHOLD_FRAMES: u32 = 3;

struct LayerEntry {
    pixmap: Pixmap,
    frames_clean: u32,
    last_used: u64,
}

#[derive(Default)]
pub struct LayerCompositor {
    cache: FxHashMap<LayerId, LayerEntry>,
    current_frame: u64,
}

impl LayerCompositor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn advance_frame(&mut self) {
        self.current_frame += 1;
    }

    pub fn invalidate(&mut self, id: LayerId) {
        self.cache.remove(&id);
    }

    pub fn try_get_cached(&mut self, id: LayerId, rw: u32, rh: u32) -> Option<&Pixmap> {
        if let Some(entry) = self.cache.get_mut(&id) {
            if entry.pixmap.width() == rw && entry.pixmap.height() == rh {
                entry.last_used = self.current_frame;
                entry.frames_clean += 1;
                if entry.frames_clean >= STABILITY_THRESHOLD_FRAMES {
                    return Some(&entry.pixmap);
                }
            }
        }
        None
    }

    pub fn store(&mut self, id: LayerId, pixmap: Pixmap) {
        let entry_bytes = (pixmap.width() * pixmap.height() * 4) as usize;
        self.evict_if_needed(entry_bytes);

        self.cache.insert(id, LayerEntry {
            pixmap,
            frames_clean: 1,
            last_used: self.current_frame,
        });
    }

    fn evict_if_needed(&mut self, required_bytes: usize) {
        let mut total_bytes: usize = self.cache.values().map(|e| (e.pixmap.width() * e.pixmap.height() * 4) as usize).sum();
        while total_bytes + required_bytes > MAX_LAYER_CACHE_BYTES && !self.cache.is_empty() {
            let oldest_id = self.cache.iter().min_by_key(|(_, e)| e.last_used).map(|(&id, _)| id);
            if let Some(id) = oldest_id {
                if let Some(e) = self.cache.remove(&id) {
                    total_bytes -= (e.pixmap.width() * e.pixmap.height() * 4) as usize;
                }
            } else {
                break;
            }
        }
    }
}
