# riode — Layout Engine

Declarative, retained-mode UI engine: reactive signals drive a flexbox layout tree,
a seven-stage frame pipeline (invalidate → layout → damage → compile → raster →
present) turns it into pixels, and interchangeable backends paint them.

## Layout

| Directory      | Responsibility |
|----------------|----------------|
| `src/anim/`    | Time-based interpolation: clocks, easing, springs, tweens, transitions |
| `src/app/`     | Application runner: window, event loop, debug overlays, telemetry |
| `src/dsl/`     | Declarative builders (`group`, `text`, `image`, `svg`) + reconciler + FLIP |
| `src/foundation/` | Shared vocabulary value types (color, geometry, constraints, shadows, …) |
| `src/input/`   | Pointer/keyboard dispatch, gestures, text selection |
| `src/layout/`  | Flexbox layout engine |
| `src/pipeline/`| Frame passes: damage analysis, display-list compile, bounds commit |
| `src/reactive/`| Signal graph: signals, derivations, effects, tickers, hooks |
| `src/render/`  | Backend selector + shared raster helpers (blur, swizzle) |
| `src/runtime/` | Engine orchestration: scheduler, frame passes, diagnostics |
| `src/scene/`   | Backend-agnostic display list (chunks + commands) |
| `src/text/`    | Typography: shaping (Parley), fonts, glyph cache, selection queries |
| `src/tree/`    | Retained arena: nodes, styles, invalidation, per-domain state groups |
| `src/prelude.rs` | Curated authoring surface (`use riode::prelude::*`) |

`NodeState` is grouped by owning domain (`Core/Input/Text/Scene/PaintState`) so each
pass touches its own slice. Cross-module writes go through `TreeArena` directly for
now; per-domain stores are the planned seam for any future split.

`examples/` holds runnable demos (see each file's header); `tests/` holds the
stage-by-stage TDD suites plus integration harnesses (driven by
`riode-tiny-skia::testing::Harness`).

## Dependencies

Stands alone except windowing (`winit`), shaping (`parley`, `swash`), SVG (`resvg`),
and raster helpers (`tiny-skia` for debug capture). Rendering backends
(`riode-tiny-skia`, `riode-vello`) and presentation (`riode-wayland`) are separate
crates that depend on this one — never the reverse.

## Features

- `os-clipboard` (default): push Ctrl+C selections to the OS clipboard in real windows.
- `tracy`: Tracy profiling integration.
