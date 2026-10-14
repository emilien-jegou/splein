# Retention in Aurora

How frame N+1 reuses frame N's work, and the invariants that keep reuse honest.

The short version: **a stacking context recompiles only when something about its
content changed.** Movement, damage, and repaints must not re-encode byte-identical
command streams. When they do, it is a bug — this guide names the exact conditions
so the bug has nowhere to hide.

That rule is what makes the awkward cases affordable. Most frames in a live UI
change almost nothing: a dragged row moves, a window exposes a strip, a hover
repaints. Without retention each of those re-emits the whole scene; with it, the
engine replays cached chunks and pays only for what is actually new.

> **Status.** Translated-bounds replay and the damage/content split are
> connected: chunks replay at any origin with live bounds, and damage touch
> no longer forces re-encoding. `tests/docs_retention_api.rs` pins all three
> invariants green.

---

## Contents

1. [The model](#the-model)
2. [The replay predicate](#1-the-replay-predicate)
3. [The flag lifecycle](#2-the-flag-lifecycle)
4. [Placement, origin, and clips](#3-placement-origin-and-clips)
5. [Damage is not content](#4-damage-is-not-content)
6. [Cache keys and their stability contracts](#5-cache-keys-and-their-stability-contracts)
7. [Reading a frame's reuse](#6-reading-a-frames-reuse)
8. [Rules worth internalising](#rules-worth-internalising)

---

## The model

Three pieces, and it is worth being precise about which is which:

| | |
|---|---|
| **Chunks** | A `SceneChunk` is one stacking context's display list: origin-relative commands plus the `abs_origin` and `placement` that locate it. Content is shared with `Rc`, so replaying is a refcount bump, not a copy. |
| **Dirty flags** | `DirtyFlags` on each node say what changed: `MEASURE`, `LAYOUT`, `PAINT`, plus `SUBTREE_DIRTY`, which means "something below me changed". |
| **Damage** | A `DamageRegion` says what pixels need repainting. It says nothing about whether commands are still valid. |

Content validity and pixel damage are different questions, answered by different
machinery. Every retention bug in this engine's history has been those two
getting conflated. The rest of this guide is the separation, made explicit.

---

## 1. The replay predicate

A stacking context replays its retained chunk — `fresh = false`, no re-emit —
if and only if **all** of these hold (`pipeline/compile/context.rs`):

1. The context root has no `SUBTREE_DIRTY` bit.
2. A retained chunk exists from a previous frame.
3. Its `inherited_clips` equal the current context's clips (a moved or resized
   ancestor invalidates, even when this subtree is clean).
4. Either damage is empty, or the chunk's `subtree_bounds` touch no damage rect.
5. The translated bounds overlap the viewport (culling, not a validity check).

Origin equality is deliberately **absent**: content is origin-relative by
construction, so a chunk replays at any origin. The replay translates the
retained bounds by the origin delta and culls on the translated box —
comparing the *stale* retained bounds would cull moved chunks wrongly, which
is why a naive equality check existed. The retained entry itself keeps its
pristine origin and bounds, so repeated replays never accumulate drift.

If any condition fails, the context recompiles, stores the fresh chunk as its
new retained entry, and reports `fresh = true`.

## 2. The flag lifecycle

Every set funnels through one function; every clear happens in exactly two
places. If you add a third, update this table and the test in
`tests/docs_retention_api.rs`.

| Event | Effect | Location |
|---|---|---|
| Any `mark_dirty(node, flags)` | node gets `flags \| SUBTREE_DIRTY`; every ancestor gets `SUBTREE_DIRTY` (stops at the first ancestor that already has it) | `tree/invalidation.rs` |
| Layout runs on a node | clears `MEASURE` and `LAYOUT` | `layout/engine.rs` (epilogue) |
| A node is (re)compiled | clears `PAINT` and `SUBTREE_DIRTY` | `pipeline/compile/hierarchy.rs` |

Lifecycle for one change: `SET → layout clears MEASURE/LAYOUT → compile clears
PAINT/SUBTREE_DIRTY → replay allowed`. Note what this implies: a node whose
subtree was *replayed* keeps whatever bits it has (replay required them absent,
so there is nothing to clear), and an ancestor's `SUBTREE_DIRTY` clears only
when the ancestor itself recompiles — see section 4 for why that matters.

`PAINT`, `LAYOUT`, and `MEASURE` are never consulted by the replay predicate
directly; they reach it only aggregated through `SUBTREE_DIRTY`.

## 3. Placement, origin, and clips

Commands are origin-relative: the stream opens at the chunk's own origin, so
replaying it under a new `abs_origin` paints identical content in a new place
(`SceneChunk::rebase_to_local`). The stacking root's own transform is hoisted
out of the stream into `placement`, so transform-only motion changes placement
without touching content bytes.

Consequences, each an invariant:

- Moving a chunk (scroll, drag, offset animation) must preserve its command
  bytes exactly. Only `abs_origin`/`placement` may differ.
- `placement` is never part of the replay key. Content does not depend on it.
- `inherited_clips` are absolute and placement-time state. A clip change
  *does* invalidate, because the consumer applies clips around the stream.

## 4. Damage is not content

A chunk whose bounds intersect damage is re-*rasterized*, not re-*encoded*.
Damage overlap alone must never flip `fresh` to `true`: the replay path
re-emits the shared `Rc` with the current origin, and the damage region drives
the rasterizer. Re-encoding byte-identical commands because pixels got dirty
is the conflation this guide exists to prevent.

## 5. Cache keys and their stability contracts

| Cache | Key | Must invalidate on | Must NOT invalidate on |
|---|---|---|---|
| Chunk replay | clean + clips + origin(ε) + viewport + damage-touch | content/paint/layout change in subtree, clip change, real move | damage touch alone, sub-epsilon origin drift |
| Shadow patch (both backends) | radius + blur + spread (+ color on GPU) | style change | rect move/resize, offset change |
| Shaped text | style + content + wrap width quantized to 1/16px | content/style/real wrap change | fractional width jitter, ±0, non-finite widths |

Keys use exact bits only where the domain is discrete. Anything that animates
(floats especially) is quantized or epsilon-compared — an animated value that
lands in the key verbatim thrashes the cache once per frame.

## 6. Reading a frame's reuse

`FrameDiagnostics.compile` tells you what happened:
`chunks_emitted` (scene size), `cached_chunks` (replays), `commands_emitted`
(freshly encoded commands). Healthy frames show `commands_emitted == 0` when
idle and `cached_chunks` tracking scene size under motion. Per-chunk `fresh`
in `engine.scene().chunks` names the culprits when they don't.

## Known gap: culled dirty descendants

One case breaks the contract above, and it is recorded here rather than
hidden: when compile culls an offscreen *non-context* node mid-emit
(`compile_node_hierarchy` returns early on the viewport check), that node's
bits stay set — honest, its content was never emitted — but the enclosing
stacking root still reaches the flag-clear at the end of its own emit. The
ancestor is now clean with a retained chunk that is missing the dirty
descendant's commands, and replay serves that hole on every later frame,
including after the descendant scrolls onscreen. Nothing re-emits it: the
ancestor replays (clean), the descendant is not a context (never visited
alone).

Concretely: edit text in an offscreen row, scroll it into view, and the row
paints without its text until something re-dirties an ancestor context.
`damage_parity` does not catch it — the missing content sits inside repainted
tiles (covered, not uncovered). `tests/docs_retention_api.rs::
dirty_offscreen_row_reveals_cleanly` pins the desired behavior and stays
ignored until the fix lands. The fix direction is to make culled-dirty
descendants keep their enclosing context dirty (skip that context's clear
when the walk culled anything dirty), so reveal recompiles once, correctly.

## Rules worth internalising
- **Damage repaints; it never re-encodes.** If you touch the replay predicate,
  re-read section 4 first.
- **One setter, two clearers.** New dirty bits go through `mark_node_dirty`;
  new clears go in layout epilogue or compile emit — nowhere else.
- **Floats don't belong in keys raw.** Quantize, epsilon, or collapse; an
  animated exact-bit key is a per-frame full rebuild wearing a cache costume.
- **Transform motion is free by construction.** If moving something recompiles
  it, the placement/content split is broken — fix the split, not the caller.
