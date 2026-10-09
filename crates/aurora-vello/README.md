# aurora-vello — GPU rasterization backend for the Aurora UI engine

Hardware renderer: translates `aurora` display-list scenes into Vello (`wgpu`)
encodings, with a CPU fallback backend (`create_cpu_backend`) for headless use.

## Layout

| Path | Responsibility |
|------|----------------|
| `src/renderer.rs`  | Frame render + chunk reuse predicates |
| `src/command.rs`   | `SceneCommand` → Vello scene translation |
| `src/text.rs`      | Glyph blob upload cache |
| `src/shadow.rs`    | Shadow patch cache (hand-kept in parity with tiny-skia) |
| `src/svg.rs`, `src/svg_paint.rs`, `src/shader.rs`, `src/path.rs`, `src/context.rs` | Paint/context helpers |

## Dependencies

Depends on `aurora` (scene, text contexts) — never the reverse. Needs a GPU at
runtime (or the CPU backend); `tests/gpu_snapshot.rs` covers the live path.
