# Missing Features vs HTML/CSS (tiny-skia reference)

riode's tiny-skia backend (`crates/engines/riode-engine-tiny-skia`) is the reference
CPU rasterizer. This document lists the HTML/CSS **concepts** it does not implement,
organized by impact on web/Figma fidelity. Paths are relative to
`crates/core/riode/src/foundation/` and `crates/engines/riode-engine-tiny-skia/src/`.

riode is not a CSS engine: there is no DOM or stylesheet parser. "CSS feature" here
means a visual concept that has a Rust API and is consumed by the rasterizer.

---

## Borders & Strokes

- **high** — Per-edge border color — `Stroke` has a single `fill`, so all four sides share one color.
- **high** — Border dash / dotted / dashed — `Stroke` is solid-only with no dash pattern model.
- **high** — Border dash + join/cap — extend `Stroke` with dash patterns, miter/round/bevel joins, and butt/round/square caps.
- **high** — Stroke line join + cap — no join or cap control on `Stroke`, so corners and line ends are always square.
- **high** — Variable stroke alignment per edge — `StrokeAlign` is global to the element, not configurable per side.
- **high** — Per-edge border width — stored in `Stroke::per_edge` but tiny-skia rasterizes uniform-max instead of per-side.

## Fills & Gradients

- **low** — Conic gradient — `Fill` only supports Solid, Linear, and Radial with no conic variant.
- **low** — Conic gradient shader — add `Fill::ConicGradient` plus a sweep shader in `shader.rs`.
- **medium** — Image fill + scale mode — `DrawImage` blits stretched to the dest rect with no fill/contain/cover/tile negotiation.
- **medium** — object-fit + image fill — add contain/cover/fill negotiation in `image.rs` and a background-image layer.
- **moonshot** — Multiple backgrounds — `Appearance` has a single `fill` field, so one element can't stack multiple background layers.
- **low** — background-size / position / repeat — no background layer exists; gradients are element-local with no sizing or repeat semantics.
- **low** — Gradient color-space interpolation control — user-selectable RGB vs Oklab vs HSL interpolation for perceptually-correct gradients.
- **medium** — Color spaces (HSL/OKLCH/P3) — `Color` is RGBA8 only with no wide-gamut or perceptual color space support.
- **medium** — OKLCH color space — perceptually-uniform color space for design work, accessible contrast, and modern color picking.

## Shadows & Effects

- **high** — ~~Inset box-shadow~~ — DONE: `Shadow::inset()` / `.inset_if()` + `inset` literal keyword; tiny-skia inverts interior coverage, wgpu masks in the shadow shader. CSS parity (positive offset_y darkens the top).
- **high** — drop-shadow filter / true Alpha silhouette — IMPLEMENTED, AWAITING HUMAN EYES: Alpha shadows force a layer; engines stash `DrawShadow`, cast silhouette→blur→tint→offset onto the parent behind the content (`BeginLayer.shadow_pad`, `shadow_lab` circle row; inset/no-offscreen fall back to Box; spread ignored). `filter: drop-shadow()` literal not added.
- **high** — Layer blur / background blur — ~~layer blur~~ DONE via the filter chain (`Filter::blur`, both engines; wgpu blur visually OPEN, TEMP(diag) markers armed — magenta composite tint, on-change sigma prints, committed in `1e674cbf`); background blur (sampling behind the element) still open, see Backdrop filters.
- **high** — CSS element filters — DONE: blur/saturate/brightness/contrast/hue-rotate, combined one-blur-one-matrix chain, `SceneCommand::BeginLayer` filter arg, `filter_lab` demo. tiny-skia validated by eye; wgpu color filters validated, wgpu blur under investigation.
- **high** — Backdrop filters — `backdrop-filter: blur()` for frosted-glass effects that sample what's behind the element.
- **high** — Layer effects stack — multiple independent effects (shadows, blurs, strokes) per layer with per-effect blend modes and opacity.
- **low** — Masks (alpha mask / outline mask) — only rectangular clips exist; no shape-based masking.

## Blending & Compositing

- **low** — mix-blend-mode (full set) — `BlendMode` has only Normal + Multiply, used only for shadows.
- **low** — Blend modes (extend to 15) — extend `BlendMode` to the 15 Figma modes and lift it from shadow-only to element compositing.

## Typography

- **medium** — tabular-nums / font-feature-settings — absent from `TextShapeParams` and the shape builder.
- **medium** — tabular-nums (Parley) — wire Parley `FontSettings::feature("tnum")` in `shape.rs` for numeric column alignment.
- **high** — text-transform — content is shaped as-is with no uppercase/lowercase/capitalize.
- **very low** — text-shadow — shadows are element-box only via `Appearance::shadow`, with no per-glyph shadow.
- **medium** — OpenType features at build time — compile-time selection of OpenType features (liga, tnum, kern, ss01) baked into the font pipeline.
- **medium** — Variable font optical size (auto) — automatic optical size adjustment for variable fonts based on rendered size.

## Layout

- **moonshot** — CSS Grid — layout is flex-only; `<AutoGrid>` is responsive chunking, not grid templates with templates/placement.
- **moonshot** — 3D transform / perspective — transforms are strictly 2×3 affine with no z, perspective, or rotateX/Y.

## Clipping & Transforms

- **moonshot** — clip-path (polygon/path) — `PushClip` supports rect and rounded-rect only, with no polygon or arbitrary path clipping.

## Images

- **medium** — WebP/AVIF image support — modern image formats with superior compression and quality for lossy/transparency and high-quality compression.

## Advanced Rendering

- **moonshot** — Live shaders — user-authored shaders on canvas for procedural visuals, generative art, and GPU-accelerated effects.
- **medium** — Squircle / continuous corners — iOS-style smooth corners beyond circular radius for a modern, native feel.

---

## Prioritized Backlog

### High priority
- **Cursor styles** — engine-level API for pointer/grab/text/etc (currently only
  exists in docs CSS, not in the renderer)

### Medium priority
- **Focus ring / focus-visible styling** — visible focus indicator for accessibility
- **Keyboard navigation + roving tabindex** — arrow-key navigation between focusable
  elements within a container
- **Hyphenation** — automatic line-break hyphenation for justified/narrow text
- **Hanging punctuation** — punctuation marks hang outside the text block edge
- **text-wrap: balance** — balanced line lengths for headlines and short blocks
  (note: existing `balance` is a line-breaking policy, not CSS balancing)
- **CSS color functions** — `oklch()`, `oklab()`, `hsl()`, `lab()` support
- **content-visibility** — skip rendering offscreen content for performance

### Low priority
- **Font synthesis** — synthetic bold/italic when not available in the font
- **color-mix() + relative color syntax** — programmatic color derivation
- **forced-colors / prefers-contrast** — high-contrast mode support
- **Focus trap** — keep focus inside modals/dialogs

### Very low priority
- **position: sticky** — stick element within scroll container
- **Scroll-snap** — snap scroll positions to elements
- **touch-action** — control touch gesture behavior per element
- **overscroll-behavior** — control scroll chaining and bounce
- **Pinch-zoom** — two-finger zoom gesture
- **contain (layout/paint)** — CSS containment hints
- **Scroll anchoring** — maintain scroll position when content above changes

### Not needed
- Multi-column layout
- Masonry
- print-color-adjust
- Smooth scroll behavior
- Focus-visible ring (covered by focus ring)
