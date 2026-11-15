# maio-compiler

Batch concurrency processing engine for `.maiodb` tasks.

Executes pending journal tasks in parallel (`rayon`) and emits baked
artifacts to `target/assets/<asset_id>.bin`. The image pipeline decodes,
resamples (Lanczos3), premultiplies alpha and packs RGBA8, clamped to 2048
pixels per edge. Artifacts carry a fixed-layout header (source content hash
included) so freshness is checkable without deserialization.

Licensed under GPL-3.0.
