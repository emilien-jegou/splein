// Single responsibility: Anode UI kit — brand-free liquid-glass
// primitives matched to the reference platform's look.
//
// This crate owns *looks*. The engine (`riode`) owns *mechanisms*:
// it bends, blurs, washes and shades pixels from explicit per-stage
// values and knows no presets. Every number below is fitted reference
// data (transcribed from the engine's former constant table — the
// table is deleted, the numbers move here, nothing is re-derived),
// clearly marked and never consulted by the engine itself.
//
// A preset expands to a complete explicit bundle: lens geometry plus
// every backdrop stage. Apply it as one DSL unit (`preset={...}`);
// override any single value with the atomic props afterwards.

pub mod glass;

pub use glass::GlassPreset;
// ToneCurve lives in the engine (`riode::foundation::glass`); the
// preset consts below are typed with it.
pub use riode::foundation::glass::ToneCurve;
