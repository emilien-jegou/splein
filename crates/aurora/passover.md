# Aurora Motion API — Passover

Handoff document for whoever picks this up. Written at session end, 2026-10-03.

```
Suite:   cargo test -p aurora --offline --no-fail-fast   →   149 passed / 13 failed
         The 13 are a pre-existing baseline. Never let them block you.
         Never let their count grow.
Tree:    ~131 files dirty/staged. Nothing committed — by instruction, not by oversight.
```

---

## 1. What exists

The Motion API, per the original spec, in `crates/aurora`:

| Spec section | Status | Where |
|---|---|---|
| §1 Identity / layout-vs-compositor split | done | `Key` registry, `MotionState` as a separate slot from declarative style |
| §2 Declarative transitions | done | `.animate(tween)` / `.spring(spring)` on builders |
| §3 Imperative multi-property tweens | done | `engine.animate(key).x(..).duration(..).ease(..).play()` |
| §4 Timelines / choreography | done | `engine.timeline().to(..).at("<").at("+50ms").play()` |
| §5 Signal-driven motion | done | `engine.spring_signal(spring, from, to)` |
| §6 Gestures / direct manipulation | partial | `engine.hit_test(x,y) -> Option<Key>` exists; no gesture recognizer |
| §7 FLIP + presence | done | `layout_transition`, `layout_presence` |
| §8 Detach / ballistic physics | not started | — |

Supporting machinery: `FrameReport`, `HeadlessRaster`, `Engine::canvas()`,
`Ease`/`Tween`/`Spring`/`SpringState`, `Controller`/`Playback`/`Target`/`AnimationBuilder`.

**10 motion demos**, all ≤150 lines, zero warnings, all passing `tests/motion_demo_layout.rs`:
`accordion`, `imperative`, `interruption`, `layout_split`, `playback`, `press`,
`reorder`, `signals`, `transitions`, `validate`.

---

## 2. What was done (with evidence)

### 2.1 Accordion click didn't invalidate the scene — **fixed**

The crucial engine bug. Four-link chain:

1. Height binding fires → `propagate_upward` marks only the item's immediate parent, then
   **breaks** (parent is `Size::Fill`, not `Fit`).
2. `collect_layout_boundaries` independently resolves the boundary → walks to the **root**
   (no fixed-size ancestor exists).
3. `layout_node_with_text(root)` tested only `LAYOUT | MEASURE`. The root had neither — only
   `SUBTREE_DIRTY`, from the upward mark-walk. → **cache hit → early return → descendants
   never visited.**
4. Item rects unchanged → FLIP skipped → damage `old == new` → **zero pixels damaged.**
   Resize "fixed" it only because `check_constraint_change` dirties the root for real.

**Fix:** `src/layout/engine.rs::layout_node_with_text` — `needs_full_pass` now also includes
`DirtyFlags::SUBTREE_DIRTY`. A node with a dirty descendant must not take the cache path: the
cache describes only that node's *own* size.

Evidence: `changed pixels 0 → 15152`, `recomputed=[] → 19 rects`.
Negative control: test fails without it.

### 2.2 Chevron spun about its origin — **fixed**

`Transform` had no transform origin; rotation/scale pivoted at node-local `(0,0)`.

**Fix:** `src/tree/node.rs::effective_transform()` pivots the *linear* part about
`resolved_rect`'s centre, leaving translation untouched:

```
tx' = tx + cx − (a·cx + c·cy)
ty' = ty + cy − (b·cx + d·cy)
```

Fast path when the linear part is `IDENTITY`, so pure translations stay bit-identical
(no float drift). All four readers (`emit.rs`, `pipeline/bounds.rs`, `pipeline/damage.rs`,
`tree/arena.rs`) route through this one function, so this was the single place it could go.

Also fixed every `.scale()` demo (press/transitions/imperative/reorder), which drifted
down-right. `motion/flip.rs::inverse_flip`'s half-size-delta compensation was verified
algebraically against this pivot.

### 2.3 Accordion content crushed — **fixed**

Not an engine bug — flex correctly shrinks overflow to min-content. The window was too small.
Measured, not guessed:

| HEIGHT | item heights |
|---|---|
| 640 | `136.5, 26.4, 33.8, 39.3` ← what you saw |
| **760** | `176, 56, 56, 56` ← exact declared |

`examples/motion_accordion/main.rs` `HEIGHT` 640 → 760. Items also keyed `panel-{i}`
(the existing `item-{i}` key is on the *header*, a fixed-56px node).

### 2.4 Drag-reorder tearing / disappearing rows — **fixed**

One root cause, shared with 2.1's "ghosting" class:

`last_painted_bounds` — the record of *where did I last paint this node* — was stored
**clipped** to ancestor clips. A node that moves outside its clip gets a **zero** rect from
`compute_visual_bounds` (disjoint intersection), and `DamageRegion::push` **discards empty
rects**:

```rust
pub fn push(&mut self, mut rect: ResolvedRect) {
    if rect.is_empty() { return; }   // ← (0,0,0,0) silently dropped
```

So the "where I last painted" record was destroyed, and the vacated pixels were never damaged.

**Fix (2 files):**

- `src/pipeline/bounds.rs` — store **unclipped** bounds in `last_painted_bounds`.
- `src/pipeline/damage.rs` — compare/push an unclipped `full_bounds` (both sides on the same
  basis, so it can never collapse), while keeping `subtree_bounds` **clipped** — that one
  bounds a chunk's raster work, where clipping stays correct and useful.

Evidence (instrumented, then removed):

```
FAST2 dirty=[22]  node=22  old=(0,0,0,0)  visual=(79.24,635.28,601.52,49.10)  -> old!=new
```

Stale pixels jumped 3 → 1862 at **exactly f=39 — the frame the row left the arena clip.**

Result: **25,632 → 64** stale pixels, and no longer grows with drag distance.

### 2.5 New public API

`Engine::rect(key) -> Option<ResolvedRect>` — there was no way to read a keyed view's
laid-out rect. Needed by the regression tests, and generally for gesture/debug work.

### 2.6 Tests added (7)

| File | Tests | Sensitivity |
|---|---|---|
| `tests/layout_invalidation_regression.rs` | 2 | both **verified to fail** without their fixes |
| `tests/transform_center_pivot.rs` | 3 | 2 verified to fail; 3rd is a no-over-compensation guard |
| `tests/damage_parity.rs` | 2 | drag test **verified to fail** without the fix; accordion test is a *contract* only (passes either way — see §4.2) |

Every fix in this document was negative-controlled: revert → observe failure → restore.

---

## 3. Where things stand right now

- **149 passed / 13 failed.** The 13 are byte-identical to the Phase-0 baseline:
  `test_2_1`, `test_2_5`, `test_2_6`, `test_2_7`, `test_2_9`, `test_2_11`,
  `test_4_6`, `test_5_2`, `test_e2e_counter_tick_inside_boundary_isolates_layout_and_damage`,
  `test_screen_overlay_modal_unmount_restores_underlying_content`,
  `test_shadow_blur_fringe_damage_expansion_and_cleanup`,
  `test_window_resize_forces_full_window_damage`,
  `test_window_resize_preserves_canvas_and_damages_exposed_strips`.
- Examples + tests build with **zero warnings** from this work.
  (Only warning in the tree is the pre-existing `unused dependency tracing-subscriber`.)
- Hover-to-pause in `motion_playback` **is implemented** (`controller.pause()` on hover,
  status `"paused · hover"`).
- Scratch diagnostics (`zz_*.rs`, `.shots/`, `shots/`) have been deleted.

---

## 4. What needs to be done

### 4.1 Rasterizer: sub-pixel damage-tile seam (~1218 px) — **open, localized, not root-caused**

The same scene, rendered into **two fresh renderers** — one with a damage tile, one
full-screen — produces different pixels. The card's top stroke lands **one row lower** in the
tile.

Bisected precisely (accordion, card top ≈ 284.00015, probe at x=400):

| tile origin y | rows 284/285/286 | |
|---|---|---|
| 0, 283, 283.5, 283.99994 | stroke / white / white | ✅ |
| **284, 284.00006, 284.5** | white / stroke / white | ❌ off by one |

**Trigger:** a shape's top edge lands within ~0.001 px of the tile's integer top.

I read every file in the render path — `renderer.rs`, `command.rs`, `clip.rs`, `stroke.rs`,
`path.rs`, `stroke_geometry.rs` — and found **no rounding anywhere**. The geometry
(`adjust_stroke_geometry`, `build_rounded_path`) is pure float. So the mechanism is inside
tiny-skia's `fill_path` / `stroke_path` interaction with the clip mask at a degenerate
sub-pixel edge.

**I deliberately did not "fix" this** by nudging tile origins: getting that wrong causes real
under-damage, which is worse than a 1 px seam. Next step is to build a minimal repro outside
Aurora (just tiny-skia: a rounded rect + inside stroke, drawn at y=284.00015 into two tiles)
to confirm whether it's upstream.

### 4.2 Accordion ghost text was never reproduced — **open**

`docs/splein.png` (your screenshot) shows repeated grey copy below the list. Headlessly,
cycling `open` through all four panels and comparing every frame gives
**`outside ≤ 1`** throughout — damage coverage there is correct.

So either it needs a specific pointer/resize sequence I haven't replayed, or it's §4.1.
`tests/damage_parity.rs::switching_accordion_panels_repaints_every_pixel_they_vacate`
currently passes **with and without** the §2.4 fix — it is a contract assertion, not coverage.
Don't mistake it for proof.

**Ask:** what exact clicks / window resizes produced `docs/splein.png`?

### 4.3 Reorder demo list overflows — **open, needs a product decision**

`5 × 56 + 4 × 10 = 320 px` wanted vs **292 px available** → rows shrink to ~47.7 px instead
of 56. Same class as the accordion `HEIGHT` fix. I did not change it because the right
direction (bigger window vs. smaller rows/gaps) is your call.

### 4.4 `motion_signals` speed — **unverified**

You reported it running ~200 fps. The code now steps on real `dt`
(`self.state.advance(dt)` plus a `SWING_SECS` gate). I never measured it. Worth checking.

### 4.5 The 13 pre-existing failures

Known, deliberately untouched. One of them (`Shadow::outer` doesn't rasterize outside the
element) is a genuine product bug, not a test artifact.

### 4.6 Untouched spec sections

§6 has `hit_test` but no gesture recognizer (drag/swipe/long-press). §8 (detach, ballistic
physics, reattach) is entirely unimplemented.

---

## 5. Debugging: the visibility gap, and how to fix it

### 5.1 The problem

The sandbox has **no compositor** (`NoCompositor` / `NotSupported`), so `cargo run --example
X` never shows a window. I cannot look at anything I build.

The consequence isn't "I can't see" — it's that **I must hypothesize a bug before I can
detect it.** I can only find what I already suspect. Concrete costs this effort:

- I hand-calculated the accordion's overflow and **got the numbers wrong**; only a throwaway
  measurement harness produced the real `136.5 / 26.4 / 33.8 / 39.3`.
- I burned many turns doing arithmetic against damage rect coordinates to explain the reorder
  smear, and only found the answer in one instrumented run of `damage.rs`.
- The reorder tear would have been obvious in a single screenshot. Instead I first built a
  whole retained-vs-full-repaint oracle *just to learn that something was wrong*.

I did build that oracle — and it found real bugs — but only after I guessed what to look for.

### 5.2 The fix, ranked

**P0 — Headless screenshot dump.** This is the unlock.

`Pixmap::save_png()` already works and `Engine::canvas()` returns `&Pixmap`. Add either
`ExampleCli --screenshot <path>` or `Engine::save_frame(path)`, then I can render a frame and
**actually view the PNG** — verified working this session (dumped and viewed 5 frames).

**P1 — A scripted scenario driver.** Screenshots are useless without reaching the state.
The demos are driven by `Interactive::new(closure)`. Add a headless replay mode: a list of
frames `{ move(x,y), click(x,y), resize(w,h), advance(dt) }` so I can reproduce a user's exact
sequence instead of guessing one. This is the missing half of P0, and what §4.2 needs.

**P2 — Ship the render-diff oracle as library code.** I hand-rolled `stale_pixels()` (retained
canvas vs. from-scratch repaint of the same scene, split by damage coverage). Promote it to
something like `aurora::debug::render_diff(engine) -> RenderDiff { covered, uncovered, bbox }`.
This is the single diagnostic that found §2.4.

**P3 — Revive the golden harness.** `tests_bak/common/image_compare.rs` already implements
`assert_or_save_diff`, diff-mask generation, and tolerance — a complete visual regression
harness that is sitting unused. Wire it up with baselines under `docs/baseline/`.
Prerequisite: font rendering must be deterministic across machines.

**P4 — Opt-in structured diagnostics.** I patched `damage.rs` with `eprintln!` gated on an
`AURORA_DMG` env var, then removed it. Better: `AURORA_DEBUG=damage,layout,compile` mapped to
`tracing` targets, so visibility never requires editing source.

**P5 — Contact sheets.** Dump N frames and montage them into one grid, so I can see an
animation's timeline in a single read instead of N.

### 5.3 Two principles

1. **Images are a hint. Numbers are the proof.**
   I read two screenshots and reported them as identical. The numeric diff says
   **42,982 pixels differ (8.6%)** — concentrated in exactly row-0's old slot and where it
   reappeared. I was wrong, and only the cheap assertion caught it. Never ship an image
   conclusion without a numeric assertion under it.

2. **Crop to the region of interest.** A full-window dump costs a lot of context and makes
   small defects *harder* to see. A cropped, possibly downscaled region is cheaper **and**
   more legible.

### 5.4 Operational constraints (all observed this session)

| Constraint | Detail |
|---|---|
| Screenshot path | Must be in a **pre-existing** directory. `docs/` worked; newly created `crates/aurora/shots/` was rejected with *"not in the project"*. |
| Filenames | Use **fresh unique names**. I got a stale/wrong image served for a path I had read earlier in the session. |
| Test CWD | Cargo sets it to the **package root** — write `.shots/x.png`, not `crates/aurora/.shots/x.png`. |
| Git | Root-level `.md` is **gitignored** by the deny-all `*` rule. `crates/aurora/*md` and `docs/**` are allowlisted — which is why this file lives at `crates/aurora/passover.md`. |

---

## 6. Tooling gotchas

Verified the hard way:

- `terminal`: only `command` + `cd`. `timeout_ms` / `tail_lines` → JSON serialize error.
- `grep`: only `regex`. `include_pattern` glob → JSON error.
- `read_file`: whole file only. `start_line`/`end_line` → JSON error (use `sed -n 'a,bp'`).
- `create_directory`: `reason` param → JSON error (use `mkdir -p`).
- **`write_file` does not create parent directories** and can report success without writing —
  always `ls` to verify.
- Parallel tool calls in one block can get responses crossed — do writes sequentially.
- Never commit or branch unless explicitly asked.

### Code conventions (enforced, not optional)

- One-line responsibility comment at the top of every file — **read it before editing.** If your
  change falls outside it, split the domain rather than widening the file.
- ≤ ~150 lines per file. Split rather than work around.
- Doc comment on every public item: one line, under 100 characters.
- `#[instrument(skip_all, fields(..))]` only on external/heavy calls — **never** on hot paths.
  Nothing under `src/motion/` is instrumented.
- Prefer declarative, immutable composition. No 5+ argument functions. `bon` is not a dependency.

### Architecture facts worth re-deriving if you forget them

- Layout **never** reads `node.transform` — transforms are compositor-only.
- `commit_frame` clears `dirty_nodes_this_frame` on entry, so pre-frame writes must go through
  `FrameScheduler::pending_updates`.
- Headless tests **must call `engine.canvas()` every frame**, or only the last frame's damage
  gets rasterized (phantom pixels).
- FLIP skips nodes whose `state.motion != MotionState::NEUTRAL` (so drag-reorder isn't fought).
- `DamageRegion::MAX_RECTS = 4` with greedy clustering — merging only ever *adds* area, so it
  is conservative, never lossy.
- FLIP cannot invert a degenerate rect (collapse to 0×0 → singular scale). Expansion animates;
  collapse is instant. Inherent, not a bug.
