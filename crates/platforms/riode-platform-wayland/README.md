# riode-wayland — Wayland layer-shell SHM presentation backend

Presents `riode` frames on Wayland via the layer-shell protocol with shared-memory
(SHM) buffers, compositing the core engine with the CPU (`riode-tiny-skia`)
rasterizer.

## Dependencies

Depends on `riode` + `riode-tiny-skia` — never the reverse. Compositor-side
concerns only (surfaces, buffers, layer-shell); all layout/paint logic stays in
the engine crates.
