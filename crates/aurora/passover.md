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

- **330 passed / 13 failed** (`cargo test --workspace --no-fail-fast`). The 13 are
  byte-identical to the Phase-0 baseline, verified by name rather than by count:
  `test_2_1`, `test_2_5`, `test_2_6`, `test_2_7`, `test_2_9`, `test_2_11`,
  `test_4_6`, `test_5_2`, `test_e2e_counter_tick_inside_boundary_isolates_layout_and_damage`,
  `test_screen_overlay_modal_unmount_restores_underlying_content`,
  `test_shadow_blur_fringe_damage_expansion_and_cleanup`,
  `test_window_resize_forces_full_window_damage`,
  `test_window_resize_preserves_canvas_and_damages_exposed_strips`.
  Use `--no-fail-fast`: without it cargo stops at the first failing target and the counts are
  meaningless.
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

---

## 5. Microkernel reactive API — implementation notes

The reactive microkernel now exists end to end. `#[component]`, `signal`, and
`Transition::on(signal)` are real, and all ten motion demos are written against them.

### 5.1 What each stage added

**Interaction DSL** (`src/input/`). Pointer events are a backend-neutral vocabulary
(`PointerEvent`, `PointerPhase`), and `hit_chain` resolves a point to a *chain* of nodes in
reverse painter order. Painting and hit testing share `LayoutNode::paint_layer`, so the two
cannot disagree about which sibling is on top. The chain honours ancestor clips, so a view
clipped out of its parent stops being clickable. `on_click`, `on_pointer_down/up/move`,
`on_pointer_enter/leave` and `on_drag_start/step/end` are stored per node behind one `Rc`
(`NodeHandlers`) and replaced wholesale on reconcile.

**Gesture velocity hand-off** (`Signal::set_with_velocity`). A drag release writes the value
*and* the velocity it arrived with. The reactive runtime stores the hand-off as an opaque
number it never interprets; `Transition::on` consumes it once, and `ScalarTrack::adopt_velocity`
feeds it to the spring. This is the difference between a flick that carries and a card that
stops dead.

**`#[component]`** (`crates/aurora-macros`). An attribute macro that runs the body inside the
ambient hook scope and converts the result to an `Element`. `App::run` installs that scope
before calling the root builder. The body runs **once per mount** — the engine never re-runs
a component to animate it, because a signal captured in a property binding is re-read every
frame. Outside a scope a component panics rather than quietly allocating onto a throwaway
graph.

**Reactive child lists** (`.children_of` / `.children_labels`). A list whose order is a
signal re-reconciles its children when the *key sequence* changes, so a two-row swap costs
one diff and an unchanged list costs one comparison. This is what lets a reorder happen
without re-running the component, and it is why rows must be keyed by identity: a reorder
read as "every row changed" would diff every node.

### 5.2 Bugs this surfaced

Four pre-existing defects, all now fixed and covered:

- **Pose bindings were never subscribed.** `evaluate_prop` only opened an observer when the
  node declared a transform of its own, so `.x() .y() .scale() .rotate_degrees()` driven by a
  signal was read once at mount and never again. Any view animated purely through a pose
  component was silently frozen.
- **A keyed root was unreachable.** `Engine::with_runtime` never bound the root's key, while
  `set_root` did, so a keyed root failed every key-based lookup and `hit_test`.
- **Ancestor clips did not cross chunk boundaries.** A child with `opacity < 1` becomes its own
  stacking context and therefore its own `SceneChunk`; chunks replay independently, so a
  parent's clip was already popped. Chunks now carry `inherited_clips` and re-assert them.
- **A node's background ignored its render box.** `DrawRect` used the flow box while the clip
  used the render box, so a collapsing card kept its settled height while its own contents
  revealed past it.

### 5.3 Invariants held

- FLIP stays in the engine. `layout_motion` is the only thing that moves a row's slot, and
  userland never sees the before/after rects.
- A component never scales content. Interior stays at flow size; the box is a clip.
- Handlers never invalidate layout directly. A handler writes a signal, and ordinary reactive
  propagation decides what that dirties.
- The reactive layer stays physics-agnostic. It stores the hand-off as a number only the anim
  layer interprets.

### 5.4 Every demo migrated, and one demo bug it exposed

All ten demos now declare their own interaction: no driver extension, no pointer polling, and
no `pointer.over_key` anywhere under `examples/motion_*`. `examples/common/interactive.rs` is
deleted, having become dead. An idle frame in these demos now costs no interaction work at all.

Finishing the accordion turned up two latent defects:

- **The panel fade keyed off the wrong thing.** Each item's copy faded on
  `open.is_some()` rather than on whether *that* item was open. It happened to work only
  because exactly one panel is open at a time; two could never be.
- **`motion_reorder` did not fit its panel.** The list declared 5 rows of 56 plus 10px gaps,
  which needs 320px, inside 292px of available height. The shrink pass therefore shrank each
  row by a *different* amount (47.7 → 52.6), so the row pitch was not uniform — 57.7, 59.3,
  60.7, 61.7 — and dividing the drag by one stride could not work. `STRIDE` was hardcoded to
  66, which was wrong by ~14% against even the first gap.

The demo now declares `ROW` and `GAP` as its layout contract, derives `STRIDE` from them, and
sizes them to fit. `a_reorder_uses_the_real_row_pitch` measures the settled layout and fails
if the declared constants and the real pitch ever disagree; restoring the overflowing row
height makes it fail with `Baseline should sit at its declared height of 56, got 47.67`.

That is the FLIP argument made concrete in miniature: the row pitch is an output of
measurement and constraint solving, so userland cannot compute it without either reading the
engine back after the fact or reimplementing the shrink pass.

### 5.5 Suite

`cargo test -p aurora --no-fail-fast` → **228 passed / 13 failed**, the 13 byte-identical to
the Phase-0 baseline listed in §3. Examples and tests build with no warnings from this work.

---

## 6. Diagnostics: driving, freezing, and photographing the engine

`aurora::testing` exists so that investigating a visual bug does not start with writing a
throwaway probe file. Four pieces, each answering a question that was previously answered by
guessing.

### 6.1 Freeze and slow motion

`Engine::set_time_scale` scales the delta where it is produced, so it reaches every
time-driven system at once — compositor motion, reactive tickers, render-box springs,
gesture velocity. `Harness::pause()` sets it to zero, which freezes motion while still
running layout, damage, compile and raster. That is the distinction that matters: a frozen
engine still *renders*, so you can photograph a transition mid-flight rather than only before
or after it. `Harness::slow_motion(0.1)` stretches the same transition over ten times as
many frames.

### 6.2 Reading a frame without paying for it

`Engine::canvas` rasterizes on **every call**. A per-pixel scan written against it re-renders
the frame once per probe; one such scan hung a build for fifteen minutes before the cause was
found. `Harness::snapshot()` copies the canvas once, and `Snapshot` then answers
`at`/`runs_down`/`widest_run`/`count_matching`/`changed_pixels` against that copy.

`Snapshot::over(background)` flattens the frame onto the page colour. The CPU canvas leaves
anything unpainted transparent because clearing is the backend's job, so a raw headless
capture is a dark void rather than the running app. `Harness::view()` and
`Harness::screenshot()` composite first, and `Harness::with_background` mirrors the app
config that supplies the colour.

### 6.3 Motion traces

`Harness::trace(key, frames)` samples a view's drawn transform every frame. Asserting on one
frame can only say where something is; a trace says how it got there, which is the only way to
catch a pop, a stall, or a path that reverses. `worst_jump`, `path_length` and `range` are the
measurements worth making.

### 6.4 Seeing the whole transition at once

`Filmstrip` composites labelled frames into one PNG, with a 3x5 digit font baked in so a
strip can say which frame it is. This is the single most useful thing in the module: the
accordion switch is reviewed in one image read instead of three separate screenshots, and the
whole collapse-and-reveal is legible at a glance.

### 6.5 Invariant checks, and what they are worth

- `assert_disjoint(&[(name, rect)])` — named boxes must not overlap. Sharp and unambiguous:
  a stack of cards occupying the same pixels is wrong whatever the intent.
- `assert_no_viewport_overflow(frame, page_colour, margin)` — pixel-based. Unambiguous, but
  only valid for a page whose chrome respects the margin. The accordion's full-width hairline
  reaches x=0 by design, so this check reports it as an escape and is *not* used there.
- `overflow::report` — geometric observations, informational only. It reports content reaching
  outside a parent that does not clip, and skips clipping parents deliberately: a clipping
  parent containing overflowing content is the reveal mechanism, not a bug. An earlier version
  reported that as a failure and drowned real findings in noise.

A tool that cries wolf gets ignored, so the reporting checks are separated from the asserting
ones deliberately.

### 6.6 Two engine defects this surfaced

- **A settled transition reported itself as permanently active.** `TargetTicker::advance`
  returned `true` unconditionally, so `has_active_motion()` was true forever once any
  transition existed and the engine could never go idle. The ticker must never be retired —
  its target can change again — so the fix belongs in `is_active()`, which now reports whether
  the last frame still had somewhere to go.
- **A freshly mounted harness reported every rect as zero.** `settle()` returned immediately
  because nothing was moving, so layout never ran and every later assertion was quietly wrong.
  The constructors now render one frame, and `settle()` always advances at least once.

### 6.7 Coordinate spaces

`Engine::rect` is parent-local, `render_rect` is where a view is drawn, and `absolute_rect` is
where that lands on screen. Mixing them up is the most common way to aim a pointer at nothing,
and it cost three wrong tests during this work. `Harness::located(key)` returns all three
under names that say which space each is in, and `Located::center()` is always screen space.

### 6.8 Suite

`cargo test -p aurora --no-fail-fast` → **244 passed / 13 failed**, the 13 byte-identical to
the Phase-0 baseline. `tests/testing_harness.rs` covers the tooling itself (14 tests) and
`tests/visual_capture.rs` captures the accordion switch as a labelled filmstrip.

---

## 7. Migrating the two hardest tests onto the harness

`testing_harness.rs` exercising the harness proves only that the harness satisfies its own
tests. `rewritten_demos.rs` and `damage_parity.rs` are the tests that would actually notice a
regression, so rewriting them onto the new API is what shows whether it holds up. Both found
real problems, neither in the engine.

### 7.1 A colour assertion that was passing on nothing

`rewritten_demos` matched the accent with a hard-coded `0x1D4ED8` at tolerance 40/40/60. The
demos' shared palette accent is `motion_ui::ACCENT = 0x1019EC`, whose green channel is 53 away
— outside the tolerance. The green term meant the assertion could not match the colour it
named.

It passed anyway. Measured over a rendered `layout_split` frame:

| matcher | samples found |
| --- | --- |
| hard-coded `0x1D4ED8`, tol 40 | **34** |
| actual accent `0x1019EC`, tol 12 | **2032** |

The 34 were anti-aliased card edges that happened to land inside the tolerance box. The test
`count_accent(...) > 0` was asserting that the card was painted, and was really asserting
that 34 stray edge pixels exist — a check that would survive the card being deleted. In
`motion_signals` the same matcher found 6.

The lesson generalises past this file: a colour matcher written from a source constant rather
than a rendered frame is a guess, and `> 0` on a guessed matcher is not a test. Both demos now
assert on measured values with a tolerance tight enough to exclude edge blends, and the
cross-demo smoke test asserts only that *something* was drawn, relative to the shared page
colour, because the demos do not share an accent at all — `motion_imperative` paints `#10B981`
and a dark tinted surface.

`motion_signals` no longer asserts that a particular swatch colour is present at all. Which
colour is on screen depends on where the spring happens to be, so that assertion was both
humble-looking and arbitrary; it now asserts that a large part of the frame changed hue, which
is what "one spring drives the whole page" actually means.

### 7.2 `drag_by` released before you could look

Rewriting the reorder test failed immediately: `drag_holding` and reading the row's offset gave
0. Not an engine bug — `drag_by` lifts the button before returning, and by then the spring had
carried the row most of the way home. The original test read the offset *before* its `Up`, and
no harness method could express that.

This is a genuine gap rather than a test problem: a drag's most interesting state is the held
one, and it is the only one that cannot be reached through the existing API. Added
`drag_holding` / `release_at`, which also has to release *where the drag stopped* — the view
has moved, so lifting at its new centre would be somewhere the pointer never went.

### 7.3 Damage parity promoted out of a private helper

`damage_parity.rs` opened with a 35-line `stale_pixels` that rasterises the scene twice and
walks every pixel. It was the only implementation of the only reliable way to observe damage
tracking, and it lived in one test file. It is now `testing::damage::parity`, and the test is
its remaining two cases.

The part worth keeping is the *classification*: a mismatch inside a tile the frame redrew is a
painting bug; a mismatch outside every tile is a damage bug. One pixel count cannot tell you
which, and the wrong guess sends you to the wrong half of the pipeline. `redrawn_at` is
therefore public and unit-tested on its own, covering the floor/ceil tile semantics that make
fractional damage rects behave: a pixel straddling an edge *was* redrawn, and crediting or
blaming it wrongly turns every fractional rect into a false result.

An attempt to prove the check had teeth failed informatively. The intent was to confirm the
comparison detects differences at all, on the reasoning that a dragged row must produce some.
It returned `covered: 0, uncovered: 0` — because a working damage system means the retained
canvas *equals* a full repaint, so the correct outcome is no differences at all. The premise
was inverted; a vacuous result here is the good case. The classification is unit-tested
directly instead of being probed through the engine.

### 7.4 A performance guard that failed on scheduler noise

`animating_many_boxes_does_not_explode_damage_or_frame_cost` failed once during this work at
125.99ms against a 120ms budget, and reproduced roughly one run in three in isolation at
~128ms with no change to the code under test. It asserted on the **maximum** frame time over
90 frames.

The distribution explains it, and explains why max was the wrong statistic:

| | |
| --- | --- |
| median | ~20µs |
| p90 | ~75ms |
| max | 82–112ms |

Strongly bimodal: frames where the springs have settled bail out of every pass, and frames
that genuinely relayout cost ~75ms in a debug build. p90 sat stably at 75–79ms across runs
while max wandered. Since the property being guarded — runaway damage area or an accidental
O(n²) — lifts the *whole* distribution rather than one frame, p90 detects it just as
reliably. The assertion now uses p90 and reports median and max in the failure message so a
future failure says whether the shape moved or just the tail.

The median staying at 20µs is itself the load-bearing number: it is the idle-bailout property,
and it would be the first thing to disappear if every frame started doing full-window work.

---

## 8. Input was never connected, and rebuilds ate component state

Two reported bugs, both real, both invisible to the test suite for the same reason: the tests
drove the engine through APIs that the running app did not use.

### 8.1 No pointer event ever reached the engine

`App::run` builds the tree, installs the reactive runtime's dirty hook, and runs a winit event
loop that matches exactly three window events — `CloseRequested`, `Resized`, `RedrawRequested` —
and drops the rest. There was no `CursorMoved` case, no `MouseInput` case, and the only caller
of `queue_pointer_event` anywhere in the crate was the test harness.

So every interactive demo was dead in a real window: the accordion headers did nothing, the
press button did not squeeze, the reorder rows did not drag. Meanwhile the interaction core it
should have been driving was complete and thoroughly tested — painter-ordered hit chains,
ancestor clipping, capture, hover, drag threshold, double-click, release velocity — against
handlers fed by hand.

`app/runner/pointer.rs` adds the bridge. Three shape mismatches had to be reconciled:

- winit reports occurrences with no notion of a gesture; the engine wants press, release and
  drag phases.
- **`MouseInput` carries no position at all.** A press can only be aimed using the last
  `CursorMoved`, so the bridge tracks one. When it has none, the press is dropped rather than
  aimed at the origin, which would fire a handler at whatever lives in the corner.
- Drag classification is left to the engine, which owns the threshold. Re-deriving "is this a
  drag" in the bridge would duplicate that constant and let the two disagree.

Two decisions worth stating:

**Non-primary buttons are dropped.** The engine's handler vocabulary is primary-only — there is
no secondary click callback, and its tracker counts a click burst without distinguishing which
button produced it. Forwarding a right click would therefore fire every `on_click` in the tree,
so a context-menu gesture would activate the button underneath it. Keeping that policy in the
platform layer leaves the engine's semantics about gestures.

**Input stays in the coordinate space `Resized` reports.** Winit reports both in physical
pixels, so dividing pointer positions by the scale factor while the viewport stayed physical
would aim clicks at the top-left quadrant of a HiDPI display. Consistent-and-physical is wrong
sized but clickable; consistent-and-logical requires changing resize, present and the backend
surface together.

`cancel_pointer_gesture` also had to grow a real implementation. It called `clear_hover`, which
drops the hovered node and nothing else — the capture and the drag flag survive, so a pointer
leaving mid-drag left the pressed view pinned to a position the pointer had abandoned. The
engine-side test for this initially *passed for the wrong reason*: the bridge's own held-flag
had flipped, while the engine still thought a drag was live. It now asserts on
`pointer_is_dragging()` and `pointer_position()`.

### 8.2 A rebuilt component restarted from scratch

A reactive child list is re-evaluated by re-running its builder, so a row component is
constructed again on every list change. The engine matched the rebuilt row to the node it
already had by key, but the signals the row allocated during construction were gone: `use_signal`
allocated unconditionally, and there was no notion of a component *instance* to file them under.
Measured, with a row whose height is its own hook state:

| | row height |
| --- | --- |
| mounted | 40 |
| after two clicks | 80 |
| after the list appended a row | **40** |

Before that it did not even get that far. A `#[component]` calling `use_signal` inside a
reactive list **panicked** — `hook called outside a component body` — because the builder runs
during a frame, where no hook scope is installed. Two call sites build reactive children, and
both now enter a scope: `dsl/reconcile_group.rs` on the mount path and `runtime/scheduler.rs`
on the per-frame re-render path.

Preservation then needed an identity, which turns out to be the whole problem:

- **A component cannot know its own key.** The key is set on the element the body returns,
  after the hooks have run. Keying on it retroactively is not possible.
- **Index is not identity.** Preserving by position would hand each component the state of
  whichever item moved into its index. On a reorder that is worse than resetting: the state is
  wrong rather than absent.
- **Sibling components must not collide.** Two rows under one name would otherwise swap state.

So identity is explicit. `Cx::enter_keyed` names an instance, and `use_signal` files each hook
under `(instance, hook index)` on the runtime. Hook call order is fixed by the code, so the
index is a stable name for "the second signal this component allocates" across rebuilds. A
nested component derives its identity from the keyed row containing it (`row-a#0`), so it is
preserved too; an anonymous scope that cannot be reproduced gets a fresh identity and keeps
today's behaviour, because recalling from an unreproducible name could only hit by coincidence.

`children_labels_keyed(items, key_of, row)` wires this up for lists, where `key_of` names the
item rather than its position. `children_labels` and `children_of` cannot preserve hook state
and now say so; a test pins that an unkeyed row restarts, so the limitation is discovered by
reading a test rather than by losing a user's draft.

### 8.3 Tests

`tests/app_input_e2e.rs` drives the real demo pages through the real bridge, which is the
coverage that was missing and the reason both bugs survived: clicking an accordion header moves
the open panel, clicking the open header closes the list, press-and-hold squeezes and springs
back, and a pointer drag reorders the reorder list.

`tests/reactive_component_state.rs` pins the identity contract: a keyed row keeps its state
across a rebuild, each row keeps *its own* state after a reorder, an item that leaves and
returns finds its state again, a component nested in a keyed row is preserved, two lists holding
the same label do not collide, and an unkeyed list restarts.

### 8.4 Suite

`cargo test -p aurora --no-fail-fast` → **270 passed / 13 failed**, the 13 byte-identical to the
Phase-0 baseline.

---

## 9. An animation clock, and a way to watch it run slowly

### 9.1 What already existed, and what was missing

There was already an internal clock. `anim::Clock` produced one delta per frame and
`run_frame_passes` handed that single delta to compositor motion, the reactive tickers and the
render-box springs, so animation was at least internally consistent and `set_time_scale` already
reached every time-driven system at once.

What was missing was *quantisation*. Springs are integrated explicitly, so the result depends on
the step size and not only on elapsed time, and the step size was whatever the frame happened to
measure. `stable_substeps` made that safe — it splits a delta so a stiff spring stays inside the
integrator's stability bound — but safe is not the same as frame-rate independent. A transition
that landed in one place at 30fps landed somewhere else at 144fps, and the discrepancy is
invisible in a screenshot and obvious in a side-by-side.

So the clock now banks each frame's scaled delta and spends it in whole quanta of
[`FIXED_STEP_SECS`], a fixed 120Hz grid. Thirty frames of 33ms run four quanta each; 144 frames
of 7ms run one every few frames. Both cover the same elapsed time in the same increments, which
is the property that makes the trajectory a function of time rather than of frame count.

Two consequences worth stating plainly:

- **A frame spends nothing until it can afford a whole quantum.** `MAX_STEPS_PER_FRAME` only
  guards against an absurd value; the real bound on a live frame is the existing 100ms delta
  clamp, which is exactly twelve quanta. A *forced* delta is honoured in full, because a
  headless step that asked to advance 400ms means 400ms and quietly giving it 100ms makes every
  such test assert against a simulation it did not describe.
- **Render boxes take the summed total, not the grid.** They are stepped after layout, so they
  cannot join the pre-layout loop, and advancing them per quantum would report "nothing moved"
  on the frames that banked a partial quantum — which `has_active_motion` would read as the
  animation having finished, and the app would stop redrawing mid-flight.

### 9.2 Three bugs the grid exposed

All three were latent and none was visible before, because a variable step size happened to
average out.

**A tween that had visibly finished reported itself unfinished.** Twenty-four additions of
`1/120` land a few float ulps either side of the duration, and `is_complete` compared with `>=`
alone. A transition one ulp short of its end stays active forever: the animation stops and the
frame loop does not, because the engine is still looking for motion that will never arrive.
Completion now carries a tolerance proportional to the duration — far below one quantum, so it
can only absorb arithmetic noise.

**A slowed or frozen clock could never start an animation.** Nothing simulated time on a frame
too short to afford a quantum, so tickers were never polled — and adopting a new target is not a
function of elapsed time. At a tenth-speed clock, pressing a button produced no frames of
animation at all. A frame with nothing to spend now still polls the tickers with a zero delta,
so a target change is observed without any time passing.

**A stopped clock banked time it would later replay.** Freezing kept the partial quantum, so
resuming fast-forwarded every animation at once. A frozen clock now discards its bank.

### 9.3 Slow motion from the keyboard

`Clock::set_time_scale` already existed and the test harness already used it; what was missing
was any way to reach it from a running app. `VisualDebugger` now drives it, and since every
example registers the debugger, the control is live in all of them.

| key | effect |
| --- | --- |
| `F8` | freeze, and freeze again to resume |
| `[` | step slower through 0.05x, 0.1x, 0.25x, 0.5x, 1x |
| `]` | step faster, up to real time |
| `.` | advance one frame (120Hz quantum) while frozen |

Discrete presets rather than a free-running multiplier, because a shortcut that lands on a
memorable fraction is one you can reach without looking, and the ends are the two rates anyone
actually wants: a crawl to watch a spring's last frames, and real time. The HUD grows an amber
`0.10X` badge, or a red `HOLD` when stopped, and the badge is *absent* at real time so its
appearance is itself the message.

The rate lives on the debugger rather than being read back from the engine, so the shortcut that
starts a slow motion and the badge that reports it cannot disagree. `on_update` only writes when
the value actually differs, and asks for the frame that shows the change — a shortcut that
re-rates the clock but leaves the last frame on screen looks like it did nothing.

`apply_debug_key` is split out from `handle_debug_key` because a winit `KeyEvent` cannot be
constructed outside winit; folding the window-event shape into the mapping would have made every
binding untestable.

#### Stepping, which freezing alone does not give you

Freezing and doing nothing useful is the obvious failure of a slow-motion control, and it is what
the first version actually did: a stopped clock shows exactly one frame forever. Forcing a delta
does not help either, because a forced delta is scaled like any other and a scale of zero is
zero however hard it is asked.

`Clock::request_step` therefore queues a quantum that `animation_steps` honours *even while the
clock is stopped*, so `.` walks a frozen transition one 120Hz frame at a time — about 2.5px of a
400ms tween over 120px, which is the granularity you need to see what a spring is doing near
arrival. `Engine::step_animation` exposes it and `Harness::step_once` wraps it in a frame.

A request made while time is already running is dropped rather than banked. Saving it would mean
stepping once mid-animation and then being surprised by an extra frame at some later freeze,
which is the opposite of what the key looks like it does. Requests do not queue beyond the frame
boundary either, so holding the key down runs one quantum per frame rather than building a
backlog.

### 9.4 Tests

`anim::clock` covers the accounting: equal time costs equal quanta at 15fps and 240fps, a
remainder is banked rather than dropped, a frozen clock spends nothing, a tenth-speed clock
advances a tenth as far, and a forced delta is simulated in full.

`tests/animation_clock.rs` measures the property itself: a 400ms linear tween read at 200ms
returns 60px at 15fps, 60fps and 240fps, and completes at the same moment at 20fps and 240fps.
Also that slow motion is the same animation over more frames rather than a different one, and
that the debugger's shortcuts reach a real engine and genuinely stop it.

Two measurement traps are documented in that file because both produced convincing false
failures first. `Harness::click` spends two frames, so counting frames per click is not the same
as counting simulated time. And the frame on which a transition adopts its target is not a frame
it advances, which is why `Clock::simulated` exists and why the tests line two frame rates up on
the same simulated instant rather than the same frame count.

### 9.5 Suite

`cargo test -p aurora --no-fail-fast` → **299 passed / 13 failed**, the 13 byte-identical to the
Phase-0 baseline.

---

## 10. The accordion reveal: a clip working correctly, revealing nothing

### 10.1 `layout_motion` is not legacy

Worth recording because it was the first suspicion. `layout_motion` carries no deprecation and
is the current documented mechanism: layout jumps to the new box while the box you see springs
there, so content stays at its laid-out size and nothing inside is stretched. `layout_transition`
is an alias for the same spring. The demo comment says as much. The mechanism was not the
problem.

### 10.2 What the numbers said

Measured on a settled open card, per item:

| | body height | copy ink | slack at the bottom |
| --- | --- | --- | --- |
| item 0 | 120px | ~73px | 47px |
| items 1-3 | 120px | ~90-93px | 27-30px |

The body is a hard-coded 120px and the copy fills 37-40px of it, so **roughly two thirds of the
box's travel revealed nothing**. Everything reported as a bug follows from that one number:

- The copy fit inside the box long before the box finished, so the reveal clip never had to cut
  it. It was simply fully visible for the whole back half of the animation -- which is what
  "already fully expanded while the animation is ongoing" describes.
- The rounded bottom corners sat up to 83px below the last line of text, so during the entire
  growth they floated in empty space and read as radius that was "still round".
- The copy's fade is `Ease::OutCubic` over 180ms, front-loaded enough to look finished by half
  the box's travel, compounding the impression.

### 10.3 The fix

`panel_body` now centres its copy on the main axis (`Distribution::Center`), which for a column
is vertical. The body keeps its height -- which also keeps the shared dark arena around the list
full, since that arena is used by all eight demos and shrinking it belongs in `motion_ui`, not
here.

Centring is what makes the travel meaningful: the copy is uncovered from both edges at once, so
the reveal reads as the panel opening rather than as a box growing past some text. `OPEN` and
the copy's line height and bottom padding are now named constants rather than scattered
literals, so the relationship between the box and its content is stated in one place.

### 10.4 A real engine defect the re-read turned up

`clips_imposed_by` and `enter_node_frame` each pushed **two** clips for a node that both clipped
and animated: a rounded one from the declared radius, and a square one for the reveal. Both bound
the *same rect*, so the intersection was the rounded shape and the square reveal was inert. Every
animating clipped view was revealing along its corner curve, and the code comment claimed
otherwise.

Both now push a single clip: square while a render track exists, the declared radius once it
settles. `render_track` is cleared on settle, so the condition is exactly "currently animating".
`exit_node_frame` pops once instead of twice to match. The scene now shows `r0` on animating
items and `r12` on settled ones, where before both were `r12`.

### 10.5 Reproducing it: what the real frame shows

The `S` capture closes the gap that headless testing could not. Measuring the app's own frame
rather than eyeballing it gives, on the buggy frame, **two** white rects where there should be one:

| | x extent | y extent |
| --- | --- | --- |
| header rect | 85..714 | 284-339 (56 tall) |
| body rect | **104..695** | 339-426 |

The body rect is inset about 19px on *both* sides, has its own 12px corner radii, and is separated
from the header rect by a one-pixel dark seam. On the settled frame there is a single rect per
card at 85..714 and no seam. The inset matches the header's `Margin::x(20.0)`, so the second rect
is positioned like the header while occupying the body's vertical range -- which is why it reads
as "the heading's corners are still round" and "the text is not clipped behind the parent": the
card's own fill is only covering the header's worth, and everything below it is being drawn by
something else.

Headless captures did not show this -- not mid-growth, and not on shrink-then-regrow of the same
item, which was the other structural difference from the reported sequence. Both were checked,
which is why the next step was to capture the app's own display list rather than keep guessing
at the harness.

### 10.6 Root cause: the layout-motion scope leaked down the whole subtree

The scene dump named it. In the buggy frame the display list contains, inside the animating item:

```
PushClip (0,0) 632.0x169.4 r0        <- the item's own reveal, on its render box
DrawRect (0,0) 632.0x169.4 r12 white <- the item's fill
PushOffset (20.0,56.0)
PushClip (0,0) 592.0x95.6 r0         <- ??? inset 20px a side, and only 95.6 tall
```

`592 = 632 - 40`, the body's own horizontal margins. So the **body was imposing a reveal clip of
its own**, inset inside the item's, and it is what draws the second white rect measured in the
frame: 104..695 against the item's 85..714.

The cause is one line in `step_render_boxes`:

```rust
let scoped = inherited || arena.get(id).style.layout_motion.is_some();
```

`inherited` was threaded to every descendant, so a `layout_motion` declaration scoped the entire
subtree rather than the children whose layout actually changed. The accordion's body is
`height(fill)`, so it changes size every frame the item grows -- purely as a consequence of the
parent's animation -- and therefore took its own render track, its own spring, and its own reveal
clip. Two nested springs on the same geometry, clipping the same text.

That explains every observation at once: the artefact is mid-animation only because the tracks
exist only mid-animation; it is a *second*, inset rect because the second clip belongs to the
body, not the item; it survives a resize because a resize re-lands everything at once; and it
disappears at rest because settling drops both tracks at once.

The scope is now passed on only by a node that declares it, so it reaches exactly one level: the
direct children whose box a layout change actually moves. A descendant that declares
`layout_motion` itself is still scoped, and still scopes its own children, so nesting composes.

### 10.6.1 Tests

`tests/layout_motion_scope.rs` states the invariant in the engine's own terms: any node holding a
render track must either declare `layout_motion` or have a parent that does; a fill-sized body
does not take a track of its own; and no clip narrower than the animating item appears inside it.
The first two fail against the old traversal.

### 10.7 Capturing the real frame

Because headless captures demonstrably do not show this, the debugger can now save both what the
window presented and how it was told to draw it:

- **`S`** writes `/tmp/aurora-frame-N.png` (override the directory with `AURORA_CAPTURE_DIR`),
  taken in `on_present` before the overlays are drawn, so the file is the frame as the engine
  produced it.
- The same press writes **`aurora-frame-N.txt`** beside it: every chunk with its origin, bounds
  and inherited clips, plus each `PushOffset`, `PushClip`, `PushOpacity`, `BeginLayer` and
  `DrawRect`. Only the commands that decide where something lands, because a frame that draws the
  wrong shape is explained by one of those -- and text diffs between two frames far better than
  two screenshots do.

A pixel tells you something is wrong; the display list tells you which node drew it.

### 10.8 A measurement error worth recording

Mid-investigation it looked like content was escaping the clip entirely -- a screenshot showed
copy apparently painted in the gutter below the card. It was not. Three false readings in a row,
each from measuring colour instead of asking what the colour was:

1. A first scan counted the neighbouring card's antialiased outline as escaped content.
2. Requiring a majority of the row to match still counted it, because that edge spans the full
   card width.
3. Tightening the white tolerance then counted the page showing through the gutter, at
   `#F0F1F1`.

The clip was correct throughout, and so was the engine. The fix was to stop matching background
colours and detect *content* by darkness: the copy sits at luminance ~0.48 and every background
-- white, page, arena, and any blend of the three -- is above 0.9, so one cutoff separates them.
`tests/accordion_reveal.rs` now uses that, and asserts the clip rect equals the render box rather
than the flow box, so a future change that reverts to the laid-out box fails loudly.

`tests/accordion_reveal.rs` covers the appearance side: nothing is painted in the gutter below
the growing box at any of 24 stepped frames; the copy is progressively uncovered rather than
appearing whole; the copy is centred in its body rather than sitting above a gap; every item
behaves identically; and the reveal clip tracks the render box and not the flow box.

### 10.10 Suite

`cargo test -p aurora --no-fail-fast` → **307 passed / 13 failed**, the 13 byte-identical to the
Phase-0 baseline.

---

## 11. Drag-to-reorder: the offset counted the same movement twice

Reported as "the grabbed row gets the wrong offset moving up and down, so it ends up in the wrong
position". Two bugs, one in the demo and one in the engine.

### 11.1 The drop target was measured from the wrong place

`on_drag_step` computed its target from `index`, the row's slot at the moment its closure was
built:

```rust
let target = ((info.delta.y / STRIDE).round() as i32 + from as i32)...
```

A reorder rebuilds every row, so `from` is the *current* slot -- which means the pointer's
displacement, measured from where the gesture began, was added to a slot that had already been
moved by that same displacement. Each swap therefore counted twice, and the row travelled further
than the pointer with every stride it crossed.

The fix measures from `origin`, the slot occupied when the drag began. It lives in a signal
rather than a local because every reorder rebuilds each row and would discard a local.

### 11.2 The held row was FLIP-springing while being dragged

The offset also has to discount how far the reorder has carried the row's *slot*, which is what
the transform must not re-apply. Measuring that as `(index - origin) * STRIDE` assumes the slot
jumps instantly. It does not -- it springs, because `layout_motion` covers the held row like any
other child. Measured, the held row's render box walked `120 -> 115 -> 108 -> 99 -> 84 -> 73` over
ten frames instead of stepping to its new slot.

That is the engine half. **A node with a bound `x` or `y` translation is already being told
where to go every frame; springing its box toward the new layout slot commands the same axis
twice.** The transform follows the pointer while the box crawls after it. `sync_node` now snaps
such a node's box and lets the transform carry it. Scale and rotation are deliberately not
included -- they compose with a springing box without conflict, so `has_translation_bindings` is
narrower than the existing `has_pose_bindings` on purpose.

### 11.3 The one-frame jump

With the box snapping, the discount still had to be taken from `target` rather than `index`: on
the single frame a swap lands, the slot has already moved but the closure still holds the
pre-swap index, so the row jumped a full stride for exactly one frame. Measured per frame after
each change:

| | peak drift | shape |
| --- | --- | --- |
| pointer delta only | 81.5px | diverging |
| discount from `index` | 61.6px | oscillating, recovering |
| box snaps + discount from `target` | **0px** | tracks the cursor |

### 11.4 The fix disabled FLIP for every row, and no test noticed

The rule above -- snap the box of any node with a bound translation -- is too broad, and it broke
the demo's headline behaviour silently. Every row binds `.y()` unconditionally and reads zero when
it is not held, so *every* row matched and *none* of them sprang: measured `max |render - flow|`
across a drag was **0.00px**. The rows the drag did not touch teleported instead of FLIP-springing,
which is the entire point of the demo.

`tests/reorder_flip.rs` now asserts that untouched rows are still in flight mid-drag, and it fails
against the broad rule.

The rule is now tested against the *composed* translation rather than the presence of a binding:
a node that is currently displaced is transform-driven and must snap; a node reading zero is free
to spring. Both requirements then hold at once -- the held row tracks the cursor within a pixel,
and the untouched rows spring.

### 11.5 Touching a row teleported it towards the top

Reported as "when I grab an item it gets teleported to the top". The input router delivers
`on_drag_step` on *any* movement while the button is held, including one far smaller than the
drag threshold:

```rust
TrackerChange::Moved => { /* ... */ if self.tracker.is_held() { fire_drag_step } }
TrackerChange::DragStarted => { fire_drag_start }
```

So `drag_step` runs **before** `drag_start`. The slot the gesture began in was recorded on
`drag_start`, which meant the very first step still read an origin of zero -- and a target of
zero moves the row to slot 0. The row jumped the moment it was touched.

This was introduced by 11.1: the same pre-threshold step previously computed its target from the
row's own `index`, which was already correct, so the path was harmless. Moving the calculation to
`origin` made it destructive.

The origin is now recorded on `on_pointer_down`, which is the earliest moment the slot is known
and always precedes any drag step. `tests/reorder_flip.rs` presses a row and moves it by 1-2px,
asserting the order is untouched; reverted, it reports
`["Baseline", "Underline", "Strikethrough", "Squiggle", "Highlight"]` -- the touched row jumped
from slot 4 to slot 3.

### 11.6 Two tests were calibrated against the bug

Worth recording, because it is the failure mode of trusting a green suite. Both asserted
*incidental* symptoms of the double-count rather than the contract:

- one asserted the raw transform offset exceeded 20px, which is legitimately **zero** when a row
  lands exactly on a new slot -- which is what a whole number of strides does;
- one asserted the *last* row moved, which only held when the over-counted target shuffled the
  entire list, rather than the rows the drag actually passed.

They now assert what the demo promises: the held row sits under the pointer, and the row lands
the number of slots the pointer travelled.

### 11.7 Tests

`tests/reorder_flip.rs` asserts that untouched rows still FLIP-spring, and that touching a row
without dragging it does not reorder.

`tests/reorder_drag_offset.rs` asserts the row's middle stays within 1px of the cursor at every
step of a two-stride drag in both directions, that it lands on the right slot, that a held row does
not flip, and that re-grabbing a row which has already moved does not reuse its old slot -- the
second gesture runs handlers rebuilt from a different slot than the first.

### 11.8 Suite

`cargo test -p aurora --no-fail-fast` → **313 passed / 13 failed**, the 13 byte-identical to the
Phase-0 baseline.

---

## 12. Structural audit and refactor (phases 0-7)

An audit of the codebase for tangled, leaky, and duplicated abstractions, followed by the fixes it
surfaced. Every behavioural change below is gated on a regression test that was confirmed to fail
before the fix and pass after it.

### 12.1 Baseline discipline

`cargo test` without `--no-fail-fast` aborts at the first failing test *target*, so the reported
counts depend on target order and are not comparable run to run. All numbers here come from
`cargo test --workspace --no-fail-fast`, and the 13 known failures are compared **by name**, not
by count — a count of 13 that is a different 13 would otherwise pass unnoticed.

Two shell one-liners (`awk` over `test result:` lines, `sed` on the same) produced three different
totals for the same run. Parsing that output in Python was the only thing that gave a stable
number, matching the documented baseline exactly.

### 12.2 `active_render_tracks` was stranded after the first resize — **fixed**

`TreeArena` keeps a cached count of nodes holding a render-box spring. It gates the render-box
pass's early-out and its subtree pruning, so a wrong count is a permanent performance leak, not a
visual bug.

`snap_render_boxes` cleared every node's `render_track` without decrementing the count. Every other
path that ends a spring decremented: settling in `sync_node`, and `TreeArena::remove_with`. The
symptom was an impossible-to-produce-otherwise state: `has_render_tracks() == true` with
`moving_render_boxes() == 0`.

It fires on the **first** resize, not the second. `snap_render_boxes_next` is guarded by
`has_sized_once`, which is already true by the time the first resize arrives — so the earlier
reasoning about "the second resize" was wrong, and only instrumentation of every note/forget/clear
site established the real trigger. `snap_render_boxes` was the sole leak site.

The `!scoped` branch in `sync_node` had the same hole in a narrower form: it cleared a withdrawn
scope's track only when the box had *not* settled, so a node already at its flow box kept both a
dead track and an inflated count. Both are fixed.

Guard: `tests/render_track_accounting.rs` asserts the invariant `counted == (actual > 0)` rather than
a specific count, so any future hole anywhere in the accounting fails there.

### 12.3 `.transform(t).x(signal)` silently discarded `t` — **fixed**

Reconciliation composed the declared transform with the bound pose
(`declared.multiply(&pose)`), but the reactive update path assigned the pose wholesale
(`*transform = composed`). A node declared with both rendered correctly on mount and then lost its
declared transform on the first bound pose change — the bug was only visible from the second frame
on, which is why no test caught it.

Both writers now go through one `NodeBindings::compose_with_declared`. `NodeState` gained
`declared_transform`, because the composed value cannot be decomposed back out and the reactive
path needs the declared half to fold the pose into.

Guard: `tests/transform_composition.rs`.

### 12.4 `node_absolute_rect` did not project — **fixed**

The absolute-bounds query paired a node's absolute origin with its **local** size. That is correct
only while the accumulated transform is a pure translation; under rotation or scale it reported a
box in the wrong place with the wrong extent. It now delegates to the same
`compute_visual_bounds` the paint pass uses, so it is the axis-aligned envelope of what is drawn.

This one had a directly user-visible consequence for the test harness: aiming a pointer at the
centre of the reported bounds could land on empty space. Confirmed by asserting the centre hits the
box, which fails on the old code with `None`.

Guard: `tests/absolute_bounds.rs` (3 of its 5 tests fail against the old implementation).

### 12.5 Two hit-test implementations, the correct one unreachable — **fixed**

The router — the path every real pointer event takes — used `input::hit::hit_chain`, which honours
ancestor clips and walks children in reverse paint order. `Engine::hit_test` used
`tree::hit_test::hit_test`, which ignored clips and used document order. They agreed on flat,
unclipped, same-z trees, which is exactly why the duplication survived.

Concretely: a view clipped out of its parent was still clickable, and a `z_index`-stacked sibling
resolved backwards. `Engine::hit_test` now calls `top_key`, and `tree/hit_test.rs` is deleted.

Guard: `tests/hit_test_consistency.rs`, including an assertion that the clicked view is the painted
view.

A note on how that last test was written: the first version asserted stacked siblings resolved to
the higher-z one, and it failed. The cause was the test, not the code — the root lays out in a row
by default, so without `.absolute()` the siblings sit side by side and never overlap. Checking
painted pixels directly showed paint, the chain, and the public API all agreeing. There is no z-order
bug; `test_4_6_equal_z_index_resolved_by_tree_order` remains one of the 13 known failures.

### 12.6 Deleted as dead or as a trap

- **`ContentMode`** — the whole enum. Written at reconcile, set by two call sites
  (`examples/motion_accordion/panels.rs`, `tests/render_box.rs`), and read by nothing: `Clip` was
  never consulted, `Reflow` was never constructed, and `allows_content_derived_flow` was never
  called. Both call sites relied on `.clip(true)` for actual behaviour.
- **`layout_transition`** — not dead, but a redundant alias for `.layout_motion` that silently
  did nothing for its `Timed` variant (an `if let` with no `else`). Two tests were named after it
  and passed *through* the alias, so they did cover real behaviour; they were repointed at
  `.layout_motion` and renamed rather than deleted. Removing the alias removes the silent no-op.
- **`OverlapPlan`** — computed every frame and threaded through the compile pipeline as a field
  that nothing read: a full tree walk with damage-intersection tests, paying for nothing. Deleted
  with its plumbing.
- **`Engine.root`** — a duplicate of `scheduler.root`, manually re-synced on every remount.
  `scheduler.root` is now the single owner.

### 12.7 `#[component]` could not be used inside the crate — **fixed**

The macro emits absolute `::aurora::` paths, which resolve in a dependent crate but not here, so the
crate's own components had to be hand-rolled (there was a comment in `app/runner/pointer.rs`
documenting the workaround). `extern crate self as aurora` in the crate root makes one expansion
valid on both sides. Removing the alias again reproduces `E0433`.

Guard: an in-crate unit test in `dsl/component.rs`, since the failure mode is a compile error and
the test only guards anything if it is compiled.

### 12.8 Structural changes

- `runtime/pointer_input.rs` — pointer queue, dispatch, and gesture state, with the timing rule
  (history advances on the frame's own scaled delta) that had been mixed into frame orchestration.
- `runtime/node_query.rs` — key-based geometry reads with the coordinate space named at each
  method: `flow_rect` (parent-local), `render_rect` (what is drawn, what hit-testing follows),
  `absolute_rect` (window coordinates, projected).
- `runtime/diagnostics.rs` — added `EngineDiagnostics`, a single snapshot of every counter. The
  existing `debug_*` accessors now all read through it, so there is one derivation point instead of
  a dozen that can disagree. It lives in the existing diagnostics module rather than beside it.
- `dsl/group.rs` — `MotionScope` groups `layout_motion` and `transition`, so two engine concepts
  are no longer separate fields on the declarative surface. Builder signatures are unchanged.
- `dsl/reconcile_group.rs` — split into `ResolvedProps::evaluate` → `ChangeSet::detect` → apply.
  Props were previously read in two batches with node writes in between, so the second batch could
  observe a half-updated tree.
- `pipeline/compile/emit.rs` — `node_clip_radius` is now the single authority for the reveal-clip
  rule (square while the render box animates, declared radius once settled). The display-list
  push, the inherited-clip set a descendant chunk re-asserts, and the matching pop all consult it.
  These were three separate spellings of one rule; when they drift, a push/pop pair disagrees and
  corrupts the clip stack for every tile drawn afterwards.

### 12.9 Suite

**330 passed / 13 failed**, the same 13 by name. `src/` and `tests/` are `cargo fmt` clean; the
remaining 23 fmt diffs are all pre-existing in `examples/` and were left alone.

### 12.10 Still open

The accordion artefact from §4.2 is **not** explained by anything in this audit. A 592-wide
element still appears near the animating card's bottom edge, and it is absent from the scene dump.
Nothing here touched it, and nothing here should be read as having fixed it.

---

## 13. Removing engine-side transition (the client-owns-motion migration)

The engine animated things the client could not read, so a bound value and the box it was drawn in
disagreed. The reported symptom was in `motion_validate`: the button's width and its label both
derived from one `expanded` flag, the engine sprang the *box* toward the new width, and the label
re-evaluated to the longer word on frame one. For ~200ms "Validate" was laid out in a 52px box.

The fix was structural rather than case-by-case: **the engine now animates nothing.** Every value
a client wants animated has to be an animated signal the client owns, and anything bound to it moves
with it, including a child's text.

### 13.1 What was removed

- **`src/motion/`** — deleted wholesale (13 files). `NodeTransition`, `redirect_change`, `Target`,
  `Controller`, `AnimationBuilder`, `Timeline`, `SpringTrack`, `MotionState`, `MotionVector`,
  `MotionRuntime`, plus `apply`/`step`/`mode`/`timing`/`playback`. Its own module doc already read
  *"Superseded by `anim`: transitions now live in userland as signal adapters... remains only until
  its last caller migrates."*
- **`src/tree/render_box.rs`** — deleted. `step_render_boxes`, `snap_render_boxes`, `sync_node`, the
  one-level scope walk, and the reveal clip that keyed on a live spring.
- **`NodeState`** — `transition`, `motion`, `render_rect`, `render_track`, `render_driver`,
  `motion_scope_count`.
- **`TreeArena`** — `motion_scopes`, `active_render_tracks`, `note/forget_render_track`,
  `note/forget_motion_scope`, `refresh_motion_scope_counts`, `subtree_has_motion_scope`.
- **DSL** — `.animate()`, `.spring()`, `.layout_motion()`, and `MotionScope`.
- **`redirect_change`** — the interception in `FrameScheduler` that restored a node's previous
  value and handed the delta to an engine controller. Bound values now commit directly.
- **`Node::render_rect()`** — there is one box now. Paint, damage, hit-testing and bounds all read
  `resolved_rect`, so `flow_rect()` and `render_rect()` collapsed into one question.
- **`compute_damage_bounds`** — existed only to union the flow box with the animating box.
- **`collapses_visibly`** in layout — existed only to keep a collapsing node's interior measured
  while its render box was still away from zero.

`anim` is untouched and is now the only animation path. `has_active_motion()` is purely
ticker-based, which is what keeps the redraw loop running for a client-driven animation.

### 13.2 The client side

- **`motion_validate`** — width is now `Transition::spring(Spring::snappy()).on(expanded.map(..))`
  and the label reads *that* signal, so the word cannot outgrow its box. The label swaps at the
  midpoint of the spring rather than on frame one.
- **`motion_accordion`** — each item animates its own height; the container lays out at whatever
  the live heights are. No reveal clip is needed, because the height is a real layout value.
- **`motion_reorder`** — the FLIP is genuinely client-side. Each row springs its own slot position
  and carries `animated_slot - laid_out_slot` as an offset. A **held** row must not also carry a
  FLIP offset, because it is already being told where to go by the pointer; that rule used to live
  in the engine as `transform_driven` and is now written down in the demo.
- **`Signal::combine` / `Derived::combine`** added. There was previously no way to express a value
  depending on two signals, which a FLIP offset is by definition. Reading the second source inside a
  `map` would recompute only when the first moves — which happens to work for a spring that ticks
  every frame and silently does not for anything else.

### 13.3 The bug underneath both demos — silently shrunk rows

Six reorder tests and two accordion tests failed after the port. The cause was not the port.

Both demos declared sizes that **did not fit the space the page chrome left them**, so the layout
shrink pass quietly squeezed every child:

- reorder: rows wanted 290px in a 272px list, so each was squeezed from 50 to 44.65. The real pitch
  was **55.8** while every piece of drag arithmetic used `ROW + GAP = 60`. The held row was therefore
  displaced from the cursor by the difference on every reorder — 11.4px at worst.
- accordion: panels wanted 380px in ~372, so the open panel settled at 173.07 instead of 176 and the
  closed ones at ~54 instead of 56. This is also what put "content" in the gutter below a growing
  panel: the geometry the test measured was not the geometry that was drawn.

The engine's render box had been **hiding** this, because it reported the unsprung declared height
while layout held the shrunk one. Removing it exposed a layout fact that had been invisible.

Fixed by sizing the demos to fit (`ROW` 50 -> 46, `OPEN` 176 -> 168), which makes the declared and
measured geometry identical again. The duplicated constants were the reason this went unnoticed:
`OPEN` was restated in three files, so the demo and its tests could drift apart silently. The demo
now owns them (`accordion::panels::OPEN` / `HEADER`) and the tests read them.

The general lesson worth keeping: **a child that does not fit its parent is shrunk, and nothing in
a screenshot says so.** It reads as "the animation looks slightly wrong".

### 13.4 Coverage removed, and what replaced it

Deleted, because they guarded machinery that no longer exists: `render_track_accounting`,
`render_box`, `layout_motion_scope`, `render_box_hardening`, `reorder_flip`, `probe_run`,
`motion_stage1_substrate`, `motion_stage3_controllers`, `motion_stage3_transitions`,
`motion_stage4_layout`, `motion_stage5_signals`, `motion_stage6_timeline`.

Kept rather than deleted:

- `motion_stage2_math` — re-pointed at `anim` (19 tests survive); only the one test that drove the
  vector compositor was dropped.
- `motion_stage7_input` and `damage_parity` — ported from the `update_motion` hook onto
  signal-bound transforms, which is both the only path that exists now and a fairer test.
- `accordion_reveal` — kept four of five tests. The gutter test was re-based on the *moving*
  sibling rather than a fixed band, and its lockstep assertion was restored to exact equality once
  the shrink was fixed.

### 13.5 Suite

**268 passed / 13 failed.** The 13 are the same pre-existing failures, unchanged by name. `src/`,
`tests/` and `examples/` are all `cargo fmt` clean.

Test count fell from 330 because ~12 files went with the removed systems. The number to watch is
that nothing new fails.
