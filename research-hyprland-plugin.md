# Research: `riode-glass` — Hyprland region-select blur for riode

Goal: a region in a riode app stays transparent over the desktop while the
compositor blurs the underneath content inside that region. Geometry travels
over Wayland (`ext-background-effect-v1`); effect parameters travel over a
small plugin IPC (`hyprctl riode ...`); all pixel work (sample/blur/composite)
runs compositor-side in a Hyprland plugin.

This document is the plan. Status: research complete, no code written.
Visual correctness is judged by a human, never by a harness (AGENTS.md).

## 1. Why a plugin is required

A SHM client can never sample the desktop behind it — only the compositor
holds those pixels. So backdrop blur for underneath-content *must* run
compositor-side. The in-engine glass material (`crates/core/riode/src/
foundation/glass.rs`, `crates/engines/riode-engine-tiny-skia/src/glass.rs`)
refracts the *in-app* backdrop snapshot (P1 model) and stays as the
content-layer material; the plugin owns the *desktop* backdrop. Same math
family, different pixel source — do not conflate them.

## 2. What already exists

- **Wayland presentation exists.** `crates/platforms/riode-platform-wayland`
  presents scenes as a `Layer::Overlay` SHM surface (`src/backend.rs`,
  `src/surface.rs`). The dock (`apps/dock --overlay`, `apps/dock/src/
  overlay.rs`) floats over the desktop today; its "glass" bar is a flat
  `0.55`-alpha fill (`apps/dock/src/view.rs:28`) with no blur behind it.
- **Hyprland core implements the region protocol.** The vendored reference
  at `hyprglass/` (untracked — reference only, do not build on) reads
  `m_hasBackgroundEffect` / `m_blurRegion` off every surface
  (`hyprglass/src/GlassLayerSurface.cpp:131-146`). Those fields are
  populated by Hyprland's own `ext-background-effect-v1` server
  implementation: client sets region → compositor knows which pixels to
  blur. No custom IPC is needed for geometry.
- **Rust client bindings are one dependency away.**
  `wayland-protocols 0.32.13` ships `ext::background_effect::v1` client
  bindings and depends on `wayland-client 0.31.14` — compatible with this
  workspace (`wayland-client = "0.31"`, SCTK 0.19.2 in `Cargo.toml:54-55`).
  SCTK does not bundle the protocol, but no codegen / `wayland-scanner`
  binary is needed; add `wayland-protocols = { version = "0.32",
  features = ["staging", "client"] }`.
- **Reference for every hard part** lives in `hyprglass/`:
  - `renderLayer` hook + temp-FBO redirect: `src/main.cpp:412-511`
    (`hkRenderLayer`), sample/redirect: `src/GlassLayerSurface.cpp:281-441`.
  - Protocol-region mask upload as ≤64 rects:
    `src/GlassLayerSurface.cpp:530-577`, shader side `src/Shaders.hpp:81-112`.
  - Custom hyprctl commands: `src/Diagnostics.cpp:553-584`
    (`HyprlandAPI::registerHyprCtlCommand`, `COMMAND_MATCH_PREFIX`).
  - Version-guard pause (never crash on mismatch): `src/main.cpp:764-776`
    plus `src/LoadGuard.cpp`.
  - Unload-safe persistent-helper pattern for custom protocols:
    `src/item-helper/helper.c`, `src/ItemHints.cpp`.
- **Environment (verified 2026-10-18):** running Hyprland 0.56.2
  (`efb5099`) exactly matches the newest `hyprglass/hyprpm.toml` pin, so
  the mechanism is testable right now. `hyprpm` + `g++`/`make` present; no
  system Hyprland headers (`pkg-config hyprland` empty) → plugin builds go
  through `hyprpm`, not raw `make`. No `liquid_glass` example exists yet
  (ongoing Phase 4 in `ongoing-lq-work.md` not started).

## 3. Core design decision

**Region instructions travel over Wayland itself, not over plugin IPC —
and per effect-island, not per surface.** One `wl_surface` carries exactly
one `ext-background-effect-v1` blur region and one effect (blur), so a
single surface-wide region can never distinguish a blur group from a
glass group from a merely-transparent group. The unit of compositor effect
is therefore the *island*: one **top-level layer surface** per scene
effect layer (`BeginLayer` with `glass` and/or `filter`), positioned at
the node's rect. Each island surface carries its own effect state:

- `ext-background-effect-v1` blur region → the gate ("this island gets an
  effect") plus the pixel mask. No region, no hint → the plugin never
  touches the island (this is how plain-transparent groups stay plain).
- a `riode_item_v1` hint object (preset + rounded-rect shape, copied from
  the `hyprglass_item_v1` shape: inert-on-unload, double-buffered on
  commit) → the *what* (blur-only vs glass preset) and the shape.

Crucial correction (verified 2026-10-19 against core 0.56.2 source):
islands must be **top-level layer surfaces, NOT subsurfaces**.
`CBackgroundEffect::setResource` copies the region into the view via
`CWLSurface::fromResource`, which is null for layer-subsurface surfaces,
so regions set on them are **silently dropped on commit** (early return,
no error). Synchronized-subsurface islands were built and proven dead by
the plugin's own rejection counters (`noview_layer`); the demo now owns
one `Layer::Overlay` per island (anchor TOP|LEFT + margins), which takes
the proven top-level path. `IslandPool` (subsurface variant) is retained
in the platform crate for a possible window-subsurface future, but is not
the island mechanism.

`ext-background-effect-v1` is the *where* channel only: it carries zero
pixels and cannot render liquid glass (no refraction, dispersion, or
specular). All glass pixels are rendered by the plugin's own shader
(Phase 3). Native Hyprland blur is scaffolding: it proves the geometry
channel lands correctly (Phase 0/2) and is then suppressed on glassed
namespaces to avoid double-blur / stale-cache fights. Plugin IPC
(`hyprctl riode ...`) carries only *effect parameters* (radius/strength/
preset per namespace, status/stats), which change rarely and suit
`hyprctl` request/response. A `hyprctl` call per frame would be a socket
round-trip on the compositor's main thread — per-frame data travels via
commit-synchronized protocol state, never via IPC.

**No client-side API change.** `<Group glass={...}>` stays exactly as-is.
The DSL already emits everything needed (`BeginLayer { rect, glass,
filter }`); the scene→island translation lives below the public API in
the Wayland backend, which already receives the full scene every
`present()`. View code, the DSL, and the engines are untouched; non-Wayland
backends keep today's semantics (glass = in-app backdrop only).

```
riode app (Rust, riode-platform-wayland)
 │ ① per island subsurface: get_background_effect + set_blur_region(rect)
 │    each commit  ← geometry, fast path (reactive for free: layout change
 │    → new rects next commit; synchronized-subsurface timing is atomic)
 │ ② per island subsurface: riode_item_v1 set_preset/set_shape ← what+shape
 │ ③ hyprctl riode <set|status>  ← effect params, slow path
 ▼
Hyprland 0.56.x + riode-glass.so (C++)
  reads m_blurRegion per island surface (free, from core) + item hints →
  renderLayer hook → sample + effect + composite, masked to island region
```

Plugin language is **C++** (non-negotiable: `PLUGIN_API_VERSION` /
`PLUGIN_INIT` + Hyprland C++ internals over an unstable ABI; Rust cannot
consume them). x86_64-only for hooks. One plugin may own `renderLayer` —
`riode-glass` and hyprglass-with-layers cannot both hook it on one machine.

## 4. Protocol facts (`ext-background-effect-v1`)

Source: `wayland-protocols-0.32.13/protocols/staging/ext-background-effect/
ext-background-effect-v1.xml`, module `wayland_protocols::ext::
background_effect::v1`.

- Manager global → `get_background_effect(id, surface)`; at most one
  effect object per surface (`background_effect_exists` error otherwise).
- `set_blur_region(wl_region | null)`; copy semantics (region object may be
  destroyed immediately); NULL removes the effect. Double-buffered: applies
  on next `wl_surface.commit`. Region is surface-local, clipped to surface
  size by the compositor.
- `capabilities` event advertises the `blur` bit; if the capability goes
  away the effect stops applying even if previously set.
- Sibling reference: `hyprglass/protocols/hyprglass-item-v1.xml` shows the
  shape of a *hint* protocol (preset/shape per surface, inert-on-unload) —
  only needed if per-item hints ever become a requirement (Phase 5).

## 5. Phased plan (each phase ends at a human-eyes gate)

**Phase 0 — Prove the mechanism with zero plugin code.**
Bind `ext_background_effect_manager_v1` from globals in the dock overlay
driver (or `riode-platform-wayland`), call `get_background_effect` on the
layer `wl_surface`, upload the bar rect via `set_blur_region` each commit;
add `layerrule = blur, dock` to the Hyprland config. Human runs the dock
overlay and confirms the desktop blurs behind the bar.
Prediction: native Hyprland blur appears exactly inside the region, nowhere
else. A negative result proves native layer-blur doesn't honor the region
and the plugin becomes mandatory one phase early.

**Phase 1 — Minimal `riode-glass.so` skeleton.**
New tracked dir (proposed `plugins/riode-glass/`), C++23, copying the
`hyprglass` Makefile / `hyprpm.toml` / LoadGuard shape: `PLUGIN_INIT`
(version-hash guard → paused description, never crash), namespace
allowlist config, one `hyprctl riode status` command (Diagnostics pattern),
`PLUGIN_EXIT` teardown. No rendering. Human loads/unloads/reloads it and
confirms `hyprctl plugin list` is clean and the compositor survives
reload. No tests (AGENTS.md hard rule: no tests until human validates).

 **Phase 2 — Rust island plumbing (no client API change).**
 Island extraction as a pure function (`&Scene → Vec<Island>` — landed:
 `crates/core/riode/src/scene/islands.rs`, unit-tested), then the
 Wayland backend promotes each island to a synchronized subsurface
 (landed: `riode-platform-wayland/src/island_pool.rs` + `apps/
 desktop-glass` three-island demo: blur card / glass card / plain card,
 drifting so regions move every frame). Per-commit `set_blur_region` +
 (later) `set_preset`/`set_shape`, positioned at the node rect,
 intersected with the surface size. Empty island list clears.
 Validated against native blur on the three-group demo: blur lands
 exactly on the first two islands, nowhere else — the plain card proves
 exclusion by absence. (Supersedes the earlier single-surface
 `set_blur_regions` sketch: one surface-wide region cannot distinguish
 per-group effects.)

**Phase 3 — Plugin-owned blur (the "shader at compositor level").**
`renderLayer` hook in the `hkRenderLayer` pattern (pre-pass:
sample+blur+redirect to temp FBO; post-pass: composite masked by the
protocol region rects, 64-rect uniform approach), plus `noblur`-style
suppression of Hyprland's native blur on glassed namespaces to avoid
double-blur / stale-cache fights (`blur:new_optimizations` captures before
plugin decorations — the known "effect only shows while dragging" symptom).
Start with plain Gaussian at configurable radius — no liquid-glass optics
yet. Human A/Bs plugin blur vs native blur on the dock.

 **Phase 4 — Effect-parameter IPC.**
 `hyprctl riode set <namespace> radius|passes|preset`, per-namespace config
 with JSON `status`/`stats`. Per-island hints travel over `riode_item_v1`
 (landed: preset + rounded-rect shape, inert-on-unload, double-buffered;
 see `plugins/riode-glass/protocols/`, `src/item-helper/`,
 `crates/platforms/riode-platform-wayland/src/item_hints.rs`,
 `apps/desktop-glass/src/island_layers.rs`). Only then consider mirroring
 in-engine `Glass` material params to plugin uniforms.
 Two hard-won facts: (1) on 0.56.2, `wl_global` bind callbacks do NOT
 receive create-time data — generation checks on the bind argument mark
 everything inert; gate inertness on stop() only. (2) Every ABI bump loads
 yet another resident helper copy (RTLD_NODELETE by design); adoption
 reuses the newest matching one, so bump only when the layout changes.

**Phase 5 (explicitly deferred) — subsurface items, custom hint protocol,
tests.** A `riode_item_v1`-style hint protocol only if per-item
preset/shape is ever needed (copy the persistent-helper pattern); tests
only after human validation of Phase 3+ visuals.

## 6. Risks and constraints

- Hooks target private symbols; they break across Hyprland minor versions
  → `hyprpm.toml` commit pins per release + pause-on-mismatch (proven in
  reference). Track `hyprland-0.56` first (matches running compositor).
- `PLUGIN_EXIT` is not guaranteed (crash/forced eject) → never rely on it
  for client-visible state; custom protocols need the persistent-helper
  pattern if Phase 5 happens.
- Compositor main loop is single-threaded; no blocking calls in
  `PLUGIN_INIT` or render hooks.
- AGENTS.md hard rules apply throughout: demos over descriptions, human
  owns the verdict, one root cause / one fix, commit only on explicit human
  approval.

## 7. Open questions (recommendations in parentheses)

1. First target: dock bar (existing overlay, simplest region) or splein's
   select-region tool? (Recommend: dock bar.)
2. Is native-blur-behind-region an acceptable v1, plugin shader in Phase 3
   — or plugin-owned rendering from the start? (Recommend: native first.)
3. Fresh `riode-glass` plugin vs forking vendored hyprglass?
   (Recommend: fresh and minimal, namespace `riode`, borrowing patterns
   only — avoids inheriting liquid-glass complexity and the
   `hyprglass_item_v1` protocol surface.)
4. Plugin home: `plugins/riode-glass/` in this repo (tracked from day one)
   vs separate repo via hyprpm? (Recommend: in-repo while iterating,
   split on first release.)

## 8. Pixel ownership: full shader control without Hyprland's inner shaders

Requirement: the plugin has direct pixel access and redefines the shader —
full control of look and feel, no reliance on Hyprland's blur/shading that
could drift across versions. Verdict: **feasible, and already ~90% how the
reference works.** "Relying on the compositor" splits into two layers with
very different answers.

### 8.1 Where the reference draws the line (verified in source)

**Owned — the look.** Own fragment shaders embedded in the binary
(`hyprglass/src/Shaders.hpp`: `liquidglass.frag`, `gaussianblur.frag`;
compiled in `src/ShaderManager.cpp:20-100`), own blur kernel with own
radius/fold math (`src/GlassRenderer.cpp:358-388`), own tone mapping, tint,
refraction. The glass path never calls Hyprland's blur or Hyprland's
surface shader. The only borrowed shader text is `TEXVERTSRC` — Hyprland's
trivial fullscreen-quad passthrough vertex shader, inlinable in ~5 lines.

**Borrowed — integration, not look.**
`g_pHyprOpenGL->bindFramebuffer / setViewport / scissor / setActiveTexture /
useShader` (`src/GlassRenderer.cpp`, `src/GlassLayerSurface.cpp:423-447`)
are GL *state setters* keeping Hyprland's GL state tracker in sync —
bypassing them with raw GL trips the desync detector
(`recordStateDesync`, `src/Diagnostics.cpp`). Plus: FBO allocation (dumb
GPU memory), pass hook points, damage in/out, config/event plumbing, and
`renderTextureInternal` for drawing *content* (`src/GlassRenderer.cpp:250`)
— not glass.

`ext-background-effect-v1` carries **zero pixels** (region declarations
only); all pixel flow stays inside the compositor either way (§2–§3).

### 8.2 The contract, precisely stated

- **Achievable:** identical shader sources + identical uniforms + identical
  input pixels ⇒ identical output pixels, across Hyprland versions *and*
  GPUs. Pin it: embed own vert+frag, hash the sources, log the hash in
  `hyprctl riode status`.
- **Not achievable:** guaranteeing the *input* pixels and *output* encoding
  are identical across Hyprland versions. The plugin cannot own when/where
  it draws (pass system, hook signatures, damage contracts — violating
  `boundingBox`/`needsLiveBlur` aborts the compositor), what pixels it is
  handed (source FB encoding), or what happens downstream (target FB format
  XRGB2101010 vs FP16, color management). An HDR/CMS change in Hyprland can
  change final appearance with byte-identical plugin shaders.
- **Failure mode this buys:** version drift surfaces at *load time* (hash
  guard → paused plugin, no wrong look) or as *detectable* input/output
  changes (report FB format / CMS path in `status`; re-verify per release
  with human eyes) — never as silent shader drift.

### 8.3 Implementation rules for `riode-glass`

1. **Own everything that touches a pixel value:** embedded shaders (vertex
   too — drop the `TEXVERTSRC` borrow), own separable-Gaussian pre-blurs at
   plateau/rim sigma, own sRGB→linear→sRGB discipline, own triangular
   dither, own SDF/shape, own param→uniform mapping.
2. **Keep the thin state-setter wrappers** (`bindFramebuffer`, `setViewport`,
   `scissor`, `setActiveTexture`, `useShader`). They are not "inner shaders"
   and bypassing them buys nothing but tracker-desync bugs.
3. **No driver-defined filtering in the pixel path.** Avoid
   `glGenerateMipmap` for any pyramid — mip-filtering differs between GPUs
   and becomes cross-machine variance. Use explicit downsample passes with
   own taps (P1's 13-tap `FS_DOWN` pattern,
   `apple-glass/webgl-sdf-shader/shaders/`).
4. **Branch on the target, don't assume it:** query the output FB format
   (SDR 8-bit vs FP16/HDR) and adapt the encode path; log which branch ran.
5. **Per-release integration shims:** hook signatures, struct field names,
   FB-format branches adapt per Hyprland release behind `hyprpm.toml` pins;
   the shader sources stay identical across pins. Test each pin with human
   eyes on the dock demo (static + magnification + video behind).
