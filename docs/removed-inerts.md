# Removed inert glass knobs

The `GlassBank` fields dropped in Phase 2 of `ongoing-glass-upgrade-v2.md`,
what each was *supposed* to do, and whether any deserve to come back.

Every field here was **`0.0` in all five shipped presets** (`regular`,
`clear`, `regular_dark`, `clear_bare`, `clear_dark`), so removing them is
provably a no-op on the shipped look — the uniform wire was kept
byte-identical (the packer writes literal `0.0` in their slots) and the
shader's dead reads were deleted. The `riode-anodeui` `fitted_rows` tests
passing is the proof the fields were zero.

Two categories, because they died differently:

* **Relocated** — moved to the general `Shadow` API in Phase 1 (not truly
  dropped; they live on, on `Shadow`).
* **Dropped** — the iOS-26 tab-lens / artistic terms no preset ever set.

---

## Relocated → `Shadow` (Phase 1)

The glass cast shadow became a plain Gaussian drop shadow drawn as a normal
`Shadow` layer *behind* the glass, so these left `Glass`/`GlassBank` for the
general `Shadow` API.

| Field | What it was supposed to do |
|---|---|
| `shadow_radius` | Falloff radius of the glass's drop shadow outside the shape. |
| `shadow_opacity` | Shadow strength at the shape edge. |
| `shadow_offset_y` | How far (logical px) the shadow is moved down; `0` centred it. |

These are **not return candidates** — `Shadow` owns them now (see
`foundation::shadow::Shadow`, `GlassPreset.shadow`, and the `Shadow::layered(…)`
build in `riode-anodeui`'s `lens()`).

---

## Dropped (zero in every preset)

Grouped by why they died.

### The overhanging-lens "bar" (`backdrop_*`, `band_*`)

Refract only a vertical *band* of the backdrop — the bar visible under an
overhanging lens (its edge, its shadow, the page beside it) — instead of the
whole backdrop. Never enabled; the shader's `inBand` clamp and the Q/R slots
went with them.

| Field | What it was supposed to do |
|---|---|
| `backdrop_inset` | When `> 0`, the glass refracts only the backdrop between the shape's top and bottom inset by this much (logical px). `0` = the whole backdrop. |
| `backdrop_reach` | How far (logical px) *beyond* that inset band the glass still refracts. |
| `band_split_drop` | How much of the colour split across the rim is dropped where the glass reaches past the band. |

**Verdict:** drop for good. Niche tab-lens artifact; the Phase 5
bevel/magnification height field is the better tool for the overhang case.

### Sideways / capsule-end optics (`dispersion_tangent`, `lens_ellipse`, `lens_vertical`)

Geometric niceties for capsules and pills (tab lenses).

| Field | What it was supposed to do |
|---|---|
| `dispersion_tangent` | Sideways colour split along the outline *tangent*, as a share of the lens displacement (fringe only at a capsule's round ends). `0` split only along the normal. |
| `lens_ellipse` | `0–1`: bend along the normals of the ellipse inscribed in the shape instead of the outline's own (smooths the bend toward capsule ends). |
| `lens_vertical` | How much the lens is limited to where the outline faces up/down: `0` bends all round; `1` leaves the sideways-facing outline unbent (clear capsule ends). |

**Verdict:**
* `lens_ellipse` / `lens_vertical` — likely obsoleted by Phase 5 bevel
  (a real height field handles the overhang better). Don't re-add.
* `dispersion_tangent` — **distinct mechanism** from Phase 3 dispersion
  (which is a *wavelength loop*, `ior(λ)`, not a tangent offset). If physical
  dispersion alone under-foots the edges, this artistic control could return.

### Rim colour (`rim_tint`, `rim_hue`)

| Field | What it was supposed to do |
|---|---|
| `rim_tint` | How far the isotropic white rim turns toward `rim_hue`. Negative split the rim's colour by side instead. |
| `rim_hue` | Hue of `rim_tint`, in degrees. |

**Verdict:** superseded by Phase 4's GGX specular + Fresnel, which give the
rim a physically-driven colour from the environment. If that rim reads too
neutral, these could return as a post-hoc artistic *override*.

### Ambient luma (`ambient_lift`)

| Field | What it was supposed to do |
|---|---|
| `ambient_lift` | How much the whole glass lightens with the average *luma* of the backdrop under it beyond `0.2` (a `5×5` tap average over the blended rect). `0` = off. |

**Verdict — the one you might miss.** Its sibling `ambient_mix` /
`ambient_reach` (`P.x` / `P.y`, the *chroma* half) is **kept and live**;
`ambient_lift` was the *luma* twin. If you ever want the glass to brighten
over bright backdrops, this is the knob — trivial to re-add.

---

## Would any be useful to keep or add?

**None need to return for the planned phases.** Every knob the remaining
phases add is *new*, not a revival:

* **Phase 3 — dispersion:** `ior`, `dispersion` (spread), `dispersion_reach`.
* **Phase 4 — reflection/lighting:** Fresnel, `env_tint`/`env_intensity`,
  GGX `specular`/`roughness`/`spec_width`, `inner_glow`/`inner_shadow`,
  `caustic`.
* **Phase 5 — bevel/magnification:** `magnify`, `thickness`, `bevel`.

Ranked by how much you might miss them:

1. **`ambient_lift`** — only one with a live, used sibling. Re-add if you
   want backdrop-luma brightening.
2. **`dispersion_tangent`** — distinct from Phase 3's wavelength loop; a
   possible edge-fringe add-on later.
3. **`rim_tint` / `rim_hue`** — possible artistic override if the physical
   rim is too neutral.
4. **`lens_ellipse` / `lens_vertical`** — capsule-end niceties; Phase 5
   bevel likely supersedes them.
5. **`backdrop_inset` / `backdrop_reach` / `band_split_drop`** — niche;
   Phase 5 is the better tool.
6. **`shadow_*` (3)** — relocated, not return candidates.
