# aurora-tiny-skia — CPU rasterization backend for the Aurora UI engine

Software renderer: executes `aurora` display-list scenes onto CPU pixel buffers
via `tiny-skia` + `softbuffer` presentation.

## Layout

| Path | Responsibility |
|------|----------------|
| `src/renderer.rs` | Frame render: damage-driven chunk replay |
| `src/command.rs`  | Per-command execution onto scratch tiles |
| `src/text.rs`     | Glyph compositing + line-bitmap cache |
| `src/shadow/`     | Layered shadow patch generation + direct-blur path |
| `src/svg.rs`, `src/image.rs`, `src/shader.rs`, `src/clip.rs`, `src/path.rs`, `src/stroke.rs` | Primitive rasterizers |
| `src/presenter.rs`| Window presentation + scanline canvas preservation |
| `src/testing/`    | Headless `Harness` (frames, input injection, snapshots, filmstrips) used by `aurora`'s test suites |

## Dependencies

Depends on `aurora` (scene, text contexts) — never the reverse. Parses the same
`SceneCommand` stream as the Vello backend; shadow/patch parity between the two
is maintained by hand (see `shadow/patch.rs`).
