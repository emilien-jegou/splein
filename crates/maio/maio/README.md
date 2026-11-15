# maio

Concurrent, macro-safe storage backing `.maiodb`.

A high-throughput, multi-process ACID task journal and content-addressed
metadata store: multiple `rustc` compiler jobs, `rust-analyzer` workers and
`cargo check` instances append concurrently, guarded by advisory file locks
(`fs4`). Registration is designed to stay under a millisecond: hash the task
parameters, append a frame, release the lock.

Licensed under GPL-3.0.
