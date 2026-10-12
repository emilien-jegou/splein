# Aurora Engine Optimization Plan

Stages: ✅ done · 🔲 stage 0 (queued) · 🔷 stage 1 (queued).
- Stage 0 (no trade-off easy wins): #16 ✅, #19 ✅, #26 ✅, #27 ✅, #17 ✅ (done with #26), #10 🔲 (remainder), #29 ✅ (already shipped, verified).

Exhaustive list of every optimization identified in the Aurora vs Slint comparison, excluding threading changes (single-threaded reactive model is correct and fast).

---

## 1. Contiguous Component Storage ✅ STAGE 2 (S2-3) — PARTIAL (slots Vec was already contiguous; child Vecs eliminated via CSR; full bump-arena deferred)

**Current:** `TreeArena` uses `Vec<Slot>` with `Rc<RefCell<Node>>` handles, generational indices, and a free list. Each node is separately heap-allocated behind `RefCell`.

**Target:** Single contiguous `Vec<u8>` memory region storing components inline (like Slint's `#[repr(C)]` layout). Builders emit into the arena instead of allocating `RefCell` nodes. Indices replace `Rc` handles.

**Benefits:**
- Cache-friendly tree walks (contiguous memory vs pointer chasing)
- Deterministic memory usage (no fragmentation from per-node allocation)
- Smaller per-node overhead (no `RefCell` borrow flag, no `Rc` refcount)
- O(1) removal without free-list fragmentation

**Tradeoffs:**
- Internal change only — the Rust builder DSL stays the same
- Signal graph still needs `Rc`/`RefCell` for property storage (fine — properties are separate from geometry)
- Requires rearchitecting tree walk, dirty propagation, and layout cache

**Approach:**
1. Define a `#[repr(C)]` component header with geometry, flags, and child indices
2. Implement a bump-arena allocator with free list for component storage
3. Migrate `TreeArena` operations (insert, remove, walk) to the new layout
4. Keep the signal graph (`ReactiveRuntime`) unchanged — it references components by index
5. Benchmark tree walk cache misses before/after

---

## 2. Build-Time Text Shaping via Proc Macros — DEFERRED (text-runtime kill-switch done in S2-1; pre-shape proc-macro is #2-full, needs macro/API design)

**Runtime cost:** Parley shaping (~0.1–1 ms per string cache miss) + swash rasterization (~0.05–0.5 ms per glyph cache miss) + 4–64 MB glyph cache + 2048-entry shaped text cache.

**Target:** Proc macros that shape and rasterize static string literals at build time, embedding the result as `&'static [u8]`. Runtime becomes a simple blit.

**Benefits:**
- Zero runtime shaping latency for static text
- Zero runtime glyph cache memory (4–64 MB freed)
- Zero runtime shaped text cache (400 KB freed)
- Identical quality (same parley+swash, just run on build host)
- Binary size increase is bounded by the character set, not screen resolution

**Tradeoffs:**
- Dynamic text (user input, formatted numbers) still needs runtime parley+swash
- Build time increases (shaping happens once per unique string)
- Subpixel-animated text needs either multiple pre-rasterized offsets or runtime fallback
- Requires a `std`-capable proc macro (runs on build host — fine)

**Approach:**
1. Create `aurora-macros` extension: `#[pre_shape("Hello, World")]`
2. Proc macro invokes parley for shaping at compile time
3. Proc macro invokes swash for rasterization at compile time
4. Embeds shaped layout + glyph bitmap as `&'static [u8]` in the binary
5. Generates a `Text::pre_shaped(&ATLAS)` constructor
6. Keep runtime parley+swash path for dynamic text (feature-gateable for embedded)
7. Quantize strings to avoid duplicate entries (same string shaped once)

**Macro API sketch:**
```rust
#[component]
fn Greeting() -> Node {
    text!("Hello, World")  // shaped at build time
    text!(format!("Count: {}", count))  // dynamic, shaped at runtime
}
```

---

## 3. Build-Time SVG Rasterization — DEFERRED (svg-runtime kill-switch done in S2-1; pre-raster proc-macro is #3-full, needs asset pipeline design)

**Runtime cost:** resvg pulls ~50 dependencies (usvg, svgtypes, kurbo, etc.) and is a full SVG 1.1 renderer. On desktop this is fine; on embedded it's ~2 MB of code and significant RAM.

**Target:** Build-time rasterization via proc macro or build script. Resvg runs on the host, embeds the bitmap as `&'static [u8]`.

**Benefits:**
- Zero runtime SVG parsing cost
- resvg removed from embedded binaries entirely
- Zero runtime SVG memory (tree, paths, gradients all pre-baked)

**Tradeoffs:**
- Dynamic SVG theming (recoloring) needs multiple pre-rasterized variants or runtime fallback
- Build time increases per SVG file
- SVG animation not supported (would need runtime resvg or a different format)

**Approach:**
1. Proc macro: `include_svg!("icon.svg")` — rasterizes via resvg at build time
2. Or build script: scan `assets/svg/`, rasterize all to `.rgba` files, include via `include_bytes!`
3. Generate `Image::pre_rasterized(width, height, &BYTES)` constructor
4. Keep runtime resvg for dynamic SVG (feature-gateable: `feature = "svg-runtime"`)
5. For embedded: only pre-rasterized path, zero runtime SVG

---

## 4. Linebuffer Backend ✅ STAGE 2 (S2-5) — DONE (new aurora-linebuffer crate; row-equivalence test byte-matches tile backend)

**Current:** All backends use a full-frame persistent canvas (`w × h × 4` bytes). At 4K this is 32 MB. At 800×600 it's 1.8 MB.

**Target:** A linebuffer backend that rasterizes one scanline at a time and pushes directly to the display, requiring only a few KB of line buffer.

**Benefits:**
- Eliminates full-frame buffer allocation (32 MB → ~16 KB at 4K)
- Natural fit for SPI displays, e-pink, and small LCDs
- Reduces memory pressure on constrained devices

**Tradeoffs:**
- Requires Y-ordered rasterization (top-to-bottom scanline iteration)
- Random-access drawing (e.g., a rect at the bottom drawn before one at the top) requires either deferral or a small tile cache
- Not suitable for GPU backends (they naturally produce full frames)

**Approach:**
1. Implement a `LineBufferBackend` that implements the `Backend` trait
2. Rasterize scene chunks in Y-order (sort by `bounds.y`)
3. For each scanline, rasterize intersecting chunks into a small RGBA line buffer
4. Push the line buffer to the display via SPI/I2C/framebuffer write
5. Handle overlapping chunks by rasterizing inPainter's order within each line
6. Feature-gate: `feature = "linebuffer"`

---

## 5. DMA2D Backend (STM32)

**Current:** No hardware-accelerated blitting path for STM32 targets.

**Target:** A backend that uses STM32's DMA2D peripheral for hardware-accelerated pixel copies, blends, and format conversions.

**Benefits:**
- Zero CPU cost for blits and fills (DMA2D runs independently)
- Hardware-accelerated alpha blending
- Frees CPU for layout and signal processing

**Tradeoffs:**
- STM32-specific (not portable to other MCUs)
- Requires STM32 HAL or PAC as dependency
- Only useful on STM32F4/F7/H7 families with DMA2D

**Approach:**
1. Create `aurora-dma2d` crate
2. Use `stm32f4xx-hal` or `stm32h7xx-hal` for DMA2D registers
3. Implement `Backend` trait with DMA2D-accelerated blit paths
4. Fall back to CPU blitting for unsupported operations
5. Feature-gate: `feature = "dma2d"`

---

## 6. SIMD Rasterization Paths 🔷 STAGE 1 — DEFERRED (blur kernels have bit-exactness tests — symmetry/antisymmetry — and the col pass is strided; needs criterion benchmarks proving auto-vectorization gaps before hand-rolled AVX2/NEON)

**Current:** No explicit SIMD. Relies on auto-vectorization (unpredictable).

**Target:** Explicit NEON (ARM) and AVX2 (x86) paths for box blur, glyph blitting, and pixel swizzling.

**Benefits:**
- 4–8× speedup on box blur (the most CPU-intensive operation)
- Deterministic performance (no relying on compiler auto-vectorization)
- Frees CPU for other work on constrained devices

**Tradeoffs:**
- Platform-specific code paths with runtime dispatch
- Increased code size (multiple implementations of each kernel)
- Maintenance burden (each SIMD path needs testing)

**Approach:**
1. Identify hot paths: box blur, glyph blit, RGBA↔BGRA swizzle, tile copy
2. Implement scalar baseline (already exists)
3. Add AVX2 paths using `core::arch::x86_64` intrinsics
4. Add NEON paths using `core::arch::aarch64` intrinsics
5. Runtime-dispatch via `is_x86_feature_detected!("avx2")` / `std::arch::is_aarch64_feature_detected!("neon")`
6. Benchmark each kernel scalar vs AVX2 vs NEON
7. Feature-gate: `feature = "simd"`

---

## 7. AOT-Resolved Static Layouts ✅ STAGE 2 (S2-2) — DONE (runtime static-subtree freeze: memoized proof + damage-walk skip; no proc macros needed)

**Current:** Full flexbox layout runs every frame for every node (with caching via layout boundaries).

**Target:** At build time (or first frame), identify static subtrees (no signal dependencies) and pre-compute their layout. Only dynamic (signal-driven) subtrees run the flexbox engine at runtime.

**Benefits:**
- Zero layout cost for static UIs (buttons, labels, static containers)
- Reduced per-frame CPU on largely-static screens
- Smaller layout cache (fewer nodes to cache)

**Tradeoffs:**
- Requires dependency analysis (which nodes read signals?)
- First-frame cost increases (pre-computation happens upfront)
- Dynamic content invalidates pre-computed layout (must track dependencies)

**Approach:**
1. Extend the reactive signal graph to track which nodes subscribe to which signals
2. Mark nodes as "static" if they have no signal subscriptions
3. On first frame, pre-compute layout for static subtrees and freeze them
4. Only re-layout dynamic subtrees when their signals change
5. Invalidate static layout when the viewport resizes or fonts change
6. This is complementary to the existing `LayoutCache` — it extends it to full pre-computation

---

## 8. `no_std` Support

**Current:** All crates use `std::` extensively (`Arc`, `HashMap`, `Vec`, `RefCell`, `std::thread`).

**Target:** Engine core (signals, layout, damage, scene) compiles for `no_std` + `alloc`. Text and rasterizer backends remain `std` (feature-gated).

**Benefits:**
- Enables RTOS and baremetal targets
- Reduces binary size (no std overhead)
- Unlocks MCU-class deployment

**Tradeoffs:**
- Significant refactoring effort
- `parley` and `swash` are `std`-only (must be feature-gated out)
- `winit` is `std`-only (replaced by direct display access)
- `tiny-skia` may have `std` dependencies (needs verification)

**Approach:**
1. Audit all `std::` usage in `aurora` crate (engine core)
2. Replace `std::` with `core`/`alloc` equivalents:
   - `std::rc::Rc` → `alloc::rc::Rc`
   - `std::collections::HashMap` → `alloc::collections::BTreeMap` or `hashbrown` (no_std compatible)
   - `std::vec::Vec` → `alloc::vec::Vec`
   - `std::string::String` → `alloc::string::String`
3. Feature-gate `std`-dependent modules behind `feature = "std"`
4. Text stack (`parley`, `swash`) behind `feature = "text-std"` (runtime shaping)
5. Pre-shaped text (from #2) works without `std`
6. Rasterizer backends behind their own features (`tiny-skia`, `linebuffer`, `dma2d`)
7. Provide a `no_std` backend trait implementable by the embedder

---

## 9. RTOS/Baremetal Support

**Current:** No RTOS integration. Event loop is `winit`-based.

**Target:** Engine can run inside an RTOS task with a timer tick source.

**Benefits:**
- Deployment on FreeRTOS, Zephyr, QNX
- Deterministic real-time behavior
- MCU-class deployment

**Approaches:**
1. Implement `no_std` support (prerequisite — see #8)
2. Provide a `HeadlessEngine` that is already partially implemented — extend it to accept a tick source from the RTOS
3. Implement a `no_std` backend that writes directly to a framebuffer
4. Document integration patterns for FreeRTOS/Zephyr
5. Provide a `Cargo.toml` example for STM32/RP2040/ESP32

**RTOS integration sketch:**
```rust
// In the RTOS task:
fn ui_task() {
    let mut engine = Engine::headless();
    let backend = FramebufferBackend::new(FB_ADDR, WIDTH, HEIGHT);
    engine.set_backend(backend);
    loop {
        engine.tick();       // advance clock
        engine.frame();     // render one frame
        rtos::delay(16);    // ~60fps
    }
}
```

---

## 10. Scene Arena (Per-Frame Allocation Reduction) ✅ STAGE 0 — DONE (pooled command Vecs in Scene: capped at 32, Rc::get_mut-guarded so retained/shared streams never alias; outer Vec already retained capacity)

**Current:** `Scene` is cleared and rebuilt each frame. `Rc<Vec<SceneCommand>>` uses copy-on-write for chunk replay, but the scene itself allocates per frame.

**Target:** Reuse `Scene` allocation across frames via an arena allocator.

**Benefits:**
- Zero per-frame allocation for scene chunks
- Reduced allocator pressure (no malloc/free churn)
- Deterministic memory usage

**Tradeoffs:**
- Arena grows to peak usage and stays there (no shrinking)
- Requires scene to be `Clear()`able without deallocating

**Approach:**
1. Implement a `SceneArena` that owns a `Vec<u8>` and hands out slices
2. `Scene::clear()` resets the arena offset to 0 (no deallocation)
3. Chunk command streams are emitted into the arena
4. `Rc<Vec<SceneCommand>>` is replaced by arena slices (copy-on-write still applies)
5. Arena grows on demand, never shrinks (bounded by peak frame usage)

---

## 11. Binary Size Reduction ✅ STAGE 2 (S2-1) — DONE (text-runtime/svg-runtime/desktop features, per-backend svg-runtime, embedded profile)

**Current:** Heavy runtime dependencies (parley, swash, resvg, tiny-skia, winit, wgpu, smithay-client-toolkit). Binary size is not optimized for embedded.

**Target:** Strip embedded builds to < 1 MB (from current multi-MB).

**Benefits:**
- Fits in smaller flash storage
- Faster boot times
- Reduced attack surface

**Approaches:**
1. Feature-gate all heavy dependencies:
   - `parley` + `swash` → only with `feature = "text-runtime"` (see #2 for build-time alternative)
   - `resvg` → only with `feature = "svg-runtime"` (see #3 for build-time alternative)
   - `winit` → only with `feature = "desktop"`
   - `wgpu`/`vello` → only with `feature = "gpu"`
   - `smithay-client-toolkit` → only with `feature = "wayland"`
   - `notify` → only with `feature = "hot-reload"`
2. Use `opt-level = "z"` (optimize for size) in release profile
3. Use `lto = true` and `codegen-units = 1`
4. Strip symbols: `strip = true`
5. Use `panic = "abort"` (no unwinding overhead)
6. Provide a `embedded` feature preset that disables everything except the essentials

---

## 12. Minimum RAM Reduction ✅ STAGE 2 — COMPOSED (S2-1 features + S2-4 shaped-cache 8MB cap + S2-5 linebuffer; glyph cache was already adaptive 4-64MB)

**Current:** Not practical below ~8 MB (frame buffer + glyph cache + shaped text cache + engine overhead).

**Target:** < 1 MB practical floor for embedded Linux; < 300 KiB is achievable only with AOT-compiled UI (see #13).

**Benefits:**
- Deployment on cheaper MCUs with less SRAM
- More headroom for application logic
- Enables battery-powered devices with tight power budgets

**Approaches:**
1. All of the above optimizations contribute
2. Reduce frame buffer: linebuffer backend (#4) eliminates full-frame allocation
3. Reduce glyph cache: build-time shaping (#2) eliminates runtime glyph cache entirely
4. Reduce shaped text cache: build-time shaping (#2) eliminates it
5. Reduce node storage: contiguous arena (#1) reduces per-node overhead
6. Reduce scene allocs: scene arena (#10) eliminates per-frame allocation
7. Strip dependencies: binary size reduction (#11) reduces RAM overhead
8. Provide a `MemoryBudget` struct that lets embedders set hard limits on each cache

---

## 13. AOT Compilation (Data Layout + Pre-Computation) ✅ STAGE 2 (S2-2) — RUNTIME HALF DONE (static freeze without macros; build-time half deferred with #2/#3)

**Note:** The user questioned the value of AOT vs Rust compilation. The real win is not compilation per se — it's **pre-computation** and **data layout**. AOT is the mechanism; the benefit is eliminating runtime interpretation.

**Current:** Runtime engine interprets the component tree every frame. The tree is runtime data behind `Rc<RefCell>`.

**Target:** Pre-compute everything that can be known at build time: static layouts (#7), static text (#2), static SVG (#3), static component geometry. The runtime only processes what is genuinely dynamic.

**Benefits:**
- Eliminates per-frame tree interpretation for static UIs
- Enables the compiler to see the component structure as data (not just function calls)
- Unlocks further optimizations (constant folding, dead code elimination, layout pre-computation)

**Approaches:**
1. This is the meta-optimization that enables #2, #3, #7
2. Does NOT require a new markup language — proc macros on Rust builders suffice
3. The component tree is still constructed at runtime, but static subtrees are pre-computed
4. The signal graph is the irreducible runtime cost — it cannot be AOT-compiled away

---

## 14. Production Embedded Maturity

**Current:** Zero embedded deployments. No field testing on real hardware.

**Target:** Production-ready embedded deployment with real-world validation.

**Benefits:**
- Confidence in stability
- Community contributions and bug fixes
- Ecosystem growth (board support packages, examples)

**Approaches:**
1. All technical optimizations above (#1–#13) are prerequisites
2. Build and maintain board support packages for:
   - STM32F4/F7/H7 (DMA2D, LCD controllers)
   - RP2040 (PIO for display, no DMA2D)
   - ESP32-S3 (DMA, LCD peripheral)
   - Raspberry Pi (Linux-class, GPU optional)
3. Create embedded examples:
   - Hello world on SPI OLED
   - Sensor dashboard with live updates
   - Touch UI with buttons and sliders
4. Benchmark on real hardware: frame rate, memory, power consumption
5. Document integration patterns for each RTOS
6. Establish a testing pipeline for embedded targets (QEMU or hardware-in-loop)

---

## 15. Absolute Transform Caching ✅ STAGE 1 — DONE (per-frame memo + epoch bump + subtree invalidation on mark_dirty)

**Current:** `node_absolute_transform` (`arena.rs:228-253`) walks from the node to the root on every call, building a path and multiplying transforms. O(D) per call, O(M×D) per frame where M = dirty nodes, D = tree depth.

**Called from:** damage computation (`damage.rs:53,121`), layout boundary collection (`boundary.rs:462`), spatial queries (`arena.rs:261,277,290`).

**Target:** Compute absolute transforms top-down during the layout pass and store them on each node. All queries become O(1).

**Benefits:**
- Eliminates the most frequent O(D) walk in the entire codebase
- Benefits damage computation, layout boundary collection, and all spatial queries simultaneously
- One `Transform` (6 `f32`s = 24 bytes) per node overhead

**Tradeoffs:**
- Requires adding an `absolute_transform` field to `LayoutNode` (24 bytes/node)
- Must be invalidated when a node's `resolved_rect` or `effective_transform` changes (already tracked by dirty flags)

**Approach:**
1. Add `absolute_transform: Transform` field to `LayoutNode`
2. During `perform_node_layout`, compute the node's absolute transform and store it
3. Pass the accumulated transform down to children during recursive layout
4. Replace all `node_absolute_transform(id)` calls with `arena.get(id).absolute_transform`
5. Invalidate on dirty (already happens via existing dirty flags)

---

## 16. Dual-Slot Intrinsic Cache ✅ STAGE 0 — DONE

**Current:** `LayoutCache.intrinsic` (`tree/cache.rs:12`) stores a single `Option<(Constraints, bool, IntrinsicSize)>`. When a node is queried with `for_height=false` (item sizing) and then `for_height=true` (container-height sizing) in the same frame, the second query misses and re-walks the subtree.

**Target:** Two separate slots — one for each role:
```rust
pub intrinsic_item: Option<(Constraints, IntrinsicSize)>,
pub intrinsic_height: Option<(Constraints, IntrinsicSize)>,
```

**Benefits:**
- Eliminates redundant subtree walks for any node queried with both roles in the same frame
- Trivial fix — change one struct field and update the two query sites

**Tradeoffs:**
- One extra `Option<(Constraints, IntrinsicSize)>` per node (~40 bytes)
- Minimal code change

**Approach:**
1. Replace `intrinsic: Option<(Constraints, bool, IntrinsicSize)>` with two slots
2. Update `compute_intrinsic` callers in `engine.rs:488,510` to use the appropriate slot
3. Update cache invalidation to clear both slots

---

## 17. `dispose_effect` Reverse Index ✅ STAGE 0 — DONE (with #26)

**Current:** `dispose_effect` and `dispose_subscriber` (`runtime.rs:640-656`) scan ALL subscriber sets to remove entries — O(S) per disposal where S = total signals with subscribers.

**Target:** Reverse index tracking which signals each effect/subscriber subscribes to. Disposal becomes O(K) where K = number of signals the effect actually subscribes to.

**Benefits:**
- O(S) → O(K) per disposal (K << S for most effects)
- Critical for component rebuilds that dispose many effects
- Same pattern already exists for `derived_sources`

**Tradeoffs:**
- Additional memory: one `Vec<SignalId>` per effect/subscriber
- Must maintain the reverse index on every `track_read`

**Approach:**
1. Add `effect_sources: FxHashMap<SubscriberId, Vec<SignalId>>` to `ReactiveRuntime`
2. In `track_read`, push the signal ID to the current observer's `effect_sources` entry
3. In `dispose_effect`, look up `effect_sources[id]` and remove only from those subscriber sets
4. Clean up `effect_sources` on disposal

---

## 18. CPU-to-GPU Vertex Transform (wgpu backend) ⚠️ STAGE 1 — RESCOPED (translation fast-path done; full uniform move deferred: it forces run-splitting on transform change = more draw calls, plus f32 FMA ulp risk on AA edges)

**Current:** The wgpu backend transforms every vertex corner on the CPU during command emission (`aurora-wgpu/src/command.rs:389-419`). For a scene with thousands of rects, this is the single hottest CPU path.

**Target:** Pass the `Transform` as a uniform buffer to the vertex shader. The shader already does ortho projection — adding a 2×3 matrix multiply is nearly free on GPU.

**Benefits:**
- Eliminates all CPU-side `transform_point` calls in the wgpu backend
- Reduces vertex buffer sizes (store local coords instead of device coords)
- Makes the vertex cache portable across transform changes

**Tradeoffs:**
- Requires a uniform buffer update per draw call (or per transform change)
- Slightly more complex shader
- Only benefits the wgpu backend (tiny-skia and vello have their own paths)

**Approach:**
1. Add a `transform: [[f32; 3]; 2]` uniform to the vertex shader
2. In the vertex shader, multiply the local position by the transform before the ortho projection
3. Store local (pre-transform) coordinates in vertex buffers
4. Update the uniform only when the transform changes (track via a transform stack)
5. Benchmark CPU time before/after

---

## 19. Skip Compile on Idle Frames ✅ STAGE 0 — DONE

**Current:** `Scene::clear()` (`pipeline/compile/mod.rs:75`) and a full tree walk happen every frame even when nothing changed. The damage system has a fast path for idle frames, but its result isn't used to skip compilation — only to skip rasterization.

**Target:** When damage is empty and no nodes are dirty, skip the compile pass entirely and reuse the previous scene.

**Benefits:**
- Zero CPU cost for idle frames (no tree walk, no command emission)
- Reduced allocator pressure (no per-frame scene allocation)
- Simple check at the top of the compile pass

**Tradeoffs:**
- Must correctly detect "idle" (no dirty nodes, no damage, no pending structural changes)
- First frame after a change must still compile fully

**Approach:**
1. At the top of `compile_scene_instrumented`, check if `dirty_nodes.is_empty() && damage.is_empty()`
2. If idle, return the previous `Scene` without modification
3. Otherwise, proceed with normal compilation
4. Ensure structural changes (node insert/remove) mark the scene as dirty

---

## 20. Tiny-Skia Chunk Cache ⏪ STAGE 1 — REVERTED (was: bounded per-chunk raster cache; root-caused a scroll-parity failure — tile-space/shared-cache interactions: cached pixmaps baked the wrong frame, integer-blit snapped fractional origins, miss renders risked shared line-cache keys. Compile-time retained-chunk replay + damage-gated tiles already capture most of the win. Redesign (per-space isolation) deferred.)

**Current:** The tiny-skia backend re-executes all commands in all chunks overlapping each damage rect. Within a tile, all intersecting chunks are re-executed even if only one chunk changed.

**Target:** Cache the last-emitted command stream per chunk and compare hashes before re-execution. If the hash matches, blit the previous rasterized result instead of re-executing.

**Benefits:**
- Matches the wgpu backend's `ChunkCache` behavior
- Eliminates redundant command execution for unchanged chunks
- Reduces CPU per frame for partially-updated scenes

**Tradeoffs:**
- Requires a content hash per chunk (already exists as `hash_commands`)
- Additional memory for cached rasterized tiles
- Must invalidate cache entries when chunks are recompiled

**Approach:**
1. Add a `HashMap<NodeId, (u64, Pixmap)>` chunk cache to `TinySkiaRenderer`
2. Before re-executing a chunk, compute its command hash and check the cache
3. On hash match, blit the cached pixmap instead of re-executing
4. On hash miss, execute commands and store the result in the cache
5. Invalidate cache entries when chunks are recompiled (track via `fresh` flag)

---

## 21. Single-Pass Box Blur for Small Radii

**Current:** The box blur (`render/blur.rs:28-45`) always runs two box blur iterations for Gaussian approximation, regardless of radius.

**Target:** For radii ≤4px (common UI shadows), run a single pass. The difference between one and two passes at small radii is sub-pixel.

**Benefits:**
- 2× savings on the most expensive CPU raster operation
- Visually indistinguishable for small radii (the common case for UI shadows)

**Tradeoffs:**
- Slightly less accurate Gaussian approximation for small radii
- Must choose a threshold (≤4px is safe; ≤8px is borderline)

**Approach:**
1. Add a radius check in `box_blur_alpha_with_falloff`
2. If `radius <= 4`, run only the first pass
3. If `radius > 4`, run both passes (current behavior)
4. Visual A/B test at radii 2, 4, 6, 8 to confirm the threshold

---

## 22. SoA Layout for TreeArena ✅ STAGE 2 (S2-3) — DONE (CSR sibling links replace per-parent Vec; ChildIter/children_vec call sites migrated)

**Current:** `TreeArena` stores nodes in an array-of-structs layout. `LayoutNode` is a large struct containing style, geometry, and state. Tree walks access only a few fields per node but traverse the entire struct.

**Target:** Struct-of-arrays layout — separate `Vec` for `resolved_rect`, dirty flags, style, etc. Hot-path fields are contiguous in memory.

**Benefits:**
- Improved cache locality during tree walks (layout, damage, compile)
- Fewer cache misses per node visit
- Better prefetcher behavior

**Tradeoffs:**
- Significant refactoring of `TreeArena` and all tree walk code
- More complex indexing (multiple arrays to keep in sync)
- May increase code size (more index arithmetic)

**Approach:**
1. Identify hot-path fields: `resolved_rect`, `style`, `dirty` flags, `children`
2. Create separate `Vec`s for each field group
3. Migrate `TreeArena` operations to use SoA indexing
4. Benchmark cache misses (perf stat) before/after
5. Consider a hybrid: SoA for hot fields, AoS for cold fields

---

## 23. Unified Command Emitter 🔷 STAGE 1 — DEFERRED (pure refactor, zero direct perf; sizable cross-backend churn — schedule with next SceneCommand variant addition)

**Current:** Three backends (wgpu, vello, tiny-skia) each implement their own `match cmd { ... }` dispatch for `SceneCommand`. Adding a new variant requires updating all three.

**Target:** A trait-based emitter or visitor pattern that each backend implements. Shared command-stream analysis (hashing, optimization) lives in one place.

**Benefits:**
- Eliminates triplicated bug surface
- Enables cross-backend optimizations (shared command-stream optimizer)
- Easier to add new `SceneCommand` variants
- Single point of truth for command semantics

**Tradeoffs:**
- Requires defining a `CommandEmitter` trait with one method per variant
- Each backend implements the trait (boilerplate, but mechanical)
- May need to unify the emission context types

**Approach:**
1. Define `trait CommandEmitter { fn push_transform(&mut self, ...); fn draw_rect(&mut self, ...); ... }`
2. Implement the trait for each backend's command executor
3. Replace the three `match cmd { ... }` blocks with a single `cmd.emit(&mut emitter)` call
4. Move `hash_commands` and other analysis to the shared `SceneCommand` type

---

## 24. Speculative Text Prefetch

**Current:** Text shaping happens synchronously when the layout pass requests it. No prefetching of likely-next-frame text.

**Target:** During idle frames or low-CPU-usage periods, speculatively shape text for nodes just outside the viewport (next scroll page, next animation frame).

**Benefits:**
- Eliminates shaping latency when scrolling or animating
- Smooth UX for text-heavy interfaces
- Leverages idle CPU time that would otherwise be wasted

**Tradeoffs:**
- Wasted work if the user doesn't scroll/animate as predicted
- Requires a visibility heuristic (which nodes are "near" the viewport?)
- Adds complexity to the frame scheduler

**Approach:**
1. Track which text nodes are visible or near-visible (within one viewport of the scroll position)
2. During idle frames (no dirty nodes, no damage), shape text for near-visible nodes
3. Store results in the shaped text cache (they'll be used when the node scrolls into view)
4. Cap prefetch work to avoid wasting CPU (e.g., max 2 nodes per idle frame)

---

## 25. Glyph Bitmap Compression — DEFERRED (raster-scale quantization is pixel-visible; needs human A/B. Shaped-cache byte cap done in S2-4)

**Current:** The glyph bitmap cache stores full `Vec<u8>` per glyph per phase (1 byte per pixel). Hinted glyphs (≤16px) have mostly binary coverage (hinted stems are 0 or 255).

**Target:** For hinted glyphs, use 1-bit-per-pixel or simple RLE compression. 4-8× memory reduction for the hinted path.

**Benefits:**
- 4-8× more glyphs in the same memory budget
- Reduced memory fragmentation (smaller allocations)
- Better cache locality (smaller glyph data)

**Tradeoffs:**
- Decompression cost on glyph lookup (minimal for RLE)
- Only applies to hinted path (≤16px); linear coverage above 16px is already compact
- Adds complexity to the glyph cache

**Approach:**
1. Detect hinted glyphs (size ≤16px and hinted)
2. Compress bitmap data with RLE (run-length encoding is simple and fast for binary-ish data)
3. Store compressed data in `CachedBitmap` with a compression flag
4. Decompress on lookup (or decompress to a temporary buffer)
5. Benchmark memory savings vs decompression cost

---

## 26. `track_read` Allocation Reduction ✅ STAGE 0 — DONE

**Current:** `track_read` (`runtime.rs:706-713`) calls `entry().or_default()` which allocates a new `FxHashSet` for each signal read for the first time. A component reading 10 signals allocates 10 hash sets.

**Target:** Use `SmallVec<[Observer; 4]>` as the subscriber set type. Most signals have 0-2 subscribers.

**Benefits:**
- Zero allocation for signals with ≤4 subscribers (the common case)
- Reduced allocator pressure during component initialization
- Better cache locality (subscribers are inline)

**Tradeoffs:**
- Slightly more complex insertion/removal logic
- Signals with >4 subscribers fall back to heap (still works, just slower)

**Approach:**
1. Change `subscribers: FxHashMap<SignalId, FxHashSet<Observer>>` to `FxHashMap<SignalId, SmallVec<[Observer; 4]>>`
2. Update `track_read` to push to the SmallVec
3. Update `dispose_effect` to retain/remove from the SmallVec
4. Benchmark allocation count before/after

---

## 27. `invalidate_roots` Allocation Reduction ✅ STAGE 0 — DONE

**Current:** `invalidate_roots` (`runtime.rs:742-787`) allocates a `FxHashSet` for `visited` and a `Vec` for `stack` on every call. Called on every signal write.

**Target:** Reusable buffers via `RefCell` or a generation-stamped visited array.

**Benefits:**
- Zero allocation on signal writes (the hot path)
- Reduced GC pressure for large graphs with frequent writes

**Tradeoffs:**
- Requires interior mutability (`RefCell`) or unsafe code for the reusable buffer
- Must be careful about reentrancy (signal writes during invalidation)

**Approach:**
1. Add `RefCell<Vec<SignalId>>` and `RefCell<FxHashSet<SignalId>>` to `ReactiveRuntime`
2. In `invalidate_roots`, borrow the buffers, clear them, and reuse
3. Alternatively, use a generation-stamped `Vec<u32>` (one per signal) as the visited set — O(1) clear by bumping the generation counter
4. The generation-stamped approach is faster and avoids `RefCell` borrow overhead

---

## 28. `balance_best_width` Binary Search

**Current:** `balance_best_width` (`text/shape.rs:220-269`) shapes the same text up to 12 times at different widths to find the optimal balanced wrapping.

**Target:** Binary search over widths — O(log 10) ≈ 4 shapes instead of 10.

**Benefits:**
- ~3× fewer shaping calls for balanced text
- Only affects cache misses (balanced headings are relatively rare)
- Simple change to the search strategy

**Tradeoffs:**
- Binary search may not find the exact optimal width (but close enough)
- The cost function may not be strictly monotonic (but usually is)

**Approach:**
1. Replace the linear scan over 10 candidates with a binary search
2. Define a cost function (e.g., `|line_count - target| + |width - avail|`)
3. Binary search for the width that minimizes the cost
4. Fall back to linear scan if the cost function is non-monotonic

---

## 29. Dual-Axis Min-Size Computation

**Current:** `compute_min_size` (`layout/engine.rs:200,221`) is called separately per axis per child, each triggering a subtree walk. O(2ND) for N children at depth D.

**Target:** Compute both axes in a single pass. `compute_min_size` returns `(f32, f32)` and caches both simultaneously.

**Benefits:**
- Halves the number of subtree walks for min-size computation
- Better cache locality (both axes computed from the same subtree visit)

**Tradeoffs:**
- Requires changing the `compute_min_size` signature and cache structure
- Slightly more complex cache invalidation (both axes invalidated together)

**Approach:**
1. Change `compute_min_size` to return `(f32, f32)` (horizontal, vertical)
2. Replace `min_inline: Option<f32>` and `min_block: Option<f32>` with `min_size: Option<(f32, f32)>`
3. Update `min_group_size` to compute both axes in one loop
4. Update callers to destructure the tuple

---

## Summary Table

| # | Optimization | Category | Embedded Impact | Desktop Impact | Effort |
|---|-------------|----------|-----------------|----------------|--------|
| 1 | Contiguous component storage | Data layout | High | Medium | High |
| 2 | Build-time text shaping | Pre-computation | Critical | Low | Medium |
| 3 | Build-time SVG rasterization | Pre-computation | High | Low | Medium |
| 4 | Linebuffer backend | Backend | Critical | None | Medium |
| 5 | DMA2D backend | Backend | High (STM32) | None | Medium |
| 6 | SIMD rasterization | CPU | Medium | Medium | Medium |
| 7 | AOT-resolved static layouts | Pre-computation | High | Low | Medium |
| 8 | `no_std` support | Portability | Critical | None | High |
| 9 | RTOS/baremetal support | Portability | Critical | None | Medium |
| 10 | Scene arena | Allocation | Medium | Low | Low |
| 11 | Binary size reduction | Build | High | None | Low |
| 12 | Minimum RAM reduction | Memory | Critical | None | High |
| 13 | AOT pre-computation | Pre-computation | High | Low | Medium |
| 14 | Production embedded maturity | Ecosystem | Critical | None | High |
| 15 | Absolute transform caching | Algorithmic | High | High | Moderate |
| 16 | Dual-slot intrinsic cache | Cache | High | High | Low |
| 17 | `dispose_effect` reverse index | Algorithmic | Medium | Medium | Moderate |
| 18 | CPU-to-GPU vertex transform | GPU | None | High | Medium |
| 19 | Skip compile on idle frames | Lazy eval | Medium | Medium | Low |
| 20 | Tiny-skia chunk cache | Incremental | Medium | Medium | Medium |
| 21 | Single-pass box blur | Approximation | Medium | Medium | Low |
| 22 | SoA layout for TreeArena | Memory layout | Medium | Medium | High |
| 23 | Unified command emitter | Architecture | Low | Low | Medium |
| 24 | Speculative text prefetch | Speculative | Low | Medium | Medium |
| 25 | Glyph bitmap compression | Compression | Medium | Low | Medium |
| 26 | `track_read` allocation reduction | Allocation | Low | Low | Low |
| 27 | `invalidate_roots` allocation reduction | Allocation | Low | Low | Low |
| 28 | `balance_best_width` binary search | Algorithmic | Low | Low | Low |
| 29 | Dual-axis min-size computation | Algorithmic | Medium | Medium | Moderate |

**Critical path for embedded Linux:** #1 → #2 → #3 → #4 → #8 → #11 → #12 → #14

**Critical path for MCU/baremetal:** #8 → #2 → #4 → #12 → #9 → #14 (requires `no_std` first)

**Desktop wins (no embedded requirement):** #15 → #16 → #18 → #19 → #21 → #6 → #10

**Quick wins (low effort, high impact):** #16 → #19 → #21 → #26 → #27 → #28
