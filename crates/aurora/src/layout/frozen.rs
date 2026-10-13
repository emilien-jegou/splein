// Single responsibility: Build-time-frozen layout snapshots for instant boot.
//
// A fully-static subtree lays out identically on every run for a given
// viewport: same constraints in, same boxes out. `FrozenLayout` captures
// those boxes (plus the constraints they were computed under, for cache
// priming) keyed by stable key-paths, and replays them on a fresh tree —
// skipping measurement for frozen subtrees from frame 0. This is the
// build-time half of AOT static layouts (#7-full); the runtime half is the
// `subtree_static` memo that decides *what* is freezable.
//
// Format (hand-rolled little-endian, no serde dependency):
// `AURF` magic + u32 version + u32 viewport w/h + u32 entry count, then per
// entry: u32 path len + path bytes + 4×f32 constraints
// (min_w/max_w/min_h/max_h) + 4×f32 resolved rect (x/y/w/h).
//
// Soundness rules (all-or-nothing per subtree):
// - Capture records only nodes with a proven `subtree_static` memo.
// - Replay addresses nodes by key-path (`key` segments, `#i` for unkeyed);
//   any structural mismatch aborts that subtree into normal layout.
// - A subtree replays only if EVERY node in it has an entry (dynamic
//   children veto their ancestors — mixed subtrees lay out normally).
// - Viewport must match exactly; fonts/content are the caller's contract
//   (same binary + assets ⇒ same metrics; text shaping itself is #2's job).
// - PAINT flags are untouched: frozen pixels still paint on frame 0.

use crate::foundation::{Constraints, Point, ResolvedRect};
use crate::tree::{DirtyFlags, NodeId, TreeArena};

const MAGIC: u32 = 0x41555246; // "AURF"
const VERSION: u32 = 1;

/// One frozen node: where it measured under what constraints.
#[derive(Clone, Debug)]
struct FrozenEntry {
    path: String,
    constraints: Constraints,
    resolved: ResolvedRect,
}

/// A captured static-layout snapshot, ready to encode or replay.
#[derive(Clone, Debug, Default)]
pub struct FrozenLayout {
    viewport_w: u32,
    viewport_h: u32,
    entries: Vec<FrozenEntry>,
}

fn segment(arena: &TreeArena, id: NodeId, index: usize) -> String {
    match &arena.get(id).key {
        Some(k) => k.0.clone(),
        None => format!("#{index}"),
    }
}

impl FrozenLayout {
    /// Walks proven-static subtrees recording boxes + constraints.
    pub fn capture(arena: &TreeArena, root: NodeId, viewport_w: u32, viewport_h: u32) -> Self {
        let mut out = Self {
            viewport_w,
            viewport_h,
            entries: Vec::new(),
        };
        let root_seg = segment(arena, root, 0);
        Self::capture_sub(arena, root, root_seg, &mut out);
        out
    }

    fn capture_sub(arena: &TreeArena, id: NodeId, path: String, out: &mut Self) {
        let node = arena.get(id);
        // Only proven-static nodes freeze; anything else (or a node whose
        // constraints weren't retained) falls back to live layout.
        let proven = node.state.core.subtree_static == Some(true);
        let constraints = node.state.core.cache.last_constraints;
        if let (true, Some(c)) = (proven, constraints) {
            out.entries.push(FrozenEntry {
                path: path.clone(),
                constraints: c,
                resolved: node.resolved_rect,
            });
        } else {
            return;
        }
        for (i, child) in arena.each_child(id).enumerate() {
            let child_path = format!("{path}/{}", segment(arena, child, i));
            Self::capture_sub(arena, child, child_path, out);
        }
    }

    /// Number of frozen nodes.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether any node was frozen.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Encodes the snapshot to bytes (persist beside the binary / embed).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC.to_le_bytes());
        buf.extend_from_slice(&VERSION.to_le_bytes());
        buf.extend_from_slice(&self.viewport_w.to_le_bytes());
        buf.extend_from_slice(&self.viewport_h.to_le_bytes());
        buf.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for e in &self.entries {
            buf.extend_from_slice(&(e.path.len() as u32).to_le_bytes());
            buf.extend_from_slice(e.path.as_bytes());
            for v in [
                e.constraints.min_width,
                e.constraints.max_width,
                e.constraints.min_height,
                e.constraints.max_height,
                e.resolved.x,
                e.resolved.y,
                e.resolved.width,
                e.resolved.height,
            ] {
                // Non-finite constraints (unbounded maxima) round-trip
                // bit-exact; only the length prefix is validated.
                buf.extend_from_slice(&v.to_bits().to_le_bytes());
            }
        }
        buf
    }

    /// Decodes bytes produced by [`Self::encode`]; `None` on any mismatch
    /// (magic, version, truncation, absurd sizes — never partial state).
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut cur = bytes;
        let mut take = |n: usize| -> Option<&[u8]> {
            if cur.len() < n {
                return None;
            }
            let (h, t) = cur.split_at(n);
            cur = t;
            Some(h)
        };
        let u32le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if u32le(take(4)?) != MAGIC || u32le(take(4)?) != VERSION {
            return None;
        }
        let (viewport_w, viewport_h) = (u32le(take(4)?), u32le(take(4)?));
        let count = u32le(take(4)?);
        if count > 100_000 {
            return None;
        }
        let mut entries = Vec::with_capacity(count.min(4096) as usize);
        for _ in 0..count {
            let len = u32le(take(4)?) as usize;
            if len == 0 || len > 1024 {
                return None;
            }
            let path = std::str::from_utf8(take(len)?).ok()?.to_string();
            let mut floats = [0.0f32; 8];
            for f in floats.iter_mut() {
                *f = f32::from_bits(u32le(take(4)?));
            }
            entries.push(FrozenEntry {
                path,
                constraints: Constraints {
                    min_width: floats[0],
                    max_width: floats[1],
                    min_height: floats[2],
                    max_height: floats[3],
                },
                resolved: ResolvedRect::new(floats[4], floats[5], floats[6], floats[7]),
            });
        }
        if !cur.is_empty() {
            return None;
        }
        Some(Self {
            viewport_w,
            viewport_h,
            entries,
        })
    }

    /// Replays frozen boxes onto a fresh tree. Returns the number of nodes
    /// replayed; 0 (viewport mismatch) means "lay out normally".
    ///
    /// Replay primes each node's `LayoutCache` with the recorded constraints
    /// so the layout pass takes its cache-hit path instead of remeasuring,
    /// and clears stale MEASURE/LAYOUT/SUBTREE flags the fresh reconcile
    /// left behind. PAINT is preserved — frame 0 still rasterizes.
    pub fn apply(&self, arena: &mut TreeArena, root: NodeId, w: u32, h: u32) -> usize {
        if w != self.viewport_w || h != self.viewport_h {
            return 0;
        }
        let root_seg = segment(arena, root, 0);
        // Index by path for O(1) lookup during the walk.
        let mut table = std::collections::HashMap::new();
        for e in &self.entries {
            table.insert(e.path.as_str(), e);
        }
        let n = Self::apply_sub(arena, root, root_seg, Point::ZERO, &table);
        if n > 0 {
            // Replayed boxes rewrite geometry outside the mark path.
            arena.bump_tx_frame();
        }
        n
    }

    /// Replays one subtree; 0 on any mismatch (caller falls back to layout).
    /// Two phases: first prove full coverage (pure read — structural drift
    /// vetoes before a single write), then commit top-down (parent absolute
    /// position threads down, so children land in window coordinates).
    /// All-or-nothing per subtree: a partial replay would assert a parent
    /// box around live-measured children.
    fn apply_sub(
        arena: &mut TreeArena,
        id: NodeId,
        path: String,
        parent_abs: Point,
        table: &std::collections::HashMap<&str, &FrozenEntry>,
    ) -> usize {
        if !Self::subtree_covered(arena, id, &path, table) {
            return 0;
        }
        Self::commit_sub(arena, id, &path, parent_abs, table)
    }

    /// Whether every node in the subtree has a snapshot entry (coverage
    /// proof). Structure comes from the LIVE tree; entries from the snapshot:
    /// any drift (added/removed/reordered child) fails the lookup and vetoes.
    fn subtree_covered(
        arena: &TreeArena,
        id: NodeId,
        path: &str,
        table: &std::collections::HashMap<&str, &FrozenEntry>,
    ) -> bool {
        if !table.contains_key(path) {
            return false;
        }
        for (i, child) in arena.each_child(id).enumerate() {
            let child_path = format!("{path}/{}", segment(arena, child, i));
            if !Self::subtree_covered(arena, child, &child_path, table) {
                return false;
            }
        }
        true
    }

    /// Commits a fully-covered subtree. Coverage was proven by
    /// [`Self::subtree_covered`], so every lookup below hits.
    fn commit_sub(
        arena: &mut TreeArena,
        id: NodeId,
        path: &str,
        parent_abs: Point,
        table: &std::collections::HashMap<&str, &FrozenEntry>,
    ) -> usize {
        let entry = table[path];
        let abs = ResolvedRect::new(
            parent_abs.x + entry.resolved.x,
            parent_abs.y + entry.resolved.y,
            entry.resolved.width,
            entry.resolved.height,
        );
        // Collect children first (index-stable): the commit borrows mutably.
        let kids: Vec<(usize, NodeId)> = arena.each_child(id).enumerate().collect();
        {
            let node = arena.get_mut(id);
            node.resolved_rect = entry.resolved;
            node.absolute_rect = abs;
            node.state.core.cache.store_layout(entry.constraints, entry.resolved);
            node.state.core
                .dirty
                .remove(DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::SUBTREE_DIRTY);
        }
        let mut count = 1usize;
        let origin = Point::new(abs.x, abs.y);
        for (i, child) in kids {
            let seg = match &arena.get(child).key {
                Some(k) => k.0.clone(),
                None => format!("#{i}"),
            };
            let child_path = format!("{path}/{seg}");
            count += Self::commit_sub(arena, child, &child_path, origin, table);
        }
        count
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{Constraints, Key};
    use crate::tree::{LayoutNode, NodeKind};

    fn keyed(kind: NodeKind, key: &str, rect: ResolvedRect) -> LayoutNode {
        let mut n = LayoutNode::new(kind);
        n.key = Some(Key::new(key));
        n.resolved_rect = rect;
        n.absolute_rect = rect;
        n.state.core.subtree_static = Some(true);
        n.state.core.cache.last_constraints = Some(Constraints::tight(800.0, 600.0));
        n
    }

    fn build_tree() -> (TreeArena, NodeId, NodeId) {
        let mut arena = TreeArena::new();
        let root = arena.insert(keyed(
            NodeKind::Group,
            "app",
            ResolvedRect::new(0.0, 0.0, 800.0, 600.0),
        ));
        let panel = arena.insert(keyed(
            NodeKind::Group,
            "panel",
            ResolvedRect::new(10.0, 10.0, 200.0, 100.0),
        ));
        arena.set_children(root, vec![panel]);
        // set_children clears the static memo up the chain; re-prove it the
        // way the damage walk would (bottom-up).
        arena.get_mut(panel).state.core.subtree_static = Some(true);
        arena.get_mut(root).state.core.subtree_static = Some(true);
        (arena, root, panel)
    }

    #[test]
    fn round_trip_replays_boxes() {
        let (arena, root, _panel) = build_tree();
        let snap = FrozenLayout::capture(&arena, root, 800, 600);
        assert_eq!(snap.len(), 2);
        let bytes = snap.encode();
        let decoded = FrozenLayout::decode(&bytes).expect("decodes");
        assert_eq!(decoded.len(), 2);

        // Fresh tree, same structure, dirty flags as reconcile leaves them.
        let (mut fresh, fresh_root, fresh_panel) = build_tree();
        fresh
            .get_mut(fresh_root)
            .state
            .core
            .dirty
            .insert(DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::SUBTREE_DIRTY);
        // Wipe the proofs: a fresh boot has no memos yet.
        fresh.get_mut(fresh_root).state.core.subtree_static = None;
        fresh.get_mut(fresh_panel).state.core.subtree_static = None;
        // ...and no retained geometry.
        fresh.get_mut(fresh_panel).resolved_rect = ResolvedRect::ZERO;
        fresh.get_mut(fresh_panel).absolute_rect = ResolvedRect::ZERO;

        let n = decoded.apply(&mut fresh, fresh_root, 800, 600);
        assert_eq!(n, 2);
        let node = fresh.get(fresh_panel);
        assert_eq!(
            node.resolved_rect,
            ResolvedRect::new(10.0, 10.0, 200.0, 100.0)
        );
        assert_eq!(
            node.absolute_rect,
            ResolvedRect::new(10.0, 10.0, 200.0, 100.0)
        );
        // Stale layout flags cleared so the pass takes the cache-hit path...
        assert!(!node.state.core.dirty.contains(
            DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::SUBTREE_DIRTY
        ));
        // ...whose cache was primed with the recorded constraints.
        assert!(node
            .state
            .core
            .cache
            .get_layout(&Constraints::tight(800.0, 600.0))
            .is_some());
    }

    #[test]
    fn viewport_mismatch_falls_back() {
        let (arena, root, _) = build_tree();
        let snap = FrozenLayout::capture(&arena, root, 800, 600);
        let (mut fresh, fresh_root, _) = build_tree();
        assert_eq!(snap.apply(&mut fresh, fresh_root, 1024, 768), 0);
    }

    #[test]
    fn structural_drift_vetoes() {
        let (arena, root, _) = build_tree();
        let snap = FrozenLayout::capture(&arena, root, 800, 600);
        // Fresh tree with an extra child: paths diverge.
        let (mut fresh, fresh_root, _) = build_tree();
        let extra = fresh.insert(keyed(
            NodeKind::Group,
            "extra",
            ResolvedRect::ZERO,
        ));
        let mut kids: Vec<NodeId> = fresh.each_child(fresh_root).collect();
        kids.push(extra);
        fresh.set_children(fresh_root, kids);
        // Root still replays only if fully covered — the extra child has no
        // entry, so the whole subtree falls back.
        assert_eq!(snap.apply(&mut fresh, fresh_root, 800, 600), 0);
    }

    #[test]
    fn corrupt_bytes_rejected() {
        let (arena, root, _) = build_tree();
        let mut bytes = FrozenLayout::capture(&arena, root, 800, 600).encode();
        assert!(FrozenLayout::decode(&[]).is_none());
        assert!(FrozenLayout::decode(&bytes[..10]).is_none());
        bytes[0] = 0xFF;
        assert!(FrozenLayout::decode(&bytes).is_none());
        let mut long = bytes.clone();
        long.extend_from_slice(&[0u8; 8]);
        assert!(FrozenLayout::decode(&long).is_none());
    }
}
