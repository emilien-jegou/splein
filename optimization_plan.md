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

## 2. Build-Time Text Shaping via Proc Macros ✅ DONE (`pre_shaped!`: parley shaping + swash phase-0 raster on build host → versioned blob → `shape_pre_shaped` rehydrates with zero shaping latency, zero font lookup; integer-snapped raster documented tradeoff; metrics-equality test guards the mirror)

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

## 3. Build-Time SVG Rasterization ✅ DONE (`include_svg!(path, W, H)`: host resvg → `ImageSource::from_static_rgba8`, one memcpy instead of a parse; dynamic theming stays on runtime path)

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

## 7. AOT-Resolved Static Layouts ✅ DONE (runtime freeze S2-2 + FrozenLayout key-path snapshots with cache priming, viewport-gated, corrupt-rejecting; 4 tests)

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

**Measured (demo1 release, 2026-10):** default 23.08 MB / `--no-default-features` 23.08 MB (deps unify via the demo's backend imports — flags pay off for downstream embedders, not the gallery) / `strip` 16.95 MB. Symbol attribution: wgpu 2.8, wayland 0.6, winit 0.5, skrifa+read_fonts+swash 1.2, parley 0.14, resvg 0.09, notify 0.19 MB.
**`notify`/`libloading` deliberately NOT gated:** 0.3 MB combined vs. splitting the load-bearing dev loop (`HotReload` struct, `Engine::drain_hot_reload`, component-macro runtime all touch it). Below the noise floor until `no_std` returns.

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

## 12. Minimum RAM Reduction ✅ STAGE 2+ — CLOSED (S2-1 features + S2-4 shaped-cache 8MB cap + S2-5 linebuffer + state-of-the-art pass: LRU everywhere incl. VelloImageCache, byte budgets on mirrors, CacheDiagnostics gauge + tripwires; key_map documented insert-only-coupled)

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

## 13. AOT Compilation (Data Layout + Pre-Computation) ✅ DONE (runtime freeze S2-2 + FrozenLayout snapshots + pre-shape/pre-raster macros; remaining sub-items closed individually)

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

## 29. Dual-Axis Min-Size Computation ✅ VERIFIED SHIPPED (dual `min_inline`/`min_block` memo already in tree; no work remaining)

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


---

## Removed items (with cold verdicts)

- **#5 DMA2D backend** — skipped by direction: STM32-only hardware path with no target on the desk.
- **#6 SIMD rasterization** — removed: row pass is a sequential O(1)/px sliding window (likely memory-bound, auto-vectorizes); the hard part is structural (strided col pass), not intrinsics. Future kernel work starts from benches, not a speedup claim.
- **#8 `no_std` support** — skipped by direction: portability deprioritized; feature flags are the foundation if revived.
- **#9 RTOS/baremetal** — removed with #8 (no meaning without it).
- **#14 Production embedded maturity** — skipped by direction: ecosystem/hardware work with no targets.
- **#18-full GPU vertex transform** — removed: ~0.1–0.4 ms/frame worst-case savings vs. run-splitting cost (more draw calls); translation fast-path subset shipped.
- **#21 Single-pass box blur** — removed: saves microseconds typical / ~0.9 ms only under cache thrash (better fixed via patch-cache LRU), changes Gaussian tails.
- **#23 Unified command emitter** — removed from perf plan: zero perf content (cleanup track if anywhere).
- **#24 Speculative text prefetch** — removed: wastes work by design, no shaping-latency evidence.
- **#25 Glyph bitmap compression** — removed: decompress tax on hot path for ≤16px only; byte caps already bound memory.
- **#28 `balance_best_width` binary search** — removed: unsound as specified (steppy cost); path is rare (balanced headings, cache-miss only).
- **#1-full bump-arena / #22-full hot-cold split** — removed as committed items after CSR took the allocation win; re-add only on `perf stat` evidence.
