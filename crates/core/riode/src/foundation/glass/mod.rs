// Single responsibility: the Liquid-Glass material model — what a glass
// layer *is*, the fitted constants that shape it, and the uniform pack
// that ships it to a shader.
//
// This module is a 1:1 port of `adaptive_liquid_glass/` (the Flutter
// package). Source files are named in each file's header; field names,
// defaults and constant values are transcribed rather than re-derived so a
// diff against the reference stays mechanical. The port's scope, the
// features it drops and why are tracked in
// `ongoing-liquid-glass-upgrade.md` at the repo root.
//
// * [`material`] — `Glass`: the lens material the DSL authors.
// * [`backdrop`] — `Backdrop`: the non-glass stages the lens samples
//   (core blur, wash, saturation, tone, ambient, fill). Split out of
//   `Glass` because they all stay meaningful without the lens.
// * [`spectral`] — `Spectral`: the wavelength-dependence of refraction
//   (the rainbow). Split out of `Glass` because it is a property of the
//   refraction, not the lens material — the glass stays monochromatic.
// * [`group`] — `GlassMember` / morph tracks: what a glass group packs.
pub mod glow;
// * [`constants`] — the packed constant layout (`ZERO` neutral base)
//   plus the engine-wide shape globals. No looks live here; fitted
//   numbers belong to userland presets (`riode-anodeui`).
// * [`motion`] — touch deformation and springs for `.interactive()`.
// * [`style`] — `GlassStyle`: one complete look as an engine value.
// * [`tone`] — `ToneCurve` + the engine's fixed reference curve.
// * [`uniforms`] — the frame uniform layout and `pack_glass_uniforms`.

pub mod backdrop;
pub mod constants;
pub mod group;
pub mod material;
pub mod motion;
pub mod light_spill;
pub mod refraction;
pub mod spectral;
pub mod style;
pub mod tone;
pub mod uniforms;

pub use backdrop::*;
pub use constants::*;
pub use glow::*;
pub use group::*;
pub use material::*;
pub use motion::*;
pub use light_spill::*;
pub use refraction::*;
pub use spectral::*;
pub use style::*;
pub use tone::*;
pub use uniforms::*;
