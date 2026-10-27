// Single responsibility: the material-DSL intermediate representation.
//
// The `view!` macro interprets the closure algebra at expansion time
// (D-M6) into this tree. Structure (which constructors, how nested) is
// fixed here and decides the generated WGSL; every numeric parameter
// rides a uniform slot filled at runtime. This split is load-bearing:
// structure can never vary per frame, values can (hover) with zero
// recompile.
//
// Design notes:
// - `MaterialExpr` is a plain data tree, not Rust closures: it must
//   hash (pipeline-cache key input), emit WGSL, and separate uniform
//   slots from literals. `Hash` on this tree = structural identity.
// - Values are `Value::Lit` (a float baked into the emitted source at
//   expansion time — only legal when the macro saw a literal) or
//   `Value::Slot` (a runtime expression whose tokens the macro emits
//   into the uniform-filler; the slot index is the arena position).
// - Lowering (`lower.rs`, next step) walks this tree to straight-line
//   WGSL inside `fn apply(...)`. This file owns no WGSL text.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// One numeric parameter of the material: either a compile-time literal
/// (structure — inlined into the generated WGSL) or a runtime uniform
/// slot (value — filled per frame by emitted code).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Structure: the macro saw a float literal. Inlined into WGSL.
    Lit(f32),
    /// Value: a runtime expression (`GAIN`, `hover.get()`, ...). The
    /// macro emits the expression into the uniform-filler; `slot` is
    /// the arena position it fills.
    Slot { slot: u32 },
}

impl Value {
    /// FNV-1a fold for hashing (f32 has no stable `Hash` across the
    /// DSL's needs — bits are the identity).
    fn hash_bits<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Lit(v) => {
                state.write_u8(0);
                state.write_u32(v.to_bits());
            }
            Value::Slot { slot } => {
                state.write_u8(1);
                state.write_u32(*slot);
            }
        }
    }
}

/// Normal field: the geometry every closure shades with.
#[derive(Clone, Debug, PartialEq)]
pub enum Normal {
    /// `(0,0,1)` baseline.
    Flat,
    /// Card lens profile at gain strength.
    Card { gain: Value },
    /// Bevel band of width `radius_px` at gain.
    Beveled { gain: Value, radius_px: Value },
    /// Spherical bulge; positive curvature = magnifying.
    Meniscus { radius_px: Value, curvature: Value },
    /// Soft fluid-tension boundary normal.
    FluidTension { radius_px: Value },
    /// Sine height field; ridges ∥ Y, wave along X at `freq` cycles/px.
    Ribbed { freq_per_px: Value },
    /// Gradient-noise bump at scale.
    Perlin { scale: Value },
    /// Summed perturbation (tangent slopes add, renormalized).
    Sum(Box<Normal>, Box<Normal>),
    /// Phase 2 `geo.normal`: per-pixel host normal field
    /// (`ctx.normal`). Tracks geometry; optics on it still gate
    /// Phase 5a (Refract) / 5b (fresnel weight on geo normal).
    GeoNormal,
}

impl Normal {
    fn hash_bits<H: Hasher>(&self, state: &mut H) {
        match self {
            Normal::Flat => state.write_u8(0),
            Normal::Card { gain } => {
                state.write_u8(1);
                gain.hash_bits(state);
            }
            Normal::Beveled { gain, radius_px } => {
                state.write_u8(2);
                gain.hash_bits(state);
                radius_px.hash_bits(state);
            }
            Normal::Meniscus { radius_px, curvature } => {
                state.write_u8(3);
                radius_px.hash_bits(state);
                curvature.hash_bits(state);
            }
            Normal::FluidTension { radius_px } => {
                state.write_u8(4);
                radius_px.hash_bits(state);
            }
            Normal::Ribbed { freq_per_px } => {
                state.write_u8(5);
                freq_per_px.hash_bits(state);
            }
            Normal::Perlin { scale } => {
                state.write_u8(6);
                scale.hash_bits(state);
            }
            Normal::Sum(a, b) => {
                state.write_u8(7);
                a.hash_bits(state);
                b.hash_bits(state);
            }
            Normal::GeoNormal => state.write_u8(8),
        }
    }
}

/// Environment lighting term.
#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    pub gain: Value,
}

/// A closure: one light-transport term evaluated per pixel.
#[derive(Clone, Debug, PartialEq)]
pub enum Closure {
    /// Phase 4.5 warp node: sine-displaced read of the SHARP capture
    /// (`ReadSharp` — the raw snapshot, no pre-blur so the displacement
    /// stays crisp). Both params are uniform-bound (D-M10b): values ride
    /// slots, zero recompile on hover. Lowers to its own `apply` body
    /// (not a radiance term): the warp module samples `sharp` at the
    /// shifted position and preserves the host silhouette alpha.
    Warp { frequency: Value, amplitude: Value },
    /// The only construct that reads the backdrop: refraction through
    /// `normal` at IOR, sampled with blur sigma (`0` = sharp snapshot).
    Refract {
        normal: Normal,
        ior: Value,
        frost: Value,
    },
    /// Isotropic GGX specular against the environment.
    Ggx { normal: Normal, roughness: Value },
    /// Phase 4 preset (D-M10c): 3-wavelength RGB dispersion split
    /// around a base IOR — the ex-12 pattern as one node. All params
    /// uniform-bound (D-M10b). Lowers in Phase 5a (needs Refract).
    Dispersion {
        normal: Normal,
        base_ior: Value,
        spread_px: Value,
    },
    /// Phase 4 preset (D-M10c): refraction + Beer absorption + edge
    /// darken in one node — the ex-3 pattern. All params
    /// uniform-bound (D-M10b). Lowers in Phase 5c (needs Refract+Beer).
    Slab {
        normal: Normal,
        thickness_px: Value,
        tint_r: Value,
        tint_g: Value,
        tint_b: Value,
        absorb: Value,
    },
    /// Beckmann specular (extended set, Phase 6).
    Beckmann { normal: Normal, roughness: Value },
    /// Anisotropic GGX (extended set, Phase 6).
    Anisotropic {
        normal: Normal,
        roughness_x: Value,
        roughness_y: Value,
    },
    /// Complex-IOR metallic Fresnel (extended set, Phase 6).
    Conductor {
        normal: Normal,
        kr: [Value; 3],
        ki: [Value; 3],
        roughness: Value,
    },
    /// Charlie fiber sheen (extended set, Phase 6).
    Sheen { normal: Normal, roughness: Value },
    /// Burley diffuse (extended set, Phase 6).
    Burley { normal: Normal, roughness: Value },
    /// Emissive grid (extended set, Phase 6; LDR gain until HDR lands).
    EmissionGrid {
        spacing_px: Value,
        width_px: Value,
    },
}

/// A modulator: scales or attenuates a term (factors commute).
#[derive(Clone, Debug, PartialEq)]
pub enum Modulator {
    /// Albedo / tint.
    Color { r: Value, g: Value, b: Value },
    /// Beer–Lambert `exp(-k · path_length)` absorption.
    Beer { r: Value, g: Value, b: Value, density: Value },
    /// Thin-film interference weight (extended set, Phase 6).
    ThinFilm {
        thickness_nm: Value,
        film_ior: Value,
    },
    /// Radiance gain (`lumens > 1` deferred to HDR, Step 5).
    Intensity { lumens: Value },
}

/// Blend weight for `mix`.
#[derive(Clone, Debug, PartialEq)]
pub enum Weight {
    /// Schlick `F0 + (1-F0)(1-NoV)^5`, `NoV = n.z`.
    Fresnel { normal: Normal, ior: Value },
    /// Scalar weight (runtime-driven for hover).
    Scalar(Value),
    /// Phase 2 `geo.*` scalar field (`thickness` / `edge` /
    /// `position` / `curvature`): lowers to the corresponding
    /// per-pixel host-field read (never a uniform slot).
    GeoField(String),
}

/// One material term: closures, modulated terms, sums, mixes.
#[derive(Clone, Debug, PartialEq)]
pub enum MaterialExpr {
    /// Closure against the unit ambient (gain 1.0).
    Closure(Closure),
    /// Closure against an explicit environment gain.
    Lit(Closure, Env),
    /// Multiplicative modulation of one term (commutes).
    Modulated(Modulator, Box<MaterialExpr>),
    /// Radiance sum (independent light contributions).
    Sum(Box<MaterialExpr>, Box<MaterialExpr>),
    /// `a·(1-w) + b·w`.
    Mix(Box<MaterialExpr>, Box<MaterialExpr>, Weight),
}

/// The macro-time bump allocator for uniform slots. Every value-level
/// expression the macro meets takes the next slot; the emitted filler
/// writes slots in the same order, so runtime order == arena order.
#[derive(Clone, Debug, Default)]
pub struct UniformArena {
    next: u32,
}

impl UniformArena {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Reserve one slot, returning its index.
    pub fn alloc(&mut self) -> u32 {
        let slot = self.next;
        self.next += 1;
        slot
    }

    pub fn len(&self) -> u32 {
        self.next
    }
}

/// Structural hash of the tree: feeds the pipeline-cache key (source
/// identity, D-M5). Slot *positions* participate (layout is structure);
/// slot *values* never do (values ride uniforms at runtime).
pub fn structural_hash(expr: &MaterialExpr) -> u64 {
    let mut h = DefaultHasher::new();
    expr.hash_bits(&mut h);
    h.finish()
}

impl MaterialExpr {
    fn hash_bits<H: Hasher>(&self, state: &mut H) {
        match self {
            MaterialExpr::Closure(c) => {
                state.write_u8(0);
                c.hash_bits(state);
            }
            MaterialExpr::Lit(c, e) => {
                state.write_u8(1);
                c.hash_bits(state);
                e.gain.hash_bits(state);
            }
            MaterialExpr::Modulated(m, inner) => {
                state.write_u8(2);
                m.hash_bits(state);
                inner.hash_bits(state);
            }
            MaterialExpr::Sum(a, b) => {
                state.write_u8(3);
                a.hash_bits(state);
                b.hash_bits(state);
            }
            MaterialExpr::Mix(a, b, w) => {
                state.write_u8(4);
                a.hash_bits(state);
                b.hash_bits(state);
                w.hash_bits(state);
            }
        }
    }
}

impl Closure {
    fn hash_bits<H: Hasher>(&self, state: &mut H) {
        match self {
            Closure::Warp { frequency, amplitude } => {
                state.write_u8(10);
                frequency.hash_bits(state);
                amplitude.hash_bits(state);
            }
            Closure::Refract { normal, ior, frost } => {
                state.write_u8(0);
                normal.hash_bits(state);
                ior.hash_bits(state);
                frost.hash_bits(state);
            }
            Closure::Ggx { normal, roughness } => {
                state.write_u8(1);
                normal.hash_bits(state);
                roughness.hash_bits(state);
            }
            Closure::Dispersion {
                normal,
                base_ior,
                spread_px,
            } => {
                state.write_u8(8);
                normal.hash_bits(state);
                base_ior.hash_bits(state);
                spread_px.hash_bits(state);
            }
            Closure::Slab {
                normal,
                thickness_px,
                tint_r,
                tint_g,
                tint_b,
                absorb,
            } => {
                state.write_u8(9);
                normal.hash_bits(state);
                thickness_px.hash_bits(state);
                tint_r.hash_bits(state);
                tint_g.hash_bits(state);
                tint_b.hash_bits(state);
                absorb.hash_bits(state);
            }
            Closure::Beckmann { normal, roughness } => {
                state.write_u8(2);
                normal.hash_bits(state);
                roughness.hash_bits(state);
            }
            Closure::Anisotropic { normal, roughness_x, roughness_y } => {
                state.write_u8(3);
                normal.hash_bits(state);
                roughness_x.hash_bits(state);
                roughness_y.hash_bits(state);
            }
            Closure::Conductor { normal, kr, ki, roughness } => {
                state.write_u8(4);
                normal.hash_bits(state);
                for v in kr.iter().chain(ki.iter()) {
                    v.hash_bits(state);
                }
                roughness.hash_bits(state);
            }
            Closure::Sheen { normal, roughness } => {
                state.write_u8(5);
                normal.hash_bits(state);
                roughness.hash_bits(state);
            }
            Closure::Burley { normal, roughness } => {
                state.write_u8(6);
                normal.hash_bits(state);
                roughness.hash_bits(state);
            }
            Closure::EmissionGrid { spacing_px, width_px } => {
                state.write_u8(7);
                spacing_px.hash_bits(state);
                width_px.hash_bits(state);
            }
        }
    }
}

impl Modulator {
    fn hash_bits<H: Hasher>(&self, state: &mut H) {
        match self {
            Modulator::Color { r, g, b } => {
                state.write_u8(0);
                r.hash_bits(state);
                g.hash_bits(state);
                b.hash_bits(state);
            }
            Modulator::Beer { r, g, b, density } => {
                state.write_u8(1);
                r.hash_bits(state);
                g.hash_bits(state);
                b.hash_bits(state);
                density.hash_bits(state);
            }
            Modulator::ThinFilm { thickness_nm, film_ior } => {
                state.write_u8(2);
                thickness_nm.hash_bits(state);
                film_ior.hash_bits(state);
            }
            Modulator::Intensity { lumens } => {
                state.write_u8(3);
                lumens.hash_bits(state);
            }
        }
    }
}

impl Weight {
    fn hash_bits<H: Hasher>(&self, state: &mut H) {
        match self {
            Weight::Fresnel { normal, ior } => {
                state.write_u8(0);
                normal.hash_bits(state);
                ior.hash_bits(state);
            }
            Weight::Scalar(v) => {
                state.write_u8(1);
                v.hash_bits(state);
            }
            Weight::GeoField(f) => {
                state.write_u8(2);
                state.write(f.as_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_values_do_not_change_structure() {
        // Same tree shape, different slot *positions* => different
        // structure (layout is structure). Same shape, same slots,
        // differing only in runtime values => identical hash: values
        // never participate (D-M5).
        let a = MaterialExpr::Closure(Closure::Ggx {
            normal: Normal::Flat,
            roughness: Value::Slot { slot: 0 },
        });
        let b = MaterialExpr::Closure(Closure::Ggx {
            normal: Normal::Flat,
            roughness: Value::Slot { slot: 0 },
        });
        let c = MaterialExpr::Closure(Closure::Ggx {
            normal: Normal::Flat,
            roughness: Value::Slot { slot: 1 },
        });
        assert_eq!(structural_hash(&a), structural_hash(&b));
        assert_ne!(structural_hash(&a), structural_hash(&c));
    }

    #[test]
    fn lit_vs_slot_is_structural() {
        // Baking a value into the source (Lit) vs filling it at
        // runtime (Slot) produces different modules — different hash.
        let a = MaterialExpr::Closure(Closure::Ggx {
            normal: Normal::Flat,
            roughness: Value::Lit(0.35),
        });
        let b = MaterialExpr::Closure(Closure::Ggx {
            normal: Normal::Flat,
            roughness: Value::Slot { slot: 0 },
        });
        assert_ne!(structural_hash(&a), structural_hash(&b));
    }

    #[test]
    fn arena_allocates_in_order() {
        let mut arena = UniformArena::new();
        assert_eq!(arena.alloc(), 0);
        assert_eq!(arena.alloc(), 1);
        assert_eq!(arena.len(), 2);
    }
}
