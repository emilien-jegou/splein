// Single responsibility: the backdrop tone curve as an explicit value.
//
// The engine ships exactly one fixed reference curve (below): it is
// transfer rendering, like an sRGB decode — not a look. A userland
// preset (e.g. anodeui) carries its own curve for looks that need a
// different one (shadow crush, dark lift) and passes it through
// `backdrop_tone_curve`; `None` keeps the reference curve.

/// A full tone-curve set for one glass look.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ToneCurve {
    /// Grey tone LUT output knots at inputs `i / 8`.
    pub knots: [f32; 9],
    /// Small-shape tone LUT, applied after the fill wash and dim.
    pub small_knots: [f32; 9],
    /// Half shorter side (logical px) at/below which the small curve
    /// applies at full weight.
    pub small_lo: f32,
    /// Half shorter side at/above which it is off.
    pub small_hi: f32,
    /// How far the tone LUT is keyed on luma instead of per channel.
    pub luma_mix: f32,
    /// Small-shape shadow lift.
    pub lift: f32,
    /// Input level where `lift` fades to 0.
    pub lift_knee: f32,
    /// Half shorter side at/above which `lift` is 0.
    pub lift_size_ref: f32,
}

/// The engine's fixed reference curve (the regular look's fitted
/// knots). Documented as transfer rendering: every backdrop passes
/// through it unless a preset supplies its own `ToneCurve`.
pub const FIXED_REFERENCE_CURVE: ToneCurve = ToneCurve {
    knots: [0.0, 0.113, 0.25, 0.363, 0.4973, 0.6263, 0.7534, 0.8839, 1.0],
    small_knots: [0.0, 0.1, 0.2, 0.3, 0.5, 0.611, 0.7142, 0.8538, 0.974],
    small_lo: 32.0,
    small_hi: 34.0,
    luma_mix: 0.6,
    lift: 0.0,
    lift_knee: 0.5,
    lift_size_ref: 0.0,
};
