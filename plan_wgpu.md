# plan_wgpu.md — Production UI Features for the wgpu Engine

## Goal

Bring the wgpu engine to feature parity with modern UI framework needs so it can serve as the primary renderer for text-heavy 4K UI, with Vello as a fallback for arbitrary vector content.

**Current state**: solid rects, SDF rounded rects, R8 atlas text, damage-aware partial redraw, chunk cache with origin-rebased replay.

**Target state**: borders/strokes, linear-space blending, gradients, shadows, images, dithering, and subpixel tuning — all validated by human observation per the visual bug protocol.

---

## 1. Borders / Strokes on Rects

> **Progress — Bundle 1 (with §2)**
> - [x] Premise corrected: stroked *square* rects already route through
>   `emit_rrect`/`rounded.wgsl` (`command.rs:574-590`) — the ring +
>   `StrokeAlign` math exists. Real gaps: per-edge widths (no core `Edges`
>   type anywhere), no double-wall formulation, no dashes.
> - [x] Per-edge widths **approved**. New core `Edges<T>` type;
>   `Stroke` gains `per_edge: Option<Edges<f32>>` + `widths()` accessor.
>   Legacy `.width` readers (vello, tiny-skia) keep compiling and degrade
>   to uniform-max. `appearance_bits` extended so chunk keys stay correct.
> - [x] Vertex restructure (once, now): `RRectVertex` becomes
>   `{pos, fbox(outer), radii(outer), ibox(inner), iradii(inner), fill,
>   stroke, grad}` — StrokeAlign fully resolved CPU-side; `expand` leaves
>   the shader. `grad` vec4 reserved (zeroed) so §3 becomes shader-only.
> - [x] Double-wall algorithm chosen: outer/inner walls; per-edge inner
>   inset; per-corner inner radius = `max(r − adjacent edges, 0)`.
> - [x] `append_rebased` must translate `ibox` alongside `fbox`.
> - [x] Dashed borders: **deferred** (not in Bundle 1; gpui
>   velocity-matched arc-length parameterization is a separate effort).
> - [x] Implement: core `Edges` + `Stroke.per_edge` + `widths()`.
> - [x] Implement: `RRectVertex` restructure + double-wall `rounded.wgsl`.
> - [x] Implement: `emit_rrect` resolves per-edge/align → outer+inner boxes.
> - [x] Implement: `append_rebased` translates `ibox`; `appearance_bits`
>   hashes per-edge.
> - [x] Demo: new `border_lab` example (per-edge widths, align ×3, radii ×3,
>   hairline, translucent overlap, text-on-translucency) — registered in
>   `examples/Cargo.toml`, `cargo check --workspace --examples` green.
> - [x] Human validation (Bundle 1): **"looks good"** — per-edge widths,
>   alignment, radii, hairline, overlap patch and text all as predicted.
>   One launch crash fixed en route (format edit landed on the headless
>   surface config instead of `make_persistent`; corrected so the scene
>   pass and pipelines both see `Rgba8UnormSrgb`).

### Why

Cards, buttons, inputs, and dividers all need borders. Currently only rounded rects can stroke (SDF ring). Solid rects cannot.

### Algorithm — SDF Double-Wall

For a rect with outer bounds `B`, inner bounds `B' = B - border_width`, corner radii `r`:

```
outer_sdf = sd_round_box(p, B, r)
inner_sdf = sd_round_box(p, B', r - border_width)   // clamped to >= 0

border_region = max(outer_sdf < 0 && inner_sdf > 0)   // between walls
fill_region = inner_sdf < 0

alpha_fill = saturate(0.5 - inner_sdf)
alpha_border = saturate(0.5 - outer_sdf) * saturate(0.5 + inner_sdf)

color = mix(border_color, fill_color, step(inner_sdf, 0))
final_alpha = max(alpha_fill, alpha_border)
```

For dashed borders: compute perimeter parameter `t` along the quad edge, apply dash pattern `step(fmod(t, period), length)`. Corner dashes use velocity-matched arc-length parameterization (gpui approach).

### Caching

No new cache. Borders are shader-only. The existing `ChunkCache` already caches the emitted `RRectVertex` data — border fields ride along.

### Intended use

All `SceneCommand::DrawRect` with non-zero `stroke.width`. Per-edge widths supported (`Edges<f32>`). Border color is sRGB-encoded, converted to linear in the shader before blending.

---

## 2. Linear-Space Blending

> **Progress — Bundle 1 (with §1)**
> - [x] Strategy decided: **hardware linear blending**. Persistent target
>   flips `Rgba8Unorm` → `Rgba8UnormSrgb`: the GPU decodes on texture read,
>   blends in linear, encodes on store. Correct for *every* composite
>   (including overlapping translucent quads) with no per-shader encode
>   dance — supersedes the per-fragment decode/encode loop below.
> - [x] Blit inverts: `SRGB_DECODE` → `SRGB_ENCODE` (encode only when the
>   swapchain is plain Unorm; sRGB swapchains store linear as-is).
>   Premultiply now lands in linear (after the srgb-target textureLoad
>   decode), fixing the premultiply-before-decode caveat.
> - [x] Shaders decode CPU sRGB vertex colors with the exact EOTF; output
>   is straight-alpha linear. Coverage gamma LUT in `glyph.wgsl` is
>   unaffected (it is a coverage correction, not a color-space one).
> - [x] Clear value and damage-base quad colors decoded to linear
>   (WebGPU treats `clearValue` as linear on sRGB attachments).
> - [x] `pick_format` preference unchanged (non-sRGB swapchain still
>   ideal — the blit encodes on the way out); doc comments updated.
> - [x] Implement shader/format/blit/clear changes; compile green
>   (`cargo check --workspace --examples`, 0 errors).
> - [x] Human validation bundled with §1 (same demo pass) — **"looks good"**.
> - [x] **REVERSED (vello-direction, text-led): hardware linear blending
>   produced systematically wrong text vs the vello reference (fringe 0.5
>   lands 0.735 through linear vs 0.5 encoded — a polarity-dependent
>   weight shift no gamma table can fix), and vello_cpu pins
>   premultiplied-sRGB compositing with no transfer handling anywhere.
>   Persistent target is now plain `Rgba8Unorm`; all shaders composite
>   sRGB-encoded values as-is; the blit is a raw copy (+premultiply, as
>   in vello's BlitPipeline) with a decode only for sRGB-swapchain
>   fallback. Readback now matches swapchain bytes by construction.
>   Earlier "looks good" covered shapes at low scrutiny; border_lab,
>   gradient_lab, shadow_lab, and image_lab all need one human re-pass
>   under the new composite before this bundle re-closes.

### Why

Blending in sRGB space darkens edges and thins text. Linear-space blending is physically correct and matches how macOS/Windows composite.

### Algorithm

```
// In fragment shader:
let srgb_sample = textureSample(atlas, sampler, uv).r;
let linear = srgb_to_linear(srgb_sample);      // pow(c, 2.2) or exact sRGB EOTF
let blended = linear * vertex_alpha;            // multiply in linear
let output = linear_to_srgb(blended);           // pow(c, 1/2.2) or exact sRGB EOTF
```

Use the exact sRGB transfer function (not simple gamma 2.2):
```
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { pow((c + 0.055) / 1.055, 2.4) }
}
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * pow(c, 1.0/2.4) - 0.055 }
}
```

### Caching

No cache changes. The persistent target format stays `Rgba8Unorm` (linear values stored as if sRGB, but the blit shader already handles the EOTF decode). The key change is that blending math in all fragment shaders operates on linear-decoded values.

### Intended use

All fragment shaders: `rect.wgsl`, `rounded.wgsl`, `glyph.wgsl`. The `solid.wgsl` blit shader already handles the sRGB encode for the swapchain.

---

## 3. Gradient Fills

> **Progress — Bundle 2 (with §5 dithering)**
> - [x] Plan claim "steps 1-5 don't alter the vertex format" is **wrong**
>   for gradients: direction + 2 stop positions exceed the 2 free floats
>   in `params`. Resolution: Bundle 1's `RRectVertex` restructure reserves
>   `grad = (dir.x, dir.y, stop0_t, stop1_t)` now (zeroed), plus a mode
>   flag in `grad.w` — Bundle 2 then touches only shader + emitter.
> - [x] 2-stop linear only (covers ~95% of UI); N-stop and radial keep the
>   existing `warn_once` → vello fallback. Colors reuse the `fill`/`stroke`
>   vec4s when the gradient mode flag is set.
> - [x] Oklab interpolation + Bayer 4×4 dither (§5) fold into the **same**
>   shader edit — one human review for both.
> - [x] Implementation notes: mode flag is `grad.xy != 0` (solid fills
>   emit an exact-zero axis — no extra storage). Colors ride **Oklab +
>   alpha** in the fill/stroke vec4s (one inverse conversion per pixel on
>   the GPU; CPU does the forward transform once per quad). Stop
>   positions are CSS-parameterized: `start + (end-start)*p` per stop,
>   both projected onto the axis from the outer-box origin. Degenerate
>   axis or collapsed stop span paints solid stop-1 (CSS parity).
>   Gradient + border emits a **second vertex** for the border ring
>   (the stroke slot carries stop 1) — painter's order, same run.
> - [x] Emitter: project `LinearGradient.start/end` onto quad axis →
>   `dir`, `stop0_t`, `stop1_t`.
> - [x] Shader: Oklab inverse, mode via nonzero axis, Bayer 4×4 dither
>   applied in encoded space then decoded for linear compositing.
> - [x] Demo: new `gradient_lab` example (7 rows incl. banding ramp and
>   radial-fallback) — registered, `cargo check --examples` green.
> - [x] Human validation (Bundle 2): **"works well, continue"** — banding
>   ramp clean, Oklab stops as predicted, radial-fallback row falls back.

### Why

UI uses gradients for buttons, backgrounds, and accent surfaces. Currently gradients are skipped entirely.

### Algorithm — Linear Gradient with Oklab Interpolation

```
// Vertex data:
//   gradient_direction: vec2<f32>  (unit vector)
//   stop0_t, stop1_t: f32          (0..1 positions)
//   color0_rgba, color1_rgba: vec4<f32>  (sRGB-encoded)

// In fragment shader:
let p = in.pos - quad_origin;
let t = dot(p, gradient_direction);             // project onto gradient axis
let t_norm = clamp((t - stop0_t) / (stop1_t - stop0_t), 0.0, 1.0);

// Oklab interpolation (perceptually uniform):
let lab0 = srgb_to_oklab(color0);
let lab1 = srgb_to_oklab(color1);
let lab = mix(lab0, lab1, t_norm);
let color = oklab_to_srgb(lab);
```

Oklab conversion matrices (from Björn Ottosson):
```
// sRGB -> Oklab
L = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
M = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
S = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b
l_ = pow(L, 1/3); m_ = pow(M, 1/3); s_ = pow(S, 1/3)
L = 0.2104542553*l_ + 0.7936177850*m_ - 0.0040720468*s_
a = 1.9779984951*l_ - 2.4285922050*m_ + 0.4505937099*s_
b = 0.0259040371*l_ + 0.7827717662*m_ - 0.8086757660*s_
```

### Dithering (same pass)

Add ordered Bayer 4x4 dither to the gradient output to prevent banding:
```
fn bayer_4x4(p: vec2<u32>) -> f32 {
    // Precomputed 4x4 Bayer matrix / 16
}
let dither = (bayer_4x4(vec2<u32>(floor(in.pos))) - 0.5) / 255.0;
color += dither;  // 1/255 amplitude, imperceptible but kills banding
```

### Caching

No new cache. Gradient parameters ride in `RRectVertex` fields, cached by `ChunkCache`.

### Intended use

`SceneCommand::DrawRect` with gradient fill. Two-stop linear covers ~95% of UI use. Radial/sweep gradients fall back to the vello engine.

---

## 4. Erf-Based Shadows

> **Progress — Bundle 3**
> - [x] Fidelity decision: **no flattening**. CSS box-shadow is one shadow
>   per layer, each with its own offset/blur/spread, composited with
>   normal source-over. Implemented as **one quad run per layer** (not a
>   shader loop): no layer cap, identical paint order — layers arrive
>   extent-descending (tighter over softer), exactly the order the vello
>   and tiny-skia renderers use. Sigma = `layer.softness` directly
>   (core already stores softness = CSS blur/2, see `Shadow::web_like`).
>   Spread expands the occluder box CPU-side; layer alpha =
>   `light.intensity * color.a * layer.opacity * node opacity` baked
>   into the vertex. Composited in linear space source-over.
> - [x] `ShadowShape::Alpha` stays Box-rasterized with a warn (matches
>   current core doc; offscreen alpha pass still deferred).
> - [x] `BlendMode::Multiply` approximated as Normal in v1, with its own
>   warn-once (CSS default is Normal; elevation's Multiply default gets
>   a premultiplied revisit later).
> - [x] `ShadowVertex` + `Op::Shadows` + pipeline + `emit_shadow`
>   (rigid-tx guard like rrects, `layer_pad`-derived quad padding,
>   threaded through Batch/ChunkCache/rebase/renderer upload+draw).
> - [x] Fragment shader: **verbatim port of gpui's analytic Gaussian**
>   (`gpui_wgpu/src/shaders.wgsl` fs_shadow / `blur_along_x` /
>   `gaussian` / `erf` — Evan Wallace's separable-convolution
>   technique): exact erf convolution along x at 4 Gaussian-weighted
>   row samples along y, rows clamped to +-3 sigma and to rows whose
>   profile can reach the point; hard-shadow branch `saturate(0.5 -
>   sd)`; Bayer dither on the final alpha (our addition). Quad padding
>   `>= 3*sigma` on the CPU matches gpui's vertex margin. Replaced the
>   earlier SDF-distance-axis approximation, which the human reported
>   as rendering flat/uniform.
> - [x] `shadow_lab` restored to the documented two-card web reference
>   (removed four stale "TEMPORARY diagnostics" probe cards).
> - [x] Validation: `shadow_lab` on wgpu vs tiny-skia — **human
>   validated** (shadow_studio edge shows graded falloff; blur slider
>   tracks). Verbatim-gpui shader port confirmed working.

### Why

Shadows are everywhere in modern UI (cards, modals, dropdowns, tooltips). Resolution-independent, no offscreen passes needed.

### Algorithm — SDF + 4-Tap Gaussian Integral

```
// Compute signed distance to rounded rect
let dist = quad_sdf(p, bounds, corner_radii);

// For blur_radius == 0 (hard shadow):
alpha = saturate(0.5 - dist);

// For blur_radius > 0 (soft shadow):
// Approximate the 2D Gaussian integral along scanlines
let step_size = blur_radius * 2.0 / 4.0;
var alpha = 0.0;
for (var i = 0; i < 4; i++) {
    let offset = -blur_radius + step_size * (f32(i) + 0.5);
    let scanline_dist = dist + offset;  // approximate
    let sample_alpha = saturate(0.5 - scanline_dist);
    let weight = gaussian(offset, blur_radius);
    alpha += sample_alpha * weight * step_size;
}

// Shadow color is typically black with opacity
final_color = shadow_color * alpha;
```

Gaussian weight:
```
fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-0.5 * (x * x) / (sigma * sigma)) / (sigma * sqrt(2.0 * PI));
}
```

Inset shadows: `alpha = 1.0 - alpha`, clipped to element bounds.

### Caching

No new cache. Shadow parameters in a new `ShadowVertex` struct, cached by `ChunkCache`.

### Intended use

`SceneCommand::DrawShadow` with bounds, corner radii, blur radius, spread, offset, and color. Resolution-independent (no bitmap cache). Supports both drop and inset shadows.

---

## 5. Mipmapped Images

> **Progress — Bundle 4**
> - [x] Ordering: last of the render features (new cache + pipeline +
>   shader + bind group; biggest surface area, no dependencies from it).
> - [x] `hash_commands` already hashes `DrawImage` by allocation identity —
>   chunk cache correctness comes free.
> - [x] `src/image.rs`: GPU `ImageCache` keyed by allocation identity
>   (Arc ptr), 64-entry / 32 MiB LRU, per-texture bind group on the
>   glyph-layout shape (wgpu structural layout compatibility), full CPU
>   box-filter mip chain uploaded per level (`Rgba8UnormSrgb`, hardware
>   decode to linear). `Arc<ImageGpu>` refcounted into batches and
>   `CachedChunk`s so replay survives map eviction; source bytes retained
>   to keep allocation keys unique (tiny-skia guard).
> - [x] `ImageVertex` (pos/uv/color) + textured pipeline
>   (`PREMULTIPLIED_ALPHA_BLENDING`; hardware mip selection via
>   `textureSample` derivatives) + `Op::Images` with per-quad bind index;
>   rebase/append/snapshot plumbing mirrors shadows.
> - [x] `src/svg.rs`: resvg raster cache (tree-ptr + device-size key,
>   128-entry / 16 MiB LRU) feeding the same GPU image path — one code
>   path to the screen for raster and SVG.
> - [x] Demo: `examples/image_lab` — minification ramp (256→16 px
>   high-frequency source), premultiplied-alpha blob over light/dark
>   cards, inline SVG at 64/32/16; static page exercises chunk-cache
>   image replay on every frame after the first.
> - [x] Validation: `image_lab` on wgpu — **human "good enough"** after
>   the premultiplied-sRGB fix (grayish fade on white root-caused to
>   hardware decode of premultiplied channels; plain `Rgba8Unorm` +
>   shader un/re-premultiply). Bundle 4 closed provisionally; row-2
>   re-check was not re-run separately.

### Why

Thumbnails and icons at 4K shimmer without mipmaps. Images are a prerequisite for a complete UI renderer.

### Algorithm — Textured Quad with Mip Level Selection

```
// Vertex data:
//   uv: vec2<f32>          (atlas UV for level 0)
//   texture_index: u32     (which atlas page)
//   output_size: vec2<f32> (screen-space size of the quad)

// In fragment shader:
let mip_level = log2(max(dFdx(uv.x) * atlas_size.x, dFdy(uv.y) * atlas_size.y));
let color = textureSampleLevel(image_atlas, image_sampler, uv, mip_level);
```

### Caching — ImageCache

```rust
struct ImageCache {
    // Content-addressed by image data hash
    map: FxHashMap<u64, ImageEntry>,
    // Atlas pages (RGBA8, mipmapped)
    pages: Vec<AtlasPage>,
    // LRU eviction
    frame: u64,
}

struct ImageEntry {
    page_index: u32,
    uv_rect: [f32; 4],
    mip_levels: u8,  // number of mip levels generated
    last_used: u64,
}
```

- Small images (≤256²): atlas-packed into shared pages, no mipmaps
- Large images (>256²): dedicated texture with full mipmap chain
- Eviction: LRU, drop images unused for 120 frames (same policy as glyph atlas)

### Intended use

`SceneCommand::DrawImage` with image handle, target bounds, and optional corner radii. Sub-tile extraction for rendering a visible portion of a larger image. Zero-copy video texture import is a separate future feature (NV12/P010 decode → GPU texture).

---

## 6. Subpixel Bin Tuning

> **Progress — Bundle 5 (redefined by the human report)**
> - [x] Kept as its own bundle despite being small: the human A/B (4×1 vs
>   4×4 at 8–14px during smooth scroll) is a distinct observation that
>   shouldn't share a review with a visual feature.
> - [x] Bin parameterization (4×1 default + `RIODE_SUBPIXEL_BINS` env)
>   was implemented, then **superseded** — bin *count* was never the root
>   cause (see below); the knob is removed.
> - [x] Human report on `text_smooth wgpu`: transitions worse than
>   tiny-skia/vello; glyphs "get taller or shorter by 1px or 2" between
>   subpixel positions; screenshot showed a deformed "P" top. Investigation
>   (git archaeology + path trace): `4fdff23c` had replaced the
>   human-validated axis-split policy with 4×4 binning, deleting the very
>   comment that predicted this ("every Y phase change re-fits the outline
>   (morphing/popping per glyph)"). Deeper cause, independent of bin count:
>   **wgpu rasterized a fresh hinted swash bitmap per phase** (the offset
>   is applied *before* hinting → shape refit on every bin swap → bbox
>   height churn = the 1–2px growth) **and glided Y through fractional
>   bilinear filtering of hinted stems**. References are shape-invariant:
>   tiny-skia integrates analytic coverage at each quarter quantum from ONE
>   hinted outline; vello is vector.
> - [x] Fix: wgpu text now calls `raster_glyph_exact_scaled` (new scaled
>   sibling of the shared analytic path in `text/context.rs`; unscaled
>   keys stay byte-identical, so tiny-skia slots and behavior are
>   untouched), both axes quantize via `quantize_quarter` on the device
>   pen (reference-identical wrap semantics), placement integer-anchors at
>   `bx+left / by-top` (reference-identical blit math), atlas slots keyed
>   (key, qx, qy, scale). Compile green: default + `--no-default-features`,
>   0 errors.
> - [ ] Human validation of the fix: P top, all four rows, F7 against
>   tiny-skia/vello — **awaiting**.

### Why

16 bins (4x4) uses 4x the atlas memory of 4 bins (4x1). The analysis suggests 4x1 is sufficient for most UI text since vertical subpixel positioning matters less for horizontal reading.

### Algorithm — Configurable Phase Encoding

```
// Current: 4x4 = 16 bins (bits 0-1 = X, bits 2-3 = Y)
// Proposed: 4x1 = 4 bins (bits 0-1 = X only)

// In command.rs:
const SUBPIXEL_BINS_X: u8 = 4;   // configurable
const SUBPIXEL_BINS_Y: u8 = 1;   // start with 1, upgrade to 4 if needed

let fx = placed.x - placed.x.floor();
let binx = (((fx * SUBPIXEL_BINS_X as f32) + 0.5).floor() as i8).clamp(0, SUBPIXEL_BINS_X - 1);
let phase = if SUBPIXEL_BINS_Y > 1 {
    let fy = placed.y - placed.y.floor();
    let biny = (((fy * SUBPIXEL_BINS_Y as f32) + 0.5).floor() as i8).clamp(0, SUBPIXEL_BINS_Y - 1);
    binx | (biny << 2)
} else {
    binx
};
```

### Caching

`AtlasSlot` key already includes `phase`. Reducing bins from 16 to 4 means fewer distinct entries, reducing atlas memory by ~4x for the phase dimension.

### Intended use

Human validation at small font sizes (8-14px) comparing 4x1 vs 4x4. If no visible difference, keep 4x1 and save atlas memory. If vertical shimmer is visible during smooth scroll, upgrade to 8x1.

---

## Priority and Dependencies

> **Progress — bundling decided**
> - [x] Bundle 1 (**in progress**): §1 per-edge borders + §2 linear
>   blending — both touch the same shaders/pipelines, one demo, one
>   human review.
> - [x] Bundle 2: §3 gradients + §5-dither (same shader edit).
> - [x] Bundle 3: §4 shadows (CSS-like multi-layer).
> - [x] Bundle 4: §5 images.
> - [x] Bundle 5: §6 subpixel tuning (small, standalone A/B).
> - [x] Per AGENTS.md: each bundle lands only on explicit human approval;
>   progress is checked off here as it lands.

```
1. Borders (shader-only, extends rounded.wgsl)
2. Linear blending (math change in all shaders)
3. Gradients (new shader logic in rounded.wgsl)
4. Shadows (new vertex type + new shader)
5. Dithering (add to gradient + shadow shaders)
6. Images (new cache + new pipeline + new shader)
7. Subpixel tuning (parameterize command.rs)
```

Steps 1-5 are shader-only changes that don't alter the vertex format or caching architecture. Step 6 adds a new cache. Step 7 is a tuning change.

---

## Validation Per Feature

Each feature follows the visual bug protocol:
1. Minimal demo in `examples/` isolating the feature
2. Human screenshot comparison against reference (browser, gpui, or vello engine)
3. Predictions stated before human observation
4. Commit only on explicit human approval

---

## Appendix A: wgpu vs Vello — Algorithm Comparison

### Architecture

| Aspect | wgpu engine | Vello engine |
|--------|-------------|--------------|
| GPU abstraction | Raw wgpu (manual pipelines, vertex buffers) | Vello scene API (compute shader pipeline) |
| Rendering model | CPU tessellation → instanced draw calls | CPU encoding → compute shaders (tessellate, bin, shade) |
| Scene representation | Batched vertex buffers (`RectVertex`, `RRectVertex`, `GlyphVertex`) | `VelloScene` with `fill`/`stroke`/`draw_glyphs`/`draw_image` |
| Antialiasing | Analytic SDF in fragment shaders | `AaConfig::Area` — analytic coverage on all primitives |
| Compositing | Painter's order, alpha blend in shader | Scene graph with blend modes, alpha groups |
| Damage tracking | Persistent target + base-quad repaint on partial damage | Full scene re-render each frame |
| Chunk cache | Content-addressed, origin-rebased replay for scroll | Content-addressed fragment reuse, affine rebasing |

### Text Rendering

| Aspect | wgpu engine | Vello engine |
|--------|-------------|--------------|
| Glyph rasterization | Pre-rasterized to bitmap via analytic rasterizer, uploaded to atlas | Vector outlines rasterized on-the-fly by GPU |
| Atlas format | R8 (1 byte/px), shelf-packed, 2048x2048 | Internal bump allocator |
| Subpixel positioning | 16 pre-rasterized quarter-pixel bins (4x4), placement stays exact | Exact — vectors at any fractional position |
| Text AA | Linear-filtered bitmap coverage | `AaConfig::Area` exact analytic coverage |
| Variable fonts | Limited (subpixel bins) | Native via `normalized_coords` (F2Dot14) |
| Gamma handling | Gamma LUT in shader for display type (>16px) | N/A (vector rendering) |

### Primitive Support

| Primitive | wgpu engine | Vello engine |
|-----------|-------------|--------------|
| Solid rects | `rect.wgsl` pipeline (alpha blend) | `scene.fill()` with KurboRect |
| Rounded rects | SDF in `rounded.wgsl` | Native Kurbo paths |
| Borders/strokes | SDF ring in `rounded.wgsl` (rounded only) | Native Kurbo stroke, any transform |
| Gradients | Not supported (plan: linear, Oklab) | Native linear/radial/sweep via Peniko |
| Shadows | Not supported (plan: SDF + Gaussian) | 9-patch cache with box blur |
| Images | Not supported (plan: atlas + mipmaps) | Native `draw_image` with PenikoImage |
| SVG | Not supported | Full via `resvg` → Vello scene |
| Layers | Not supported (draw inline) | `push_layer`/`pop_layer` with blend modes |
| Arbitrary paths | Not supported | Native Kurbo BezPath |

### Performance Characteristics

| Metric | wgpu engine | Vello engine |
|--------|-------------|--------------|
| Draw calls | One instanced draw per batch | Compute shader dispatches (tessellate, bin, shade) |
| CPU cost | Tessellation + vertex emission | Scene encoding |
| GPU cost | Vertex fetch + fragment shading | Compute-heavy (prefix-sum, binning) |
| Scroll fast path | Origin-rebased vertex replay | Affine-rebased fragment reuse |
| Idle cost | Zero (no redraw without damage) | Full re-render each frame |
| 4K text-heavy | Optimal (bitmap blit, cached vertices) | Slower (re-tessellates paths) |
| 4K complex vector | N/A (no path support) | Optimal (compute scales with GPU) |

### Memory Characteristics

| Resource | wgpu engine | Vello engine |
|----------|-------------|--------------|
| Glyph atlas | R8, 2048x2048, 1 byte/px, LRU eviction | Internal bump allocator |
| Vertex cache | Content-addressed chunk cache | Scene fragment cache |
| Persistent target | Rgba8Unorm (full screen) | Rgba8Unorm (full screen) |
| Offscreen layers | None (no layer support) | Allocated per `push_layer` |

### When to Use Which

| Workload | Best engine | Why |
|----------|-------------|-----|
| Text-heavy UI at 4K | wgpu | Bitmap blit is cheaper than re-tessellating outlines |
| Scrolling content | wgpu | Origin-rebased vertex replay, damage tracking |
| Many small windows | wgpu | Lower per-window overhead, shared device |
| Animated/rotated text | Vello | Vector rasterization at any transform |
| Charts, graphs, vector art | Vello | Native path rendering, analytic AA |
| Large SVGs | Vello | Full vector support via resvg |
| Gradients, blend modes | Vello | Native Peniko brush support |
| Video overlays | wgpu | Damage tracking prevents full re-render |

---

## Appendix B: gpui Reference Analysis

The following is based on a full clone of the gpui repository (`zed-industries/zed`, `crates/gpui/` and `crates/gpui_wgpu/`).

### Scene Primitives (8 variants)

```rust
pub enum Primitive {
    Shadow(Shadow),           // Drop/inset shadows with Gaussian blur
    Quad(Quad),               // Rounded rect with background, border, corner radii
    Path<Path<ScaledPixels>>, // Vector paths (triangulated curves with SDF fill)
    Underline(Underline),     // Underlines (solid or wavy)
    MonochromeSprite(MonochromeSprite),   // Single-color text/SVG icons
    SubpixelSprite(SubpixelSprite),       // Subpixel-antialiased text
    PolychromeSprite(PolychromeSprite),   // Full-color images/emoji
    Surface(PaintSurface),    // macOS/iOS video surfaces (CVPixelBuffer)
}
```

### Text Rendering

- **Shaping**: cosmic_text (grapheme clustering, bidi, font fallback)
- **Rasterization**: swash with hinting enabled
- **Atlas kinds**: Monochrome (R8Unorm), Subpixel (color, 4 X-variants), Polychrome (color, emoji)
- **Subpixel mode**: 4 X-variants, dual-source blending for LCD
- **AA**: Linear-filtered bitmap coverage + gamma/contrast correction (adapted from Microsoft Terminal)

### Rounded Rect AA

```wgsl
fn quad_sdf(point: vec2<f32>, bounds: Bounds, corner_radii: Corners) -> f32 {
    let half_size = bounds.size / 2.0;
    let center = bounds.origin + half_size;
    let center_to_point = point - center;
    let corner_radius = pick_corner_radius(center_to_point, corner_radii);
    let corner_to_point = abs(center_to_point) - half_size;
    let corner_center_to_point = corner_to_point + corner_radius;
    return quad_sdf_impl(corner_center_to_point, corner_radius);
}
```

### Shadow Algorithm

SDF + 4-tap Gaussian blur integral along scanlines with erf approximation. Supports blur radius, spread, offset, per-corner radii, and inset shadows.

### Border Algorithm

SDF double-wall: compute `outer_sdf` and `inner_sdf` (offset by border width), blend between them. Supports solid and dashed borders with per-edge widths and dash layout around rounded corners.

### Gradient Algorithm

Linear only (2 stops), sRGB or Oklab interpolation. No radial gradients. Pattern backgrounds (slash stripes, checkerboard) also supported.

### Image Handling

`PolychromeSprite` in color atlas with SDF corner clipping. Sub-tile extraction for rendering visible portions. `etagere` bucketed allocation.

### SVG Handling

Pre-rasterized via usvg/resvg at 2x scale (`SMOOTH_SVG_SCALE_FACTOR = 2.0`), stored as monochrome or polychrome sprite. Not vector-rendered.

### Key Takeaways for Our wgpu Engine

1. gpui uses **analytic SDF for quads** (not bitmap) — our `rounded.wgsl` already does this
2. gpui uses **bitmap atlas for text** — our R8 coverage atlas matches this approach
3. gpui **pre-rasterizes SVG** to bitmaps — we should do the same, not use vector SVG
4. gpui has **no radial gradients** — linear-only is sufficient for UI
5. gpui's **shadows are shader-based** (no offscreen passes) — our plan matches this
6. gpui's **borders are SDF double-wall** — our plan matches this
7. gpui uses **Oklab for gradient interpolation** — perceptually uniform, better than sRGB
8. gpui's **damage tracking** is essential for 4K — we already have this, Vello doesn't
