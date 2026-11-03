# Riode Architectural Overview & Execution Lifecycle

## 1. Workspace Topography
- `crates/core/riode/`: Layout, reactivity runtime, tree arena, scene graph, RSX macros. Strictly 0 lines of WGSL, zero wgpu references.
- `crates/core/riode-shader/`: Single source of truth for all mathematical WGSL code, SDF shapes, materials, and shader composition.
- `crates/engines/riode-engine-wgpu/`: Raw wgpu renderer, texture caches, dynamic uniform streaming, and material/effect pipeline execution.

## 2. Frame Execution Lifecycle
Every rendered frame advances through four discrete phases:
1. **Phase 1 (Reactivity & Reconcile)**: Signals write new values; downstream reactive computations and observers are marked dirty. Reconcilation diffs group props and updates retained node styles.
2. **Phase 2 (Layout & Geometry Settle)**: Measures tree bottom-up and solves flex allocations top-down. `RectSettle` glides moving nodes and latches resting nodes to whole pixels to prevent resting jitter.
3. **Phase 3 (Scene Compilation)**: Walks arena in stacking-context order, emitting abstract `SceneCommand`s (`BeginLayer`, `EndLayer`, `DrawText`, etc.) into `SceneChunk`s. Clean subtrees reuse cached chunks verbatim ($O(1)$).
4. **Phase 4 (Engine Batching & GPU Dispatch)**: Lowering engine translates commands into batches and ops (`Op::BeginLayer`, `Op::EndLayer`, `Op::Glyphs`). Compiles unified WGSL shaders, streams 256-byte aligned dynamic uniforms, and dispatches draws to WebGPU.
