# maio-core

Domain core of the `maio` asset engine: content-addressed `AssetId`,
strongly typed `AssetTask` definitions, and deterministic BLAKE3 hashing.

Deliberately decoupled from file I/O and proc-macro machinery: every other
crate in the `maio` suite (and any future proc-macro layer) depends on this
one and nothing else.

Licensed under GPL-3.0.
