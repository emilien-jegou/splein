// Single responsibility: the material-DSL runtime vocabulary — real Rust
// types, not macro syntax. Constructors build `MaterialExpr` trees at
// component evaluation time; lowering to WGSL happens in `.into_surface()`
// (mount), cached by structural source identity. Phase-gated nodes panic
// with their gate phase — the Rust type system + explicit gates, not macro
// spans.
//
// Structure vs value (load-bearing, unchanged): the tree shape decides the
// generated WGSL; every numeric rides uniforms, refilled per evaluation
// with zero recompile. Hover varies values, never structure.

use crate::foundation::{
    BackdropNeed, CustomEffect, EffectStack,
};

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// One numeric parameter. Every user-written number arrives here as a
/// runtime uniform slot (`Dyn`) — no numeric reaches WGSL text (the old
/// macro rule D-M10b, now enforced by construction: there is no `Lit`
/// constructor reachable from user code). `Lit` exists only for internal
/// degenerate defaults (the placeholder lobe's roughness 1.0 — structural
/// identity, never a user numeric).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Lit(f32),
    Dyn(f32),
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Dyn(v)
    }
}

// ---------------------------------------------------------------------------
// Normals
// ---------------------------------------------------------------------------

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
    /// `|geo|` host normal field (`ctx.normal`).
    Geo,
}

impl Normal {
    /// `Normal::flat()` — the `(0,0,1)` baseline.
    pub fn flat() -> Self {
        Normal::Flat
    }
}

impl std::ops::Add for Normal {
    type Output = Normal;
    fn add(self, rhs: Normal) -> Normal {
        Normal::Sum(Box::new(self), Box::new(rhs))
    }
}

/// Lens profile constructors (height-field family): these build the
/// `Normal` tree directly — desugar at construction time, no macro.
pub struct Lens;
impl Lens {
    pub fn group(gain: impl Into<Value>) -> Normal {
        Normal::Card { gain: gain.into() }
    }
    pub fn beveled(gain: impl Into<Value>, radius_px: impl Into<Value>) -> Normal {
        Normal::Beveled {
            gain: gain.into(),
            radius_px: radius_px.into(),
        }
    }
    pub fn meniscus(radius_px: impl Into<Value>, curvature: impl Into<Value>) -> Normal {
        Normal::Meniscus {
            radius_px: radius_px.into(),
            curvature: curvature.into(),
        }
    }
    pub fn fluid_tension(radius_px: impl Into<Value>) -> Normal {
        Normal::FluidTension {
            radius_px: radius_px.into(),
        }
    }
}

/// Bump constructors (procedural height fields).
pub struct Bump;
impl Bump {
    pub fn ribbed(freq_per_px: impl Into<Value>) -> Normal {
        Normal::Ribbed {
            freq_per_px: freq_per_px.into(),
        }
    }
    pub fn perlin_noise(scale: impl Into<Value>) -> Normal {
        Normal::Perlin {
            scale: scale.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit wrappers (always uniform-bound — every numeric rides a slot)
// ---------------------------------------------------------------------------

macro_rules! f32_wrapper {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $name(pub f32);
        impl From<$name> for Value {
            fn from(v: $name) -> Self {
                Value::Dyn(v.0)
            }
        }
        impl From<f32> for $name {
            fn from(v: f32) -> Self {
                $name(v)
            }
        }
    };
}

f32_wrapper!(Roughness, "Roughness value wrapper.");
f32_wrapper!(Gain, "Environment gain wrapper.");
f32_wrapper!(Spread, "Dispersion spread wrapper.");
f32_wrapper!(Thickness, "Slab thickness wrapper.");
f32_wrapper!(Absorb, "Absorption density wrapper.");
f32_wrapper!(Density, "Beer density wrapper.");
f32_wrapper!(Scale, "Noise scale wrapper.");
f32_wrapper!(Ior, "Index-of-refraction wrapper.");
f32_wrapper!(Spacing, "Emission grid spacing wrapper.");
f32_wrapper!(Width, "Emission grid line-width wrapper.");
f32_wrapper!(Offset, "Pixel offset wrapper.");
f32_wrapper!(RadiusPx, "Radius wrapper.");
f32_wrapper!(Intensity, "Luminance scale wrapper.");

impl Intensity {
    pub fn factor(v: f32) -> Self {
        Intensity(v)
    }
    pub fn lumens(v: f32) -> Self {
        Intensity(v)
    }
}

impl Roughness {
    pub fn value(v: f32) -> Self {
        Roughness(v)
    }
    pub fn x(v: f32) -> Self {
        Roughness(v)
    }
    pub fn y(v: f32) -> Self {
        Roughness(v)
    }
}
impl Spread {
    pub fn px(v: f32) -> Self {
        Spread(v)
    }
}
impl Thickness {
    pub fn px(v: f32) -> Self {
        Thickness(v)
    }
    pub fn nm(v: f32) -> Self {
        Thickness(v)
    }
}
impl Gain {
    pub fn gain(v: f32) -> Self {
        Gain(v)
    }
}
impl Scale {
    pub fn factor(v: f32) -> Self {
        Scale(v)
    }
}
impl Density {
    pub fn factor(v: f32) -> Self {
        Density(v)
    }
}
impl Spacing {
    pub fn px(v: f32) -> Self {
        Spacing(v)
    }
}
impl Width {
    pub fn px(v: f32) -> Self {
        Width(v)
    }
}
impl Offset {
    pub fn px(v: f32) -> Self {
        Offset(v)
    }
}
impl RadiusPx {
    pub fn px(v: f32) -> Self {
        RadiusPx(v)
    }
}

/// Curvature direction wrapper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curvature(pub f32);
impl Curvature {
    pub fn convex(k: f32) -> Self {
        Curvature(k)
    }
}
impl From<Curvature> for Value {
    fn from(v: Curvature) -> Self {
        Value::Dyn(v.0)
    }
}

/// Complex IOR pair for conductors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComplexIor {
    pub kr: f32,
    pub ki: f32,
}
impl ComplexIor {
    pub fn new(kr: f32, ki: f32) -> Self {
        Self { kr, ki }
    }
}

// ---------------------------------------------------------------------------
// Closures
// ---------------------------------------------------------------------------

/// A closure: one light-transport term evaluated per pixel.
#[derive(Clone, Debug, PartialEq)]
pub enum Closure {
    Warp { frequency: Value, amplitude: Value },
    Transmission {
        normal: Normal,
        ior: Value,
        frost: Value,
    },
    Ggx { normal: Normal, roughness: Value },
    Dispersion {
        normal: Normal,
        base_ior: Value,
        spread_px: Value,
    },
    Slab {
        normal: Normal,
        thickness_px: Value,
        tint_r: Value,
        tint_g: Value,
        tint_b: Value,
        absorb: Value,
    },
    Beckmann { normal: Normal, roughness: Value },
    Anisotropic {
        normal: Normal,
        roughness_x: Value,
        roughness_y: Value,
    },
    Conductor {
        normal: Normal,
        kr: Value,
        ki: Value,
        roughness: Value,
    },
    Sheen { normal: Normal, roughness: Value },
    Burley { normal: Normal, roughness: Value },
    EmissionGrid {
        spacing_px: Value,
        width_px: Value,
    },
}

// ---------------------------------------------------------------------------
// Terms
// ---------------------------------------------------------------------------

/// Environment lighting term.
#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    pub gain: Value,
}

/// The environment.
pub struct Sky;
impl Sky {
    pub fn gain(g: impl Into<Value>) -> Env {
        Env { gain: g.into() }
    }
}

/// A modulator: scales or attenuates a term.
#[derive(Clone, Debug, PartialEq)]
pub enum Modulator {
    Color { r: Value, g: Value, b: Value },
    Beer {
        r: Value,
        g: Value,
        b: Value,
        density: Value,
    },
    ThinFilm {
        thickness_nm: Value,
        film_ior: Value,
    },
    Intensity { lumens: Value },
}

/// Blend weight for `mix`.
#[derive(Clone, Debug, PartialEq)]
pub enum Weight {
    Fresnel { normal: Normal, ior: Value },
    Scalar(Value),
    GeoField(GeoField),
}

impl From<f32> for Weight {
    fn from(v: f32) -> Self {
        Weight::Scalar(Value::Dyn(v))
    }
}

/// Per-pixel host scalar fields (the `|geo|` vocabulary, no macro):
/// `geo.edge`, `geo.thickness`, `geo.position`, `geo.curvature`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoField {
    Edge,
    Thickness,
    Position,
    Curvature,
}

/// Geometry handle: `|geo| geo.edge` builds a `Weight::GeoField`.
/// Passed by value to `|geo|` closures (see `material_block` bridge).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Geo {
    _priv: (),
}

impl Geo {
    pub fn surface() -> Self {
        Geo { _priv: () }
    }
    pub fn normal(self) -> Normal {
        Normal::Geo
    }
    pub fn edge(self) -> Weight {
        Weight::GeoField(GeoField::Edge)
    }
    pub fn thickness(self) -> Weight {
        Weight::GeoField(GeoField::Thickness)
    }
    pub fn position(self) -> Weight {
        Weight::GeoField(GeoField::Position)
    }
    pub fn curvature(self) -> Weight {
        Weight::GeoField(GeoField::Curvature)
    }
}

/// Schlick weight.
pub fn fresnel(normal: Normal, ior: impl Into<Value>) -> Weight {
    Weight::Fresnel {
        normal,
        ior: ior.into(),
    }
}

/// One material term.
#[derive(Clone, Debug, PartialEq)]
pub enum MaterialExpr {
    Closure(Closure),
    Lit(Closure, Env),
    Modulated(Modulator, Box<MaterialExpr>),
    Sum(Box<MaterialExpr>, Box<MaterialExpr>),
    Mix(Box<MaterialExpr>, Box<MaterialExpr>, Weight),
}

// ---------------------------------------------------------------------------
// Degenerate ambient + factor attachment (`*` semantics)
// ---------------------------------------------------------------------------

/// The placeholder lobe a bare `Color` / bare `Intensity` carries:
/// flat-normal GGX at roughness 1.0 (structural identity — a `Lit`
/// value, never a user numeric).
fn is_degenerate_ambient(e: &MaterialExpr) -> bool {
    matches!(
        e,
        MaterialExpr::Closure(Closure::Ggx {
            normal: Normal::Flat,
            roughness: Value::Lit(r),
        }) if *r == 1.0
    )
}

fn degenerate_ambient() -> MaterialExpr {
    MaterialExpr::Closure(Closure::Ggx {
        normal: Normal::Flat,
        roughness: Value::Lit(1.0),
    })
}

/// `term * Sky::gain(g)` / `Sky::gain(g) * term`: light the term's
/// closures with gain `g`. Distributes over `Sum` (each lobe keeps its
/// own env — factors commute); wraps bare closures in `Lit`.
fn apply_env(term: MaterialExpr, gain: Value) -> MaterialExpr {
    match term {
        MaterialExpr::Closure(c) => MaterialExpr::Lit(c, Env { gain }),
        MaterialExpr::Lit(c, _) => MaterialExpr::Lit(c, Env { gain }),
        MaterialExpr::Sum(a, b) => MaterialExpr::Sum(
            Box::new(apply_env(*a, gain.clone())),
            Box::new(apply_env(*b, gain)),
        ),
        MaterialExpr::Mix(a, b, w) => MaterialExpr::Mix(
            Box::new(apply_env(*a, gain.clone())),
            Box::new(apply_env(*b, gain)),
            w,
        ),
        MaterialExpr::Modulated(m, inner) => {
            MaterialExpr::Modulated(m, Box::new(apply_env(*inner, gain)))
        }
    }
}

/// `Color * term`: if one side carries the degenerate ambient lobe
/// (bare `Color` / bare `Intensity` in term position), its modulator
/// re-wraps the real term; otherwise the two nest into a sum (factors
/// commute, grouping unobservable).
fn attach_color(color_side: MaterialExpr, term_side: MaterialExpr) -> MaterialExpr {
    match (color_side, term_side) {
        (MaterialExpr::Modulated(m, inner), t) if is_degenerate_ambient(&inner) => {
            MaterialExpr::Modulated(m, Box::new(t))
        }
        (t, MaterialExpr::Modulated(m, inner)) if is_degenerate_ambient(&inner) => {
            MaterialExpr::Modulated(m, Box::new(t))
        }
        (a, b) => MaterialExpr::Sum(Box::new(a), Box::new(b)),
    }
}

// ---------------------------------------------------------------------------
// Closure constructors
// ---------------------------------------------------------------------------

/// Isotropic GGX specular.
pub struct Microfacet;
impl Microfacet {
    pub fn ggx(normal: Normal, roughness: impl Into<Value>) -> MaterialExpr {
        MaterialExpr::Closure(Closure::Ggx {
            normal,
            roughness: roughness.into(),
        })
    }
    pub fn beckmann(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Microfacet::beckmann` lowers with extended closures");
    }
    pub fn anisotropic(
        _normal: Normal,
        _ax: impl Into<Value>,
        _ay: impl Into<Value>,
    ) -> MaterialExpr {
        panic!(
            "material DSL (Phase 6): `Microfacet::anisotropic` lowers with extended closures"
        );
    }
}

/// Refraction transmission constructor (Phase 5a gate — the type system enforces:
/// this panics until the optics land). `Transmission::new` — named to avoid
/// colliding with the transport-internal `effect::Refract` item marker.
pub struct Transmission;
impl Transmission {
    pub fn new(_normal: Normal, _ior: impl Into<Value>) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5a): `Transmission::new` lowers with displaced backdrop reads + frost blur"
        );
    }
    pub fn frost(self, _r: impl Into<Value>) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5a): `Transmission` lowers with displaced backdrop reads + frost blur"
        );
    }
}

/// Beer absorption (Phase 5c gate).
pub struct Absorption;
impl Absorption {
    pub fn beer(_color: crate::foundation::Color, _density: impl Into<Value>) -> Modulator {
        panic!("material DSL (Phase 5c): `Absorption::beer` lowers with displaced reads");
    }
}

/// Thin-film interference (Phase 6 gate).
pub struct ThinFilm;
impl ThinFilm {
    pub fn interference(_t: impl Into<Value>, _ior: impl Into<Value>) -> Modulator {
        panic!("material DSL (Phase 6): `ThinFilm::interference` lowers with extended closures");
    }
}

/// Grid emission (Phase 6 gate).
pub struct Emission;
impl Emission {
    pub fn grid(_s: impl Into<Value>, _w: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Emission::grid` lowers with extended closures");
    }
}

/// Complex-IOR conductor (Phase 6 gate).
pub struct Conductor;
impl Conductor {
    pub fn new(_normal: Normal, _ior: ComplexIor, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Conductor::new` lowers with extended closures");
    }
}

/// Charlie sheen (Phase 6 gate).
pub struct Sheen;
impl Sheen {
    pub fn charlie(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Sheen::charlie` lowers with extended closures");
    }
}

/// Burley diffuse (Phase 6 gate).
pub struct Diffuse;
impl Diffuse {
    pub fn burley(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Diffuse::burley` lowers with extended closures");
    }
}

// ---------------------------------------------------------------------------
// Surface: the lowered, shippable value
// ---------------------------------------------------------------------------

/// A lowered surface: WGSL source + uniform values, ready to ship as a
/// `CustomEffect`. Built by `.into_surface()` on any `MaterialExpr`.
/// Phase-gated nodes panic at build time with their gate phase.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub wgsl: String,
    pub uniforms: Vec<f32>,
    pub backdrop: BackdropNeed,
    pub pad: Option<f32>,
}

impl Surface {
    /// Structural hash: pipeline-cache key input (source identity).
    pub fn source_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in self.wgsl.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    /// Effect identity: source + uniform values (repaint detection).
    pub fn effect_id(&self) -> u64 {
        let mut h: u64 = self.source_hash();
        for u in &self.uniforms {
            h ^= u64::from(u.to_bits());
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    pub fn into_effect(self) -> CustomEffect {
        let mut e = CustomEffect::new(self.wgsl, self.uniforms, self.backdrop);
        if let Some(p) = self.pad {
            e = e.with_pad(p);
        }
        e
    }

    pub fn into_stack(self) -> EffectStack {
        EffectStack::from(self.into_effect())
    }
}

// ---------------------------------------------------------------------------
// Operators: `*` + `mix`
// ---------------------------------------------------------------------------

/// `Color * closure` — tint a term. A bare `Color` in term position
/// carries the degenerate ambient; `attach_color` replaces the inner
/// lobe so the color lands on the real term.
impl std::ops::Mul<MaterialExpr> for crate::foundation::Color {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        let tinted = MaterialExpr::Modulated(
            Modulator::Color {
                r: Value::Dyn(self.r),
                g: Value::Dyn(self.g),
                b: Value::Dyn(self.b),
            },
            Box::new(degenerate_ambient()),
        );
        attach_color(tinted, rhs)
    }
}

impl std::ops::Mul<MaterialExpr> for Intensity {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        let scaled = MaterialExpr::Modulated(
            Modulator::Intensity {
                lumens: Value::Dyn(self.0),
            },
            Box::new(degenerate_ambient()),
        );
        attach_color(scaled, rhs)
    }
}

impl std::ops::Mul<MaterialExpr> for Env {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        apply_env(rhs, self.gain)
    }
}

/// `closure * Sky::gain` — light a term.
impl std::ops::Mul<Env> for MaterialExpr {
    type Output = MaterialExpr;
    fn mul(self, env: Env) -> MaterialExpr {
        apply_env(self, env.gain)
    }
}

/// `term * scalar` — uniform scale (value-level hover).
impl std::ops::Mul<f32> for MaterialExpr {
    type Output = MaterialExpr;
    fn mul(self, k: f32) -> MaterialExpr {
        MaterialExpr::Modulated(
            Modulator::Intensity {
                lumens: Value::Dyn(k),
            },
            Box::new(self),
        )
    }
}

/// `term + term` — radiance sum.
impl std::ops::Add for MaterialExpr {
    type Output = MaterialExpr;
    fn add(self, rhs: MaterialExpr) -> MaterialExpr {
        MaterialExpr::Sum(Box::new(self), Box::new(rhs))
    }
}

impl MaterialExpr {
    /// `.into_surface()` terminal — lower to WGSL + uniforms.
    pub fn into_surface(self) -> Surface {
        lower_material(&self)
    }
}

/// `mix(a, b, w)` — blend two terms.
pub fn mix(a: MaterialExpr, b: MaterialExpr, w: impl Into<Weight>) -> MaterialExpr {
    MaterialExpr::Mix(Box::new(a), Box::new(b), w.into())
}

// ---------------------------------------------------------------------------
// Warp + Slab + Dispersion
// ---------------------------------------------------------------------------

impl crate::foundation::Warp {
    /// Lower this warp stage to a shippable `Surface` (whole-stage module,
    /// `ReadSharp` + pad = amplitude).
    pub fn into_surface(self) -> Surface {
        lower_warp(self.frequency, self.amplitude)
    }
}

/// 3-wavelength dispersion preset (Phase 5a gate).
pub struct Dispersion;
impl Dispersion {
    pub fn prism(_spread: impl Into<Value>) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5a): `Dispersion::prism` lowers with the 3-wavelength Refract split"
        );
    }
}

/// Slab preset (Phase 5c gate).
pub struct Slab;
impl Slab {
    pub fn thick(
        _thickness: impl Into<Value>,
        _tint: crate::foundation::Color,
        _absorb: impl Into<Value>,
    ) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5c): `Slab::thick` lowers with refraction + Beer absorption"
        );
    }
}

// ---------------------------------------------------------------------------
// Lowering (runtime — moved out of the macro)
// ---------------------------------------------------------------------------

fn uniform_ref(slot: u32) -> String {
    format!("u.data[{}][{}]", slot / 4, slot % 4)
}

struct LowerCx {
    uniforms: Vec<f32>,
}

impl LowerCx {
    fn value(&mut self, v: &Value) -> String {
        match v {
            Value::Lit(f) => format!("{f:?}"),
            Value::Dyn(f) => {
                let slot = self.uniforms.len() as u32;
                self.uniforms.push(*f);
                uniform_ref(slot)
            }
        }
    }

    fn normal(&mut self, n: &Normal) -> String {
        match n {
            Normal::Flat => "vec3<f32>(0.0, 0.0, 1.0)".to_string(),
            Normal::Geo => "ctx.normal".to_string(),
            Normal::Card { gain } => {
                let g = self.value(gain);
                format!("(lens_card_normal(ctx.px, ({g})))")
            }
            Normal::Beveled { gain, radius_px } => {
                let g = self.value(gain);
                let r = self.value(radius_px);
                format!("(lens_beveled_normal(ctx.px, ({g}), ({r})))")
            }
            Normal::Meniscus {
                radius_px,
                curvature,
            } => {
                let r = self.value(radius_px);
                let k = self.value(curvature);
                format!("(lens_meniscus_normal(ctx.px, ({r}), ({k})))")
            }
            Normal::FluidTension { radius_px } => {
                let r = self.value(radius_px);
                format!("(lens_fluid_normal(ctx.px, ({r})))")
            }
            Normal::Ribbed { freq_per_px } => {
                let f = self.value(freq_per_px);
                format!("(bump_ribbed_normal(ctx.px, ({f})))")
            }
            Normal::Perlin { scale } => {
                let s = self.value(scale);
                format!("(bump_perlin_normal(ctx.px, ({s})))")
            }
            Normal::Sum(a, b) => {
                let a = self.normal(a);
                let b = self.normal(b);
                format!("(normalize(({a}) + ({b})))")
            }
        }
    }

    fn closure(&mut self, c: &Closure, env: &str) -> String {
        match c {
            Closure::Warp { .. } => {
                panic!(
                    "material DSL (Phase 4.5): `Warp::sine(...)` is a whole-surface stage — \
                     it cannot nest inside `*`, `+`, `mix(...)`; \
                     author the warp alone: `Warp::sine(...).into_surface()`"
                )
            }
            Closure::Ggx { normal, roughness } => {
                let n = self.normal(normal);
                let a = self.value(roughness);
                format!("(ggx_spec(vec3<f32>(0.0, 0.0, 1.0), ({n}), ({a})) * ({env}))")
            }
            Closure::Transmission { .. } => {
                panic!("material DSL (Phase 5a): `Transmission` lowers with displaced backdrop reads")
            }
            Closure::Dispersion { .. } => {
                panic!("material DSL (Phase 5a): `Dispersion::prism` lowers with the 3-wavelength split")
            }
            Closure::Slab { .. } => {
                panic!("material DSL (Phase 5c): `Slab::thick` lowers with refraction + Beer absorption")
            }
            _ => {
                panic!("material DSL (Phase 6): extended closure lowers with extended closures")
            }
        }
    }

    fn modulator(&mut self, m: &Modulator, inner: &str) -> String {
        match m {
            Modulator::Color { r, g, b } => {
                let (r, g, b) = (self.value(r), self.value(g), self.value(b));
                format!("(vec3<f32>(({r}), ({g}), ({b})) * ({inner}))")
            }
            Modulator::Intensity { lumens } => {
                let k = self.value(lumens);
                format!("(({inner}) * ({k}))")
            }
            Modulator::Beer { .. } => {
                panic!("material DSL (Phase 5c): `Absorption::beer` lowers with displaced reads")
            }
            Modulator::ThinFilm { .. } => {
                panic!("material DSL (Phase 6): `ThinFilm::interference` lowers with extended closures")
            }
        }
    }

    fn weight(&mut self, w: &Weight) -> String {
        match w {
            Weight::Scalar(v) => format!("({})", self.value(v)),
            Weight::Fresnel { normal, ior } => {
                let n = self.normal(normal);
                let ior = self.value(ior);
                format!("(fresnel_schlick(({n}).z, ({ior})))")
            }
            Weight::GeoField(f) => match f {
                GeoField::Edge => "(1.0 - ctx.color.a)".to_string(),
                GeoField::Thickness => "(ctx.path_length)".to_string(),
                GeoField::Position => "(fract(length(ctx.px) / 80.0))".to_string(),
                GeoField::Curvature => "(length(ctx.normal.xy))".to_string(),
            },
        }
    }

    fn term(&mut self, expr: &MaterialExpr) -> String {
        match expr {
            MaterialExpr::Closure(c) => self.closure(c, "1.0"),
            MaterialExpr::Lit(c, env) => {
                let g = self.value(&env.gain);
                self.closure(c, &g)
            }
            MaterialExpr::Modulated(m, inner) => {
                let inner = self.term(inner);
                self.modulator(m, &inner)
            }
            MaterialExpr::Sum(a, b) => {
                let (a, b) = (self.term(a), self.term(b));
                format!("(({a}) + ({b}))")
            }
            MaterialExpr::Mix(a, b, w) => {
                let (a, b, w) = (self.term(a), self.term(b), self.weight(w));
                format!("mix(({a}), ({b}), ({w}))")
            }
        }
    }
}

const PRELUDE_WGSL: &str = r#"
fn fresnel_schlick(nov: f32, ior: f32) -> f32 {
    let f0 = (ior - 1.0) / (ior + 1.0);
    let f0_2 = f0 * f0;
    let t = clamp(1.0 - nov, 0.0, 1.0);
    return f0_2 + (1.0 - f0_2) * t * t * t * t * t;
}

fn ggx_spec(v: vec3<f32>, n: vec3<f32>, roughness: f32) -> vec3<f32> {
    let a = max(roughness, 0.001);
    let a2 = a * a;
    let nov = max(dot(n, v), 0.0);
    let d = (a2 - 1.0) * nov * nov + 1.0;
    let ndf = a2 / (3.14159265 * d * d);
    let f0 = vec3<f32>(0.04);
    let f = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - nov, 5.0);
    return f * ndf * nov;
}
"#;

fn lower_material(expr: &MaterialExpr) -> Surface {
    let mut cx = LowerCx { uniforms: vec![] };
    let radiance = cx.term(expr);
    let source = format!(
        "{PRELUDE_WGSL}\nfn apply(\n    ctx: EffectCtx,\n    backdrop: texture_2d<f32>,\n    sharp: texture_2d<f32>,\n    u: CustomUniforms,\n) -> EffectCtx {{\n    var out = ctx;\n    let card_a = ctx.color.a;\n    let radiance: vec3<f32> = {radiance};\n    out.color = vec4<f32>(radiance * card_a, card_a);\n    return out;\n}}\n"
    );
    Surface {
        wgsl: source,
        uniforms: cx.uniforms,
        backdrop: BackdropNeed::None,
        pad: None,
    }
}

fn lower_warp(frequency: f32, amplitude: f32) -> Surface {
    Surface {
        wgsl: crate::foundation::Warp::WARP_WGSL.to_string(),
        uniforms: vec![amplitude, frequency],
        backdrop: BackdropNeed::ReadSharp,
        pad: Some(amplitude.max(0.0)),
    }
}

// ---------------------------------------------------------------------------
// Effect plumbing: Surface rides the stack transport
// ---------------------------------------------------------------------------

use crate::reactive::IntoProp as _MaterialIntoProp;
use crate::reactive::Prop as _MaterialProp;

impl crate::foundation::IntoEffect for Surface {
    fn into_effect(self) -> Box<dyn crate::foundation::Effect> {
        Box::new(self.into_effect())
    }
}

impl _MaterialIntoProp<EffectStack> for Surface {
    fn into_prop(self) -> _MaterialProp<EffectStack> {
        _MaterialProp::Static(self.into_stack())
    }
}

impl crate::reactive::IntoPropValue<EffectStack> for Surface {
    fn into_prop_value(self) -> _MaterialProp<EffectStack> {
        _MaterialProp::Static(self.into_stack())
    }
}

impl crate::reactive::IntoPropValue<EffectStack> for EffectStack {
    fn into_prop_value(self) -> _MaterialProp<EffectStack> {
        _MaterialProp::Static(self)
    }
}

/// `move || ... .into_surface()` reactive form: covered by the blanket
/// `Fn() -> T` impl in `reactive::prop` — re-runs the closure every
/// evaluation into `Prop::Dynamic`, so signal reads inside refill uniforms
/// with zero recompile. No impl needed here.

/// `|geo| ... .into_surface()` block form: wrap the closure in `GeoFn`
/// (`material={GeoFn(|geo| ... .into_surface())}` — `view!` inserts the
/// wrapper automatically for `|geo|` literals) — the wrapper calls it
/// with `Geo::surface()` once at component time into a static prop.
/// Per-pixel `geo.*` fields ride the shader, not reactivity.
/// (A bare `|geo|` closure cannot impl `IntoProp` directly: it collides
/// with the blanket `Fn() -> T` impl — coherence forces the wrapper.)
pub struct GeoFn<F: Fn(Geo) -> Surface>(pub F);
impl<F: Fn(Geo) -> Surface + 'static> _MaterialIntoProp<EffectStack> for GeoFn<F> {
    fn into_prop(self) -> _MaterialProp<EffectStack> {
        _MaterialProp::Static(self.0(Geo::surface()).into_stack())
    }
}
