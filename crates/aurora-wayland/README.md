# aurora-wayland — Wayland layer-shell SHM presentation backend

Presents `aurora` frames on Wayland via the layer-shell protocol with shared-memory
(SHM) buffers, compositing the core engine with the CPU (`aurora-tiny-skia`)
rasterizer.

## Dependencies

Depends on `aurora` + `aurora-tiny-skia` — never the reverse. Compositor-side
concerns only (surfaces, buffers, layer-shell); all layout/paint logic stays in
the engine crates.
