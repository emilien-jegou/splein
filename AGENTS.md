# AGENTS.md — Visual Bug Protocol (Human-Pair Mode)

Visual bugs are judged by a human, never by the agent's harness. Snapshot
tests, headless renders, and unit tests are **not** sources of truth for
visual correctness. They may guard against regressions after a fix lands;
they may never declare a fix correct.

## Roles

- **Human**: owns eyes and verdict. Runs demos, takes screenshots and frame
  captures, reports what is seen. The *only* authority that closes a bug.
- **Agent**: owns code reading, root-cause reasoning, and edits. Never
  claims a visual fix works — it states predictions and asks for them to
  be checked.

## The Loop

1. **Reproduce minimal.** Build or extend the smallest demo that isolates
   the symptom (e.g. `crates/riode/examples/text_smooth.rs`: identical
   content drifting on X, Y, and diagonal at subpixel velocity). One
   variable per row. If the symptom needs motion, drive it from the frame
   clock so it moves from the first frame.
2. **Diagnose read-only.** Trace the full path the symptom travels
   (signals → layout → damage → compile → emit → raster → present) *before*
   proposing anything. Eliminate suspects with evidence, in writing. When
   every in-house stage is exonerated, say which dependency or boundary is
   left — do not gesture at it, name the exact call.
3. **Plan decisively, then implement.** One root cause, one fix, no
   "fallback" hedging. If two hypotheses survive, design the fix or the
   discriminating observation that kills one — never ship both and see.
4. **Hand off with predictions, not claims.** Every fix ships with:
   - the exact command(s) the human runs,
   - numbered observable predictions (what each demo row / screenshot
     must show if the fix is right),
   - what a negative result proves (which path it reopens).
   Example: "Y advances on *every* `.` press; if it still jumps 7-then-1,
   the frames are stale and the next fix moves to invalidation."
5. **Commit only on explicit human approval.** Visual "looks good" from
   the human is the merge gate. State the commit message in the agent's
   words; the human says go.

## Hard Rules

- **Do not run visual harnesses, snapshot suites, or pixel-diff tooling
  as validation.** They may be *compiled* to keep the tree green; they
  may not pass judgement on appearance.
- **Do not create tests until the human has validated the fix.** Tests
  lock in behavior — locking in unverified visuals bakes bugs into the
  suite.
- **Do not present screenshots, captures, or test output as proof.**
  Only the human's report counts.
- **Debug tooling the human operates**: `F8` freeze/unfreeze the clock,
  `.` advance exactly one animation quantum (1/120 s) while frozen,
  `[` / `]` slow-motion time scale, `s` write presented pixels to
  `$AURORA_CAPTURE_DIR/riode-frame-N.png` for zoomed pixel-diffing.
  Point the human at these instead of inventing new ones.
- **External debug window**: `F5` opens the demo menu, `F7` cycles raster
  engines, `F10` toggles the separate debug panel window (telemetry,
  inspector, capture — same actions as the keys above, clickable).
  The app view itself carries no debug pixels; inspector heatmaps
  (`F9`/`F11`/`F12`) still overlay the app view by design.
- **Demos over descriptions.** When a symptom is disputed, change the
  demo (smaller travel, extra size row, frozen driver) so the next
  observation discriminates — don't debate the last screenshot.

## Examples

- **Run:** `nu scripts/toolbox.nu example <name>` (e.g. `nu scripts/toolbox.nu example text_smooth`).
  Release (no hot-reload): `cargo run -p riode-examples --example <name> --release`.
  Window chrome comes from `view::HOT_*` consts, read out of the freshly
  built UI image before the window is created — pass no geometry.
- **Toolbox:** `nu scripts/toolbox.nu` lists dev commands (`build`, `test`,
  `watch`, `example`, `relacher`, `release-github`); inside devenv, `dev`
  does the same from anywhere. For native arg completion of `example` in
  interactive nushell: `use ~/projects/splein/scripts/toolbox.nu [example]`
  (import selectively — bare `test` would shadow the builtin).
- **Anatomy (one folder per example, keep in sync):**
  - `examples/<name>/view.rs` — the view only (`Page` component, no `main`),
    plus `HOT_TITLE`/`HOT_WIDTH`/`HOT_HEIGHT`/`HOT_BG` consts: the single
    source for window chrome, referenced by both drivers below.
    Edit this while `nu scripts/toolbox.nu example` runs; the window rebuilds live.
  - `examples/<name>/main.rs` — release driver: `#[path]`-includes `view.rs`
    (+ shared `common/` helpers), registers fonts, calls `.run(...)` with
    `view::HOT_*` for size/title/background.
  - `examples/<name>/hot.rs` — hot entry: `#[path]`-wiring plus one
    `riode::define_hot_ui! { root, title, size, background }` block emitting
    all four entry points (root + `riode_hot_window`/`riode_hot_title`,
    read from the same `view::HOT_*` consts). The generative
    `examples/riode-hot-gen/` crate (`riode-hot-gen-ui`, `crate-type =
    ["cdylib"]`, depending **only** on `riode-shared` — never `riode`
    directly) mounts it per `AURORA_HOT_EXAMPLE`.
- **New example checklist:** copy an existing folder (e.g. `text_smooth`),
  rename, write `hot.rs` (wiring + one `define_hot_ui! { ... }` block, see
  `text_smooth/hot.rs`). No workspace member needed. Keep `view.rs` as the
  single source of truth.
- **Honesty over closure.** "I have not proven the culprit" is always
  acceptable; "this should fix it" without a mechanism is never. Name
  what was eliminated, what remains, and what single observation decides.
- **Human guy.** The human is there to help you as much as you are here to help
  him, rely on him for visual testing or opinion.

## Why This Exists

Agent-run visual verification converges on plausible-looking wrong
answers: the harness renders through the same buggy path it validates,
snapshots enshrine artifacts, and each green check drains urgency while
the human still sees tears. Fine pixel debugging is hard and require a Human in
the loop. Human eyes on minimal demos found, in order: blend-tear seams,
phantom line breaks, phase-swap morphing, axis-split raster policy, stale-frame
quantization, and invalidation ghosts — none of which any suite flagged. Keep
it that way.
