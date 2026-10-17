# Overlay glass pipeline — failure cases & diagnosis playbook

Companion to `AGENTS.md`: its visual protocol still governs (the human is
the only verdict; ship predictions, never claims). This file is the case
library for the overlay → plugin glass path plus the toolkit that found
each root cause. Read it BEFORE debugging "no glass", "wrong glass", or
motion/damage artifacts in `plugins/riode-glass` or the overlay runner.

## Where things live

- **Client** (`crates/platforms/riode-platform-wayland`): `runner.rs`
  schedules parent/island repaints; `island_layers.rs` configures island
  surfaces and uploads glass frames/shapes; `item_hints.rs` sends
  `set_glass`/`set_shape` (empty list ⇒ `unset_glass`).
- **Helper** (`plugins/riode-glass/src/item-helper`, ABI 7, wire v5 =
  799 words): hints go `pending → (snapshot on precommit, apply on
  commit) → current`; `get()` reads `current` only. `pending` survives
  applies — late attachment can still catch up with one snapshot+apply.
- **Plugin** (`plugins/riode-glass/src`): `hkRenderPassAdd` runs at pass
  BUILD time (Hyprland rebuilds the pass every frame): it attaches hint
  listeners, queues `CGlassBlurElement`, records bounding boxes into
  `g_renderBoxes`. `hkRenderPassRender` wraps `CRenderPass::render` and
  expands frame damage by those boxes. `CGlassBlurElement::draw` runs
  mid-pass: blits the pass FB (capture) → gauss/optics → scissored
  composite.
- **Log**: `/tmp/riode-glass.log` (override `RIODE_GLASS_LOG`). The
  plugin lives in **Hyprland's** process — the env var must be set on
  the compositor, not on the demo (`RIODE_GLASS_LOG=… dev example …`
  does NOT work). Wall-clock stamped; change-gated.

Status probe (any time):

```
hyprctl -j riode status | jq '{items_detail, geo_last, regions_last_frame, optics_draws, exits, gl_errors}'
```

- `regions_last_frame` is **reset-on-read**, not per frame — a big
  number only means "since the last probe".
- `geo` reports monitor-local, `geo_last` reports global coords
  (stacked monitors ⇒ y differs by the monitor offset) — not directly
  comparable.
- `items_detail` `glass:…` = hint latched; `-:none` = helper never
  applied (Case 2).

## Case 1 — islands never render (static view, overlay)

**Fingerprint.** No glass (or no islands at all) in a static demo while
`desktop_glass` works; animations elsewhere would have masked it.

**Cause.** Island `configure` lands while the parent is
`waiting_for_frame` ⇒ `paint()` early-returns; `frame()`'s repaint gate
was `hot_redraw || has_active_motion` ⇒ a static view never paints again
⇒ islands never upload.

**Fix.** `runner.rs`: `pending_island_paint` — set when an island
configures, cleared in `paint()` past its guards, OR-ed into `frame()`.

**Invariant.** Island lifecycle counts as dirt, independent of
hot-reload/motion. Static demo shows nothing ⇒ suspect the schedule
before the renderer.

## Case 2 — blur but no glass (hints never latch)

**Fingerprint.** Cards render with plain blur; status shows
`items_detail: "-:none ×N"`, `geo_last … optics=0`, `optics_draws 0`,
shape zeros. Engine-painted content (text) can look "fine", which is
what makes it deceptive.

**Cause.** The helper applies hints only via the surface's
precommit/commit listeners, which were attached **lazily inside the
draw path** — strictly after the client's first (and, for a static
client, only) hint commit. Pending sits unapplied forever; animated
clients (commits every frame) self-heal and hide the race.

**Fix.** `ItemHints.cpp`: `set_callbacks(on_created, …)` attaches at
`get_item` time, and `ensureTracked` bootstraps `snapshot+apply` when
it attaches late (safe: `pending` is retained; draws are atomic wrt
request batches; both run on the compositor thread).

**Invariant.** A client that commits exactly once must latch. Never
defer state application to a listener attached by the consumer.

## Case 3 — hover-only artifact (frame-damage freshness)

**Fingerprint** (need all four to trust the class):

1. Artifact only while the cursor **moves over** the glass — dragging
   other windows across it does nothing.
2. Self-heals ~0.5–1 s after the cursor stops (any periodic damage
   wipes it — a blinking terminal cursor behind the demo suffices).
3. It vanishes under full damage: `hyprctl keyword debug:pass 1`
   (restore with `debug:pass 0`).
4. Possibly island-position-dependent (one island immune).

**Cause** (Hyprland v0.56.2 / `efb5099`, `src/render/pass/Pass.cpp`).
`CRenderPass::render` expands frame damage by
`(union of needsLiveBlur bboxes ∩ damage) ⊕ decoration:blur-radius`
only. A cursor frame ⇒ fresh **blob** inside our box; the rest of the
pass FB still holds the previous frame — which includes **our own last
lens output**. `CGlassBlurElement::draw` blits its whole capture box
unclipped ⇒ the optics re-lens their own output ⇒ feedback (invisible
on a flat backdrop, glaring with detail). Our `needsLiveBlur()==true`
comment promised a full re-render below us; Hyprland's expansion does
not deliver it for damage we don't generate ourselves.

**Fix.** Complete our half of the contract in the plugin:

- `hkRenderPassAdd` (build) records each glass element's
  `boundingBox()` scaled into `g_renderBoxes`;
- hook `CRenderPass::render` (`findFunctionsByName(handle,
  "CRenderPass6render")`): `expanded = damage ∪ boxes` (skip when
  `damage.empty()`), call original, clear the list;
- `install`/`uninstall` wire the second hook beside the add hook.

**Invariant.** Whenever any damage exists, our entire sample box is
re-rendered fresh below us. Stale-cursor and stale-lens both die.

**Diagnosis ladder** (the order that worked — do not skip to theory;
AGENTS.md: code-derived motion theories burn sessions):

1. **State instrumentation** in the change-gated log: entry
   scissor/viewport/stencil of our draw + timestamps. If artifact and
   OK frames log identical state, our draw inputs are exonerated —
   say so in writing.
2. **`debug:pass 1` A/B** — zero-code class test (Hyprland's own full-
   damage switch). Vanishes ⇒ damage-scoping class. Persists ⇒ the
   corruption enters outside our element (island surface path / cursor
   compositing).
3. **Capture digest** — five `glReadPixels` points of the sample FB
   right after the blit, change-gated. Signatures: statically-dark
   pixels lifting to midtones during motion ⇒ lens-of-self feedback;
   single-point light spikes ⇒ cursor pixels (benign); flat constants
   ⇒ sample is fresh, look downstream of the sample. Pair probes with
   the preceding draw line's `surfLocal` for per-island attribution.
   Strip the digest afterwards — readback is a per-frame pipeline
   stall.
4. Only now change code — one fix, one mechanism.

**Gotchas.**

- Logs span sessions: split at `[riode-glass] exiting`/`loaded` lines
  before comparing (pre-fix traces will lie to you).
- The change-gated logger shares one `last` string across line types —
  interleaving re-logs lines; near-complete traces are a feature.
- `RIODE_GLASS_LOG` must be set for **Hyprland**, not the demo.
- An island that never shows the artifact may stay unexplained after
  the fix — record it, don't block on it.
- Known residue: `gl_errors` drains GL errors after each draw (origin
  unnamed); `clip stack underflow` on `clip={true}` roots.

## Overlay quick ref

- `dev example <name> wgpu --overlay` — overlay is always headless
  tiny-skia; the engine arg is accepted but inert (the script says so).
  Glass in overlay = **the plugin**; the raster side draws none.
- `scripts/example.sh` gates overlay names and exports
  `RIODE_HOT_EXAMPLE`; `apps/desktop-glass` reads it (`hot_example()`).
- `RIODE_COMPOSITOR_GLASS=1` (set by the overlay runner) ⇒ demo views
  may drop their fake window background (prism_only halves go
  transparent).
- Plugin reload: `nu scripts/toolbox.nu load-plugin` — **restart the
  demo afterwards** (unload invalidates its item bindings).
