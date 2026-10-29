// Single responsibility: the Chain-spec shading-chain vocabulary —
// plan-better-glass-setup.md Part 2 (P1/P2 verbatim). One trait, one
// primitive set, one fixed 256-byte uniform block. Composition is
// data: a `Material` stage chain compiled per-frame from a
// CPU-resolved [`Geo`] into the `ChainFrame` block + `stage_mask`. The shader
// never changes per composition — one static `modular_glass.wgsl`.

pub mod primitives;

pub use primitives::*;
