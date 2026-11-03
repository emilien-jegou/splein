// Single responsibility: shared value vocabulary for shader stages.
//
// The contracts live in `riode` (which we depend on): `Stage`, `Geo`,
// `Color` are re-exported here so stage modules keep one import path.
// No duplicate trait/struct definitions — `riode` owns them.

pub use riode::foundation::{Color, Geo, Stage};
