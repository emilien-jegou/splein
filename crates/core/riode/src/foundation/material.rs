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

use crate::foundation::{BackdropNeed, CustomEffect, EffectStack};

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
    /// A per-pixel geometry field read shader-side (the `|geo|`
    /// vocabulary) instead of a uniform slot: `Geo::radius()` lowers
    /// here, so a numeric built from the card's resolved corner radius
    /// never occupies a uniform and never bakes WGSL — editing the
    /// box's `radius` prop edits the shader value through the uniform
    /// refill, with zero recompile.
    GeoField(GeoField),
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
///
/// V3 (minimal set): `Flat` baseline, `FromHeight` (any [`Field`] drives a
/// normal via gradients), `Geo` (host `ctx.normal`), `Perturbed` (layer two
/// fields). The old `Lens::*` / `Bump::*` namespaces are deprecated shims
/// below — they forward here, so there is one normal type, not three.
#[derive(Clone, Debug, PartialEq)]
pub enum Normal {
    /// `(0,0,1)` baseline.
    Flat,
    /// Normal derived from a scalar [`Field`] (height/pattern) at `strength`.
    FromHeight { field: Field, strength: Value },
    /// `|geo|` host normal field (`ctx.normal`).
    Geo,
    /// Layer two normals (tangent slopes add, renormalized).
    Perturbed(Box<Normal>, Box<Normal>),
    // --- Deprecated structure (kept compiling, lowers like V3) ---
    /// Deprecated: use `Normal::from_height(Field::card(..))`.
    Card { gain: Value },
    /// Deprecated: use `geo.bevel()` / `Normal::from_height`.
    Beveled { gain: Value, radius_px: Value },
    /// Deprecated: use `Normal::from_height`.
    Meniscus { radius_px: Value, curvature: Value },
    /// Deprecated: use `Normal::from_height`.
    FluidTension { radius_px: Value },
    /// Deprecated: use `Normal::from_height(Field::sine(..))`.
    Ribbed { freq_per_px: Value },
    /// Deprecated: use `Normal::from_height(Field::noise(..))`.
    Perlin { scale: Value },
    /// Deprecated: use `Perturbed`.
    Sum(Box<Normal>, Box<Normal>),
}

/// Scalar field (pattern / height / mask): `f(uv) -> f32`.
///
/// V3 primitive: one field type drives normals (`Normal::from_height`),
/// roughness, emission masks, and coverage — instead of one bespoke type
/// per use (`Bump::ribbed`, `Emission::grid`, …).
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Constant(Value),
    Sine {
        freq_per_px: Value,
        vertical: bool,
    },
    Noise {
        scale: Value,
    },
    Grid {
        spacing_px: Value,
        width_px: Value,
    },
    /// Card-local distance to the card edge in px (host-supplied when the
    /// Phase 5 host passes origin/dims; today lowers from `ctx.px` like
    /// the other geo fields).
    DistanceToEdge,
}

impl Field {
    pub fn constant(v: impl Into<Value>) -> Self {
        Field::Constant(v.into())
    }
    pub fn sine(freq_per_px: impl Into<Value>, vertical: bool) -> Self {
        Field::Sine {
            freq_per_px: freq_per_px.into(),
            vertical,
        }
    }
    pub fn noise(scale: impl Into<Value>) -> Self {
        Field::Noise {
            scale: scale.into(),
        }
    }
    pub fn grid(spacing_px: impl Into<Value>, width_px: impl Into<Value>) -> Self {
        Field::Grid {
            spacing_px: spacing_px.into(),
            width_px: width_px.into(),
        }
    }
    pub fn edge_dist() -> Self {
        Field::DistanceToEdge
    }
}

impl Normal {
    /// `Normal::flat()` — the `(0,0,1)` baseline.
    pub fn flat() -> Self {
        Normal::Flat
    }
    /// Derive a normal from any [`Field`] at `strength`.
    pub fn from_height(field: Field, strength: impl Into<Value>) -> Self {
        Normal::FromHeight {
            field,
            strength: strength.into(),
        }
    }
}

impl std::ops::Add for Normal {
    type Output = Normal;
    fn add(self, rhs: Normal) -> Normal {
        Normal::Perturbed(Box::new(self), Box::new(rhs))
    }
}

/// Deprecated: `Lens::*` forwarded to the unified [`Normal`].
/// Use `Normal::from_height(..)` or `geo.bevel(..)` instead.
pub struct Lens;
impl Lens {
    #[deprecated(note = "use Normal::from_height(Field::card/edge(..)) or geo.bevel()")]
    pub fn group(gain: impl Into<Value>) -> Normal {
        Normal::Card { gain: gain.into() }
    }
    #[deprecated(note = "use geo.bevel(radius)")]
    pub fn beveled(gain: impl Into<Value>, radius_px: impl Into<Value>) -> Normal {
        Normal::Beveled {
            gain: gain.into(),
            radius_px: radius_px.into(),
        }
    }
    #[deprecated(note = "use Normal::from_height(..)")]
    pub fn meniscus(radius_px: impl Into<Value>, curvature: impl Into<Value>) -> Normal {
        Normal::Meniscus {
            radius_px: radius_px.into(),
            curvature: curvature.into(),
        }
    }
    #[deprecated(note = "use Normal::from_height(..)")]
    pub fn fluid_tension(radius_px: impl Into<Value>) -> Normal {
        Normal::FluidTension {
            radius_px: radius_px.into(),
        }
    }
}

/// Deprecated: `Bump::*` forwarded to [`Field`] + [`Normal::from_height`].
/// Use `Normal::from_height(Field::sine/noise(..))` instead.
pub struct Bump;
impl Bump {
    #[deprecated(note = "use Normal::from_height(Field::sine(..))")]
    pub fn ribbed(freq_per_px: impl Into<Value>) -> Normal {
        Normal::Ribbed {
            freq_per_px: freq_per_px.into(),
        }
    }
    #[deprecated(note = "use Normal::from_height(Field::noise(..))")]
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
// Closures — V3 minimal set (operators discipline)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Closure {
    Warp {
        frequency: Value,
        amplitude: Value,
    },
    /// Opaque unlit plaque: bare `Color` stays on screen, no PBR lobe.
    Unlit {
        r: Value,
        g: Value,
        b: Value,
    },
    /// Transmission: backdrop sample with optional blur, spectral spread,
    /// and Beer's-law attenuation.
    Transmit {
        normal: Normal,
        ior: Value,
        frost: Value,
        dispersion: Value,
        atten_r: Value,
        atten_g: Value,
        atten_b: Value,
        atten_density: Value,
    },
    /// Microfacet specular (isotropic GGX or anisotropic), lit by `env_gain`.
    Specular {
        normal: Normal,
        distribution: SpecularDist,
        fresnel: Fresnel,
        env_gain: Value,
    },
    /// Dielectric interface: one IOR, Schlick mix of reflection and transmission.
    Interface {
        normal: Normal,
        ior: Value,
        frost: Value,
        dispersion: Value,
        atten_r: Value,
        atten_g: Value,
        atten_b: Value,
        atten_density: Value,
        roughness: Value,
        env_gain: Value,
        light: Option<Light>,
    },
    /// Emission: color × gain, optionally masked by a [`Field`].
    Emission {
        r: Value,
        g: Value,
        b: Value,
        gain: Value,
        mask: Option<Field>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fresnel {
    Dielectric { ior: Value },
    Conductor { n: Value, k: Value },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpecularDist {
    Isotropic { roughness: Value },
    Anisotropic { rx: Value, ry: Value },
}

// ---------------------------------------------------------------------------
// Terms
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    pub gain: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Light {
    Directional { x: Value, y: Value, z: Value },
    Point { x: Weight, y: Weight, height: Value },
}

pub struct Lights;
impl Lights {
    pub fn directional(dir: (f32, f32, f32)) -> Light {
        Light::Directional {
            x: Value::Dyn(dir.0),
            y: Value::Dyn(dir.1),
            z: Value::Dyn(dir.2),
        }
    }
    pub fn point(x: Weight, y: Weight, height: impl Into<Value>) -> Light {
        Light::Point {
            x,
            y,
            height: height.into(),
        }
    }
    pub fn point_xy(x: impl Into<Value>, y: impl Into<Value>, height: impl Into<Value>) -> Light {
        Light::Point {
            x: Weight::Scalar(x.into()),
            y: Weight::Scalar(y.into()),
            height: height.into(),
        }
    }
}

pub struct Sky;
impl Sky {
    pub fn gain(g: impl Into<Value>) -> Env {
        Env { gain: g.into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modulator {
    Color {
        r: Value,
        g: Value,
        b: Value,
    },
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
    Intensity {
        lumens: Value,
    },
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoField {
    Edge,
    Thickness,
    Position,
    Curvature,
    Uv,
    UvY,
    LocalPx,
    LocalPxY,
    EdgeDist,
    PointerPxX,
    PointerPxY,
    PointerUvX,
    PointerUvY,
    VelocityX,
    VelocityY,
    Radius,
}

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
    pub fn bevel(self, radius: impl Into<Value>) -> Normal {
        Normal::FromHeight {
            field: Field::DistanceToEdge,
            strength: radius.into(),
        }
    }
    pub fn uv(self) -> (Weight, Weight) {
        (
            Weight::GeoField(GeoField::Uv),
            Weight::GeoField(GeoField::UvY),
        )
    }
    pub fn local_px(self) -> (Weight, Weight) {
        (
            Weight::GeoField(GeoField::LocalPx),
            Weight::GeoField(GeoField::LocalPxY),
        )
    }
    pub fn edge_dist(self) -> Weight {
        Weight::GeoField(GeoField::EdgeDist)
    }
    pub fn radius(self) -> Value {
        Value::GeoField(GeoField::Radius)
    }
    pub fn pointer_px(self) -> (Weight, Weight) {
        (
            Weight::GeoField(GeoField::PointerPxX),
            Weight::GeoField(GeoField::PointerPxY),
        )
    }
    pub fn pointer_uv(self) -> (Weight, Weight) {
        (
            Weight::GeoField(GeoField::PointerUvX),
            Weight::GeoField(GeoField::PointerUvY),
        )
    }
    pub fn velocity(self) -> (Weight, Weight) {
        (
            Weight::GeoField(GeoField::VelocityX),
            Weight::GeoField(GeoField::VelocityY),
        )
    }
    pub fn edge(self) -> Weight {
        Weight::GeoField(GeoField::Edge)
    }
    pub fn thickness(self) -> Weight {
        Weight::GeoField(GeoField::Thickness)
    }
    #[deprecated(note = "use geo.uv() / geo.local_px()")]
    pub fn position(self) -> Weight {
        Weight::GeoField(GeoField::Position)
    }
    pub fn curvature(self) -> Weight {
        Weight::GeoField(GeoField::Curvature)
    }
}

pub fn fresnel(normal: Normal, ior: impl Into<Value>) -> Weight {
    Weight::Fresnel {
        normal,
        ior: ior.into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaterialExpr {
    Closure(Closure),
    Lit(Closure, Env),
    LitLight(Closure, Light),
    Modulated(Modulator, Box<MaterialExpr>),
    Sum(Box<MaterialExpr>, Box<MaterialExpr>),
    Mix(Box<MaterialExpr>, Box<MaterialExpr>, Weight),
}

impl MaterialExpr {
    pub fn lit_by(self, gain: impl Into<Value>) -> MaterialExpr {
        apply_env(self, gain.into())
    }
    pub fn lit_by_light(self, light: Light) -> MaterialExpr {
        apply_light(self, light)
    }
    pub fn scaled(self, k: impl Into<Value>) -> MaterialExpr {
        MaterialExpr::Modulated(Modulator::Intensity { lumens: k.into() }, Box::new(self))
    }
    pub fn attenuate(
        self,
        color: crate::foundation::Color,
        density: impl Into<Value>,
    ) -> MaterialExpr {
        MaterialExpr::Modulated(
            Modulator::Beer {
                r: Value::Dyn(color.r),
                g: Value::Dyn(color.g),
                b: Value::Dyn(color.b),
                density: density.into(),
            },
            Box::new(self),
        )
    }
}

fn tint(term: MaterialExpr, color: crate::foundation::Color) -> MaterialExpr {
    match term {
        MaterialExpr::Modulated(m, inner) => {
            MaterialExpr::Modulated(m, Box::new(tint(*inner, color)))
        }
        t => MaterialExpr::Modulated(
            Modulator::Color {
                r: Value::Dyn(color.r),
                g: Value::Dyn(color.g),
                b: Value::Dyn(color.b),
            },
            Box::new(t),
        ),
    }
}

fn apply_env(term: MaterialExpr, gain: Value) -> MaterialExpr {
    match term {
        MaterialExpr::Closure(c) => MaterialExpr::Lit(c, Env { gain }),
        MaterialExpr::Lit(c, _) => MaterialExpr::Lit(c, Env { gain }),
        MaterialExpr::LitLight(c, l) => MaterialExpr::LitLight(c, l),
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

fn apply_light(term: MaterialExpr, light: Light) -> MaterialExpr {
    match term {
        MaterialExpr::Closure(c) => MaterialExpr::LitLight(c, light),
        MaterialExpr::Lit(c, _) => MaterialExpr::LitLight(
            c,
            Light::Directional {
                x: Value::Dyn(0.0),
                y: Value::Dyn(0.0),
                z: Value::Dyn(1.0),
            },
        ),
        MaterialExpr::LitLight(c, _) => MaterialExpr::LitLight(c, light),
        MaterialExpr::Sum(a, b) => MaterialExpr::Sum(
            Box::new(apply_light(*a, light.clone())),
            Box::new(apply_light(*b, light)),
        ),
        MaterialExpr::Mix(a, b, w) => MaterialExpr::Mix(
            Box::new(apply_light(*a, light.clone())),
            Box::new(apply_light(*b, light)),
            w,
        ),
        MaterialExpr::Modulated(m, inner) => {
            MaterialExpr::Modulated(m, Box::new(apply_light(*inner, light)))
        }
    }
}

// ---------------------------------------------------------------------------
// V3 closure constructors
// ---------------------------------------------------------------------------

pub struct Transmit;
impl Transmit {
    pub fn sample() -> TransmitBuilder {
        TransmitBuilder {
            normal: Normal::Flat,
            ior: Value::Dyn(1.0),
            frost: Value::Dyn(0.0),
            dispersion: Value::Dyn(0.0),
            atten: None,
        }
    }
}

pub struct TransmitBuilder {
    normal: Normal,
    ior: Value,
    frost: Value,
    dispersion: Value,
    atten: Option<(crate::foundation::Color, Value)>,
}

impl TransmitBuilder {
    pub fn normal(mut self, n: Normal) -> Self {
        self.normal = n;
        self
    }
    pub fn ior(mut self, ior: impl Into<Value>) -> Self {
        self.ior = ior.into();
        self
    }
    pub fn frost(mut self, r: impl Into<Value>) -> Self {
        self.frost = r.into();
        self
    }
    pub fn tint(mut self, c: crate::foundation::Color) -> Self {
        let d = self.atten.map(|(_, d)| d).unwrap_or(Value::Dyn(0.0));
        self.atten = Some((c, d));
        self
    }
    pub fn disperse(mut self, spread_px: impl Into<Value>) -> Self {
        self.dispersion = spread_px.into();
        self
    }
    pub fn attenuate(mut self, color: crate::foundation::Color, density: impl Into<Value>) -> Self {
        self.atten = Some((color, density.into()));
        self
    }
    pub fn build(self) -> MaterialExpr {
        MaterialExpr::Closure(Closure::Transmit {
            normal: self.normal,
            ior: self.ior,
            frost: self.frost,
            dispersion: self.dispersion,
            atten_r: match &self.atten {
                Some((c, _)) => Value::Dyn(c.r),
                None => Value::Dyn(1.0),
            },
            atten_g: match &self.atten {
                Some((c, _)) => Value::Dyn(c.g),
                None => Value::Dyn(1.0),
            },
            atten_b: match &self.atten {
                Some((c, _)) => Value::Dyn(c.b),
                None => Value::Dyn(1.0),
            },
            atten_density: match &self.atten {
                Some((_, d)) => d.clone(),
                None => Value::Dyn(0.0),
            },
        })
    }
    pub fn into_surface(self) -> Surface {
        self.build().into_surface()
    }
}

pub struct Interface;
impl Interface {
    pub fn dielectric(normal: Normal, ior: impl Into<Value>) -> InterfaceBuilder {
        InterfaceBuilder {
            normal,
            ior: ior.into(),
            frost: Value::Dyn(0.0),
            tint: None,
            dispersion: Value::Dyn(0.0),
            atten: None,
            roughness: Value::Dyn(0.02),
            env_gain: Value::Dyn(1.0),
            light: None,
        }
    }
}

pub struct InterfaceBuilder {
    normal: Normal,
    ior: Value,
    frost: Value,
    tint: Option<crate::foundation::Color>,
    dispersion: Value,
    atten: Option<(crate::foundation::Color, Value)>,
    roughness: Value,
    env_gain: Value,
    light: Option<Light>,
}

impl InterfaceBuilder {
    pub fn transmit(mut self, frost: impl Into<Value>) -> Self {
        self.frost = frost.into();
        self
    }
    pub fn tint(mut self, c: crate::foundation::Color) -> Self {
        self.tint = Some(c);
        self
    }
    pub fn disperse(mut self, spread_px: impl Into<Value>) -> Self {
        self.dispersion = spread_px.into();
        self
    }
    pub fn attenuate(mut self, color: crate::foundation::Color, density: impl Into<Value>) -> Self {
        self.atten = Some((color, density.into()));
        self
    }
    pub fn reflect(mut self, roughness: impl Into<Value>) -> Self {
        self.roughness = roughness.into();
        self
    }
    pub fn lit_by(mut self, gain: impl Into<Value>) -> Self {
        self.env_gain = gain.into();
        self
    }
    pub fn lit_by_light(mut self, light: Light) -> Self {
        self.light = Some(light);
        self
    }
    pub fn build(self) -> MaterialExpr {
        let (ar, ag, ab, dens) = match &self.atten {
            Some((c, d)) => (Value::Dyn(c.r), Value::Dyn(c.g), Value::Dyn(c.b), d.clone()),
            None => (
                Value::Dyn(1.0),
                Value::Dyn(1.0),
                Value::Dyn(1.0),
                Value::Dyn(0.0),
            ),
        };
        let tint = self.tint.unwrap_or(crate::foundation::Color::WHITE);
        let iface = MaterialExpr::Closure(Closure::Interface {
            normal: self.normal,
            ior: self.ior,
            frost: self.frost,
            dispersion: self.dispersion,
            atten_r: ar,
            atten_g: ag,
            atten_b: ab,
            atten_density: dens,
            roughness: self.roughness,
            env_gain: self.env_gain,
            light: self.light,
        });
        MaterialExpr::Modulated(
            Modulator::Color {
                r: Value::Dyn(tint.r),
                g: Value::Dyn(tint.g),
                b: Value::Dyn(tint.b),
            },
            Box::new(iface),
        )
    }
    pub fn into_surface(self) -> Surface {
        self.build().into_surface()
    }
}

pub struct Specular;
impl Specular {
    pub fn ggx(normal: Normal, roughness: impl Into<Value>) -> SpecularBuilder {
        SpecularBuilder {
            normal,
            dist: SpecularDist::Isotropic {
                roughness: roughness.into(),
            },
            fresnel: Fresnel::Dielectric {
                ior: Value::Dyn(1.5),
            },
            tint: None,
            env_gain: Value::Dyn(1.0),
            light: None,
        }
    }
    pub fn anisotropic(
        normal: Normal,
        rx: impl Into<Value>,
        ry: impl Into<Value>,
    ) -> SpecularBuilder {
        SpecularBuilder {
            normal,
            dist: SpecularDist::Anisotropic {
                rx: rx.into(),
                ry: ry.into(),
            },
            fresnel: Fresnel::Dielectric {
                ior: Value::Dyn(1.5),
            },
            tint: None,
            env_gain: Value::Dyn(1.0),
            light: None,
        }
    }
    pub fn beckmann(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Specular::beckmann` lowers with extended closures");
    }
}

pub struct SpecularBuilder {
    normal: Normal,
    dist: SpecularDist,
    fresnel: Fresnel,
    tint: Option<crate::foundation::Color>,
    env_gain: Value,
    light: Option<Light>,
}

impl SpecularBuilder {
    pub fn dielectric(mut self, ior: impl Into<Value>) -> Self {
        self.fresnel = Fresnel::Dielectric { ior: ior.into() };
        self
    }
    pub fn conductor(mut self, ior: ComplexIor) -> Self {
        self.fresnel = Fresnel::Conductor {
            n: Value::Dyn(ior.kr),
            k: Value::Dyn(ior.ki),
        };
        self
    }
    pub fn tint(mut self, c: crate::foundation::Color) -> Self {
        self.tint = Some(c);
        self
    }
    pub fn lit_by(mut self, gain: impl Into<Value>) -> Self {
        self.env_gain = gain.into();
        self
    }
    pub fn lit_by_light(mut self, light: Light) -> Self {
        self.light = Some(light);
        self
    }
    pub fn build(self) -> MaterialExpr {
        let (r, g, b) = self
            .tint
            .map(|c| (Value::Dyn(c.r), Value::Dyn(c.g), Value::Dyn(c.b)))
            .unwrap_or((Value::Dyn(1.0), Value::Dyn(1.0), Value::Dyn(1.0)));
        let spec = MaterialExpr::Closure(Closure::Specular {
            normal: self.normal,
            distribution: self.dist,
            fresnel: self.fresnel,
            env_gain: self.env_gain,
        });
        let spec = match self.light {
            Some(l) => MaterialExpr::LitLight(spec_into_closure(spec), l),
            None => spec,
        };
        MaterialExpr::Modulated(Modulator::Color { r, g, b }, Box::new(spec))
    }
    pub fn into_surface(self) -> Surface {
        self.build().into_surface()
    }
}

fn spec_into_closure(e: MaterialExpr) -> Closure {
    match e {
        MaterialExpr::Closure(c) => c,
        _ => panic!("material DSL: internal error — SpecularBuilder wrapped a non-closure"),
    }
}

pub struct Emission;
impl Emission {
    pub fn glow(color: crate::foundation::Color, gain: impl Into<Value>) -> EmissionBuilder {
        EmissionBuilder {
            color,
            gain: gain.into(),
            mask: None,
        }
    }
    pub fn grid(_s: impl Into<Value>, _w: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Emission::grid` is `Emission::glow(..).mask(Field::grid(..))`");
    }
}

pub struct EmissionBuilder {
    color: crate::foundation::Color,
    gain: Value,
    mask: Option<Field>,
}

impl EmissionBuilder {
    pub fn mask(mut self, f: Field) -> Self {
        self.mask = Some(f);
        self
    }
    pub fn build(self) -> MaterialExpr {
        MaterialExpr::Closure(Closure::Emission {
            r: Value::Dyn(self.color.r),
            g: Value::Dyn(self.color.g),
            b: Value::Dyn(self.color.b),
            gain: self.gain,
            mask: self.mask,
        })
    }
    pub fn into_surface(self) -> Surface {
        self.build().into_surface()
    }
}

pub struct Diffuse;
impl Diffuse {
    pub fn burley(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Diffuse::burley` lowers with extended closures");
    }
}

pub struct Sheen;
impl Sheen {
    pub fn charlie(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Sheen::charlie` lowers with extended closures");
    }
}

// ---------------------------------------------------------------------------
// Surface
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub wgsl: String,
    pub uniforms: Vec<f32>,
    pub backdrop: BackdropNeed,
    pub pad: Option<f32>,
}

impl Surface {
    pub fn source_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in self.wgsl.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

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
// Operators
// ---------------------------------------------------------------------------

impl std::ops::Mul<Intensity> for crate::foundation::Color {
    type Output = MaterialExpr;
    fn mul(self, rhs: Intensity) -> MaterialExpr {
        let base: MaterialExpr = self.into();
        base.scaled(rhs.0)
    }
}

impl std::ops::Mul<crate::foundation::Color> for Intensity {
    type Output = MaterialExpr;
    fn mul(self, rhs: crate::foundation::Color) -> MaterialExpr {
        rhs * self
    }
}

impl From<crate::foundation::Color> for MaterialExpr {
    fn from(c: crate::foundation::Color) -> MaterialExpr {
        MaterialExpr::Closure(Closure::Unlit {
            r: Value::Dyn(c.r),
            g: Value::Dyn(c.g),
            b: Value::Dyn(c.b),
        })
    }
}

impl std::ops::Mul<MaterialExpr> for crate::foundation::Color {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        tint(rhs, self)
    }
}

impl std::ops::Mul<crate::foundation::Color> for MaterialExpr {
    type Output = MaterialExpr;
    fn mul(self, rhs: crate::foundation::Color) -> MaterialExpr {
        tint(self, rhs)
    }
}

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

impl std::ops::Mul<MaterialExpr> for Intensity {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        rhs.scaled(self.0)
    }
}

impl std::ops::Mul<Intensity> for MaterialExpr {
    type Output = MaterialExpr;
    fn mul(self, rhs: Intensity) -> MaterialExpr {
        self.scaled(rhs.0)
    }
}

impl std::ops::Mul<MaterialExpr> for Env {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        apply_env(rhs, self.gain)
    }
}

impl std::ops::Mul<Env> for MaterialExpr {
    type Output = MaterialExpr;
    fn mul(self, env: Env) -> MaterialExpr {
        apply_env(self, env.gain)
    }
}

impl std::ops::Mul<MaterialExpr> for f32 {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        rhs * self
    }
}

impl std::ops::Add for MaterialExpr {
    type Output = MaterialExpr;
    fn add(self, rhs: MaterialExpr) -> MaterialExpr {
        MaterialExpr::Sum(Box::new(self), Box::new(rhs))
    }
}

impl MaterialExpr {
    pub fn into_surface(self) -> Surface {
        lower_material(&self)
    }
}

pub fn mix(a: MaterialExpr, b: MaterialExpr, w: impl Into<Weight>) -> MaterialExpr {
    MaterialExpr::Mix(Box::new(a), Box::new(b), w.into())
}

// ---------------------------------------------------------------------------
// Warp
// ---------------------------------------------------------------------------

impl crate::foundation::Warp {
    pub fn into_surface(self) -> Surface {
        lower_warp(self.frequency, self.amplitude)
    }
}

// ---------------------------------------------------------------------------
// Lowering
// ---------------------------------------------------------------------------

const MAX_USER_UNIFORMS: usize = 28;

fn uniform_ref(slot: u32) -> String {
    format!("u.data[{}][{}]", slot / 4, slot % 4)
}

fn card_rect_ref() -> &'static str {
    "u.data[7]"
}

fn card_radius_ref() -> &'static str {
    "u.data[8]"
}

struct LowerCx {
    uniforms: Vec<f32>,
}

impl LowerCx {
    fn value(&mut self, v: &Value) -> String {
        match v {
            Value::Lit(f) => format!("{f:?}"),
            Value::GeoField(field) => self.geo_field_value(*field),
            Value::Dyn(f) => {
                assert!(
                    self.uniforms.len() < MAX_USER_UNIFORMS,
                    "material DSL: expression needs {} user uniforms (cap is {MAX_USER_UNIFORMS}); slots 28..36 are host-reserved",
                    self.uniforms.len() + 1,
                );
                let slot = self.uniforms.len() as u32;
                self.uniforms.push(*f);
                uniform_ref(slot)
            }
        }
    }

    fn geo_field_value(&mut self, field: GeoField) -> String {
        match field {
            GeoField::Radius => {
                "(max(max(u.data[8].x, u.data[8].y), max(u.data[8].z, u.data[8].w)))".to_string()
            }
            other => panic!(
                "material DSL: value-level geo field {other:?} has no scalar lowering (geo.radius() is the only one)"
            ),
        }
    }

    fn normal(&mut self, n: &Normal) -> String {
        match n {
            Normal::Flat => "vec3<f32>(0.0, 0.0, 1.0)".to_string(),
            Normal::Geo => "ctx.normal".to_string(),
            Normal::FromHeight { field, strength } => {
                let s = self.value(strength);
                let px = "ctx_local_px(ctx)";
                let hx1 = self.field_expr_at(field, &format!("(({px}) + vec2<f32>(1.0, 0.0))"));
                let hx0 = self.field_expr_at(field, &format!("(({px}) - vec2<f32>(1.0, 0.0))"));
                let hy1 = self.field_expr_at(field, &format!("(({px}) + vec2<f32>(0.0, 1.0))"));
                let hy0 = self.field_expr_at(field, &format!("(({px}) - vec2<f32>(0.0, 1.0))"));
                format!(
                    "(normalize(vec3<f32>((-(({hx1}) - ({hx0})) * 0.5 * ({s})), (-(({hy1}) - ({hy0})) * 0.5 * ({s})), 1.0)))"
                )
            }
            Normal::Perturbed(a, b) => {
                let a = self.normal(a);
                let b = self.normal(b);
                format!("(normalize(({a}) + ({b})))")
            }
            Normal::Card { gain } => {
                let s = self.value(gain);
                let px = "ctx_local_px(ctx)";
                let hx1 = format!(
                    "(material_edge_dist((({px}) + vec2<f32>(1.0, 0.0)), ({}), ({})))",
                    card_rect_ref(),
                    card_radius_ref()
                );
                let hx0 = format!(
                    "(material_edge_dist((({px}) - vec2<f32>(1.0, 0.0)), ({}), ({})))",
                    card_rect_ref(),
                    card_radius_ref()
                );
                let hy1 = format!(
                    "(material_edge_dist((({px}) + vec2<f32>(0.0, 1.0)), ({}), ({})))",
                    card_rect_ref(),
                    card_radius_ref()
                );
                let hy0 = format!(
                    "(material_edge_dist((({px}) - vec2<f32>(0.0, 1.0)), ({}), ({})))",
                    card_rect_ref(),
                    card_radius_ref()
                );
                format!(
                    "(normalize(vec3<f32>((-(({hx1}) - ({hx0})) * 0.5 * ({s})), (-(({hy1}) - ({hy0})) * 0.5 * ({s})), 1.0)))"
                )
            }
            Normal::Beveled { gain, radius_px } | Normal::Meniscus { radius_px, curvature: gain } => {
                let (s, r) = (self.value(gain), self.value(radius_px));
                let px = "ctx_local_px(ctx)";
                let card = card_rect_ref();
                let radii = card_radius_ref();
                
                // Sample the continuous rounded-box distance field e at ±1px
                let ex1 = format!("(material_edge_dist((({px}) + vec2<f32>(1.0, 0.0)), ({card}), ({radii}), ({r})))");
                let ex0 = format!("(material_edge_dist((({px}) - vec2<f32>(1.0, 0.0)), ({card}), ({radii}), ({r})))");
                let ey1 = format!("(material_edge_dist((({px}) + vec2<f32>(0.0, 1.0)), ({card}), ({radii}), ({r})))");
                let ey0 = format!("(material_edge_dist((({px}) - vec2<f32>(0.0, 1.0)), ({card}), ({radii}), ({r})))");
                let e_mid = format!("(material_edge_dist(({px}), ({card}), ({radii}), ({r})))");

                // 1. Calculate the unit 2D edge normal (points inward along the SDF gradient)
                // 2. Scale by Method 2's fitted decay (decay = 6.17 over band r):
                //    Maximum deflection right at the perimeter rim, decaying to 0 inside.
                // 3. Scale by gain (s) and map into n.xy for the Snell refraction pass.
                format!(
                    "({{
                        let grad = vec2<f32>((({ex1}) - ({ex0})) * 0.5, (({ey1}) - ({ey0})) * 0.5);
                        let grad_len = max(length(grad), 1e-4);
                        let edge_dir = grad / grad_len;
                        let u = clamp(({e_mid}) / max(({r}), 1.0), 0.0, 1.0);
                        let falloff = exp(-6.17 * u);
                        let slope = edge_dir * falloff * ({s});
                        vec3<f32>(-slope.x, -slope.y, sqrt(max(1.0 - dot(slope, slope), 0.01)))
                    }})"
                )
            }
            Normal::FluidTension { radius_px } => {
                let r = self.value(radius_px);
                let px = "ctx_local_px(ctx)";
                let card = card_rect_ref();
                let radii = card_radius_ref();
                let prof = |e: &str| {
                    format!("(sin(3.14159265 * clamp(({e}) / max(({r}), 1.0), 0.0, 1.0)))")
                };
                let ex1 = format!(
                    "(material_edge_dist((({px}) + vec2<f32>(1.0, 0.0)), ({card}), ({radii})))"
                );
                let ex0 = format!(
                    "(material_edge_dist((({px}) - vec2<f32>(1.0, 0.0)), ({card}), ({radii})))"
                );
                let ey1 = format!(
                    "(material_edge_dist((({px}) + vec2<f32>(0.0, 1.0)), ({card}), ({radii})))"
                );
                let ey0 = format!(
                    "(material_edge_dist((({px}) - vec2<f32>(0.0, 1.0)), ({card}), ({radii})))"
                );
                let (hx1, hx0, hy1, hy0) = (prof(&ex1), prof(&ex0), prof(&ey1), prof(&ey0));
                format!("(normalize(vec3<f32>((-(({hx1}) - ({hx0})) * 0.5), (-(({hy1}) - ({hy0})) * 0.5), 1.0)))")
            }
            Normal::Ribbed { freq_per_px } => {
                let f = self.value(freq_per_px);
                format!("(normalize(vec3<f32>(-(6.2831853 * ({f}) * cos(6.2831853 * ({f}) * ctx_local_px(ctx).x)), 0.0, 1.0)))")
            }
            Normal::Perlin { .. } => {
                panic!(
                    "material DSL (Phase 5e): `Bump::perlin_noise` lowers with extended closures"
                )
            }
            Normal::Sum(a, b) => {
                let a = self.normal(a);
                let b = self.normal(b);
                format!("(normalize(({a}) + ({b})))")
            }
        }
    }

    fn fresnel_expr(&mut self, f: &Fresnel) -> (String, String) {
        match f {
            Fresnel::Dielectric { ior } => {
                let ior = self.value(ior);
                (format!("(fresnel_schlick_f0(({ior})))"), ior)
            }
            Fresnel::Conductor { .. } => {
                panic!("material DSL (Phase 6): conductor Fresnel lowers with extended closures")
            }
        }
    }

    fn light_dir(&mut self, l: &Light) -> String {
        match l {
            Light::Directional { x, y, z } => {
                let (x, y, z) = (self.value(x), self.value(y), self.value(z));
                format!("(normalize(vec3<f32>(({x}), ({y}), ({z}))))")
            }
            Light::Point { x, y, height } => {
                let (x, y) = (self.weight(x), self.weight(y));
                let h = self.value(height);
                format!("(point_lamp_dir(vec2<f32>(({x}), ({y})), ctx_local_px(ctx), ({h})))")
            }
        }
    }

    fn closure(&mut self, c: &Closure, env: &str) -> String {
        match c {
            Closure::Warp { .. } => {
                panic!("material DSL (Phase 4.5): `Warp::sine(...)` is a whole-surface stage")
            }
            Closure::Unlit { r, g, b } => {
                let (r, g, b) = (self.value(r), self.value(g), self.value(b));
                let base = format!("vec3<f32>(({r}), ({g}), ({b}))");
                if env == "1.0" {
                    format!("({base})")
                } else {
                    format!("(({base}) * ({env}))")
                }
            }
            Closure::Transmit {
                normal,
                ior,
                frost,
                dispersion,
                atten_r,
                atten_g,
                atten_b,
                atten_density,
            } => {
                let n = self.normal(normal);
                let ior = self.value(ior);
                let fr = self.value(frost);
                let dsp = self.value(dispersion);
                let (ar, ag, ab) = (
                    self.value(atten_r),
                    self.value(atten_g),
                    self.value(atten_b),
                );
                let kd = self.value(atten_density);
                let path = "transmit_path_len(ctx)".to_string();
                let slab = "transmit_refract_slab(ctx)".to_string();
                let refr = format!("(ctx.px + transmit_offset(({n}), ({ior}), ({slab})))");
                let base = format!("transmit_sample(sharp, ({refr}), ({fr}), ({dsp}), ({n}).xy)");
                if env == "1.0" {
                    format!("transmit_beer(({base}), vec3<f32>(({ar}), ({ag}), ({ab})), ({kd}), ({path}))")
                } else {
                    format!("(transmit_beer(({base}), vec3<f32>(({ar}), ({ag}), ({ab})), ({kd}), ({path})) * ({env}))")
                }
            }
            Closure::Specular {
                normal,
                distribution,
                fresnel,
                env_gain,
            } => {
                let n = self.normal(normal);
                let rough = match distribution {
                    SpecularDist::Isotropic { roughness } => self.value(roughness),
                    SpecularDist::Anisotropic { .. } => {
                        panic!(
                            "material DSL (Phase 6): anisotropic NDF lowers with extended closures"
                        )
                    }
                };
                let (f0, _) = self.fresnel_expr(fresnel);
                let gain = if env == "1.0" {
                    self.value(env_gain)
                } else {
                    env.to_string()
                };
                format!(
                    "(ggx_spec_f0(vec3<f32>(0.0, 0.0, 1.0), ({n}), ({rough}), ({f0})) * ({gain}))"
                )
            }
            Closure::Interface {
                normal,
                ior,
                frost,
                dispersion,
                atten_r,
                atten_g,
                atten_b,
                atten_density,
                roughness,
                env_gain,
                light,
            } => {
                let n = self.normal(normal);
                let ior = self.value(ior);
                let fr = self.value(frost);
                let dsp = self.value(dispersion);
                let (ar, ag, ab) = (
                    self.value(atten_r),
                    self.value(atten_g),
                    self.value(atten_b),
                );
                let kd = self.value(atten_density);
                let rough = self.value(roughness);
                let gain = if env == "1.0" {
                    self.value(env_gain)
                } else {
                    env.to_string()
                };
                let path = "transmit_path_len(ctx)".to_string();
                let slab = "transmit_refract_slab(ctx)".to_string();
                let refr = format!("(ctx.px + transmit_offset(({n}), ({ior}), ({slab})))");
                let trans = format!("transmit_beer((transmit_sample(sharp, ({refr}), ({fr}), ({dsp}))), vec3<f32>(({ar}), ({ag}), ({ab})), ({kd}), ({path}))");
                let refl = match light {
                    Some(l) => {
                        let ld = self.light_dir(l);
                        format!("(ggx_spec_light(vec3<f32>(0.0, 0.0, 1.0), ({n}), ({rough}), fresnel_schlick_f0(({ior})), ({ld})) * ({gain}))")
                    }
                    None => {
                        format!("(ggx_spec_f0(vec3<f32>(0.0, 0.0, 1.0), ({n}), ({rough}), fresnel_schlick_f0(({ior}))) * ({gain}))")
                    }
                };
                format!("mix(({trans}), ({refl}), fresnel_schlick(max(({n}).z, 0.0), ({ior})))")
            }
            Closure::Emission {
                r,
                g,
                b,
                gain,
                mask,
            } => {
                let (r, g, b) = (self.value(r), self.value(g), self.value(b));
                let mut k = self.value(gain);
                if env != "1.0" {
                    k = format!("(({k}) * ({env}))");
                }
                let base = format!("(vec3<f32>(({r}), ({g}), ({b})) * ({k}))");
                match mask {
                    Some(f) => {
                        let m = self.field_expr(f);
                        format!("(({base}) * ({m}))")
                    }
                    None => format!("({base})"),
                }
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
            Modulator::Beer { r, g, b, density } => {
                let (r, g, b) = (self.value(r), self.value(g), self.value(b));
                let kd = self.value(density);
                format!("(transmit_beer(({inner}), vec3<f32>(({r}), ({g}), ({b})), ({kd}), transmit_path_len(ctx)))")
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
                GeoField::Uv => "(ctx_uv(ctx).x)".to_string(),
                GeoField::UvY => "(ctx_uv(ctx).y)".to_string(),
                GeoField::LocalPx => "(ctx_local_px(ctx).x)".to_string(),
                GeoField::LocalPxY => "(ctx_local_px(ctx).y)".to_string(),
                GeoField::EdgeDist => format!(
                    "(material_edge_dist(ctx_local_px(ctx), ({card}), ({radius})))",
                    card = card_rect_ref(),
                    radius = card_radius_ref(),
                ),
                GeoField::PointerPxX
                | GeoField::PointerPxY
                | GeoField::PointerUvX
                | GeoField::PointerUvY
                | GeoField::VelocityX
                | GeoField::VelocityY => {
                    panic!("material DSL (Phase 9e): pointer/velocity uniforms gate phase 9e")
                }
                GeoField::Position => "(fract(length(ctx.px) / 80.0))".to_string(),
                GeoField::Curvature => "(length(ctx.normal.xy))".to_string(),
                GeoField::Radius => self.geo_field_value(GeoField::Radius),
            },
        }
    }

    fn field_expr(&mut self, f: &Field) -> String {
        self.field_expr_at(f, "ctx_local_px(ctx)")
    }

    fn field_expr_at(&mut self, f: &Field, at: &str) -> String {
        match f {
            Field::Constant(v) => format!("({})", self.value(v)),
            Field::Sine {
                freq_per_px,
                vertical,
            } => {
                let k = self.value(freq_per_px);
                let coord = if *vertical {
                    format!("(({at}).y)")
                } else {
                    format!("(({at}).x)")
                };
                format!("(sin(6.2831853 * ({k}) * {coord}))")
            }
            Field::Noise { scale } => {
                let k = self.value(scale);
                format!("(field_noise_2oct(({at}), ({k})))")
            }
            Field::Grid {
                spacing_px,
                width_px,
            } => {
                let (s, w) = (self.value(spacing_px), self.value(width_px));
                format!("(field_grid_mask(({at}), ({s}), ({w})))")
            }
            Field::DistanceToEdge => {
                format!(
                    "(material_edge_dist({at}, ({card}), ({radius})))",
                    at = at,
                    card = card_rect_ref(),
                    radius = card_radius_ref(),
                )
            }
        }
    }

    fn reads_backdrop(expr: &MaterialExpr) -> bool {
        match expr {
            MaterialExpr::Closure(Closure::Transmit { .. } | Closure::Interface { .. }) => true,
            MaterialExpr::Closure(_) => false,
            MaterialExpr::Lit(c, _) => {
                matches!(c, Closure::Transmit { .. } | Closure::Interface { .. })
            }
            MaterialExpr::LitLight(c, _) => {
                matches!(c, Closure::Transmit { .. } | Closure::Interface { .. })
            }
            MaterialExpr::Modulated(_, inner) => Self::reads_backdrop(inner),
            MaterialExpr::Sum(a, b) | MaterialExpr::Mix(a, b, _) => {
                Self::reads_backdrop(a) || Self::reads_backdrop(b)
            }
        }
    }

    fn max_frost(expr: &MaterialExpr) -> f32 {
        match expr {
            MaterialExpr::Closure(
                Closure::Transmit { frost, .. } | Closure::Interface { frost, .. },
            ) => match frost {
                Value::Dyn(f) | Value::Lit(f) => *f,
                Value::GeoField(_) => 0.0,
            },
            MaterialExpr::Closure(_) => 0.0,
            MaterialExpr::Lit(c, _) | MaterialExpr::LitLight(c, _) => match c {
                Closure::Transmit { frost, .. } | Closure::Interface { frost, .. } => match frost {
                    Value::Dyn(f) | Value::Lit(f) => *f,
                    Value::GeoField(_) => 0.0,
                },
                _ => 0.0,
            },
            MaterialExpr::Modulated(_, inner) => Self::max_frost(inner),
            MaterialExpr::Sum(a, b) | MaterialExpr::Mix(a, b, _) => {
                Self::max_frost(a).max(Self::max_frost(b))
            }
        }
    }

    fn max_dispersion(expr: &MaterialExpr) -> f32 {
        match expr {
            MaterialExpr::Closure(
                Closure::Transmit { dispersion, .. } | Closure::Interface { dispersion, .. },
            ) => match dispersion {
                Value::Dyn(f) | Value::Lit(f) => *f,
                Value::GeoField(_) => 0.0,
            },
            MaterialExpr::Closure(_) => 0.0,
            MaterialExpr::Lit(c, _) | MaterialExpr::LitLight(c, _) => match c {
                Closure::Transmit { dispersion, .. } | Closure::Interface { dispersion, .. } => {
                    match dispersion {
                        Value::Dyn(f) | Value::Lit(f) => *f,
                        Value::GeoField(_) => 0.0,
                    }
                }
                _ => 0.0,
            },
            MaterialExpr::Modulated(_, inner) => Self::max_dispersion(inner),
            MaterialExpr::Sum(a, b) | MaterialExpr::Mix(a, b, _) => {
                Self::max_dispersion(a).max(Self::max_dispersion(b))
            }
        }
    }

    fn term(&mut self, expr: &MaterialExpr) -> String {
        match expr {
            MaterialExpr::Closure(c) => self.closure(c, "1.0"),
            MaterialExpr::Lit(c, env) => {
                let g = self.value(&env.gain);
                self.closure(c, &g)
            }
            MaterialExpr::LitLight(c, light) => match c {
                Closure::Specular {
                    normal,
                    distribution,
                    fresnel,
                    env_gain,
                } => {
                    let n = self.normal(normal);
                    let rough = match distribution {
                        SpecularDist::Isotropic { roughness } => self.value(roughness),
                        SpecularDist::Anisotropic { .. } => {
                            panic!("material DSL (Phase 6): anisotropic NDF lowers with extended closures")
                        }
                    };
                    let (f0, _) = self.fresnel_expr(fresnel);
                    let ld = self.light_dir(light);
                    let gain = self.value(env_gain);
                    format!("(ggx_spec_light(vec3<f32>(0.0, 0.0, 1.0), ({n}), ({rough}), ({f0}), ({ld})) * ({gain}))")
                }
                Closure::Interface {
                    normal,
                    ior,
                    frost,
                    dispersion,
                    atten_r,
                    atten_g,
                    atten_b,
                    atten_density,
                    roughness,
                    env_gain,
                    light: _,
                } => {
                    let owned = Closure::Interface {
                        normal: normal.clone(),
                        ior: ior.clone(),
                        frost: frost.clone(),
                        dispersion: dispersion.clone(),
                        atten_r: atten_r.clone(),
                        atten_g: atten_g.clone(),
                        atten_b: atten_b.clone(),
                        atten_density: atten_density.clone(),
                        roughness: roughness.clone(),
                        env_gain: env_gain.clone(),
                        light: Some(light.clone()),
                    };
                    let g = "1.0".to_string();
                    self.closure(&owned, &g)
                }
                _ => {
                    let g = "1.0".to_string();
                    self.closure(c, &g)
                }
            },
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

fn ggx_spec_f0_decl() -> &'static str {
    "fn ggx_spec_f0(v: vec3<f32>, n: vec3<f32>, roughness: f32, f0: vec3<f32>) -> vec3<f32>"
}

// In PRELUDE_WGSL:

const PRELUDE_WGSL: &str = r#"
// Add inside PRELUDE_WGSL:

fn beveled_lens_normal(
    p: vec2<f32>,
    card: vec4<f32>,
    radii: vec4<f32>,
    band_r: f32,
    strength: f32,
) -> vec3<f32> {
    let r = max(band_r, 1.0);
    let e = material_edge_dist(p, card, radii, r);
    
    // Central difference gradient taps at ±1px
    let ex1 = material_edge_dist(p + vec2<f32>(1.0, 0.0), card, radii, r);
    let ex0 = material_edge_dist(p - vec2<f32>(1.0, 0.0), card, radii, r);
    let ey1 = material_edge_dist(p + vec2<f32>(0.0, 1.0), card, radii, r);
    let ey0 = material_edge_dist(p - vec2<f32>(0.0, 1.0), card, radii, r);
    
    let grad = vec2<f32>((ex1 - ex0) * 0.5, (ey1 - ey0) * 0.5);
    let grad_len = length(grad);
    if (grad_len < 1e-4) {
        return vec3<f32>(0.0, 0.0, 1.0);
    }
    
    // Unit edge vector pointing inward into the card
    let edge_dir = grad / grad_len;
    
    // Method 2's fitted exponential decay profile (decay = 6.17):
    // Maximum refraction at the rim (e = 0), decaying smoothly to 0 inside the card.
    let u = clamp(e / r, 0.0, 1.0);
    let falloff = exp(-6.17 * u);
    let slope = edge_dir * falloff * strength;
    
    return vec3<f32>(-slope.x, -slope.y, 1.0);
}

fn transmit_refract_slab(ctx: EffectCtx) -> f32 {
    if (ctx.path_length > 0.0) {
        return ctx.path_length;
    }
    // Delivers 16px - 22px of true liquid lens displacement
    return 48.0;
}

fn transmit_offset(n: vec3<f32>, ior: f32, slab_depth: f32) -> vec2<f32> {
    let bend = max(1.0 - 1.0 / max(ior, 1.0), 0.0);
    return -n.xy * bend * slab_depth;
}

// Analytic rounded-box SDF: returns distance to edge (positive inside, 0 at boundary).
// 1. Unclamped analytic rounded-box SDF: continuous across corners
fn material_edge_dist(p: vec2<f32>, card: vec4<f32>, radii: vec4<f32>, fallback_r: f32) -> f32 {
    let size = max(card.zw, vec2<f32>(1.0));
    let half = size * 0.5;
    
    // Select quadrant corner radius
    var r = radii.x;
    if (p.x > half.x) {
        if (p.y > half.y) {
            r = radii.z;
        } else {
            r = radii.y;
        }
    } else if (p.y > half.y) {
        r = radii.w;
    }
    if (r <= 0.0) {
        r = fallback_r;
    }
    r = clamp(r, 0.0, min(half.x, half.y));
    
    // Standard unclamped 2D rounded box SDF
    let q = abs(p - half) - half + vec2<f32>(r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    return max(-d, 0.0);
}

// 2. Optical travel depth (px)
fn transmit_refract_slab(ctx: EffectCtx) -> f32 {
    if (ctx.path_length > 0.0) {
        return ctx.path_length;
    }
    // Optical depth constant: delivers 15px - 22px of true liquid lens displacement
    return 48.0;
}

fn transmit_offset(n: vec3<f32>, ior: f32, slab_depth: f32) -> vec2<f32> {
    let bend = max(1.0 - 1.0 / max(ior, 1.0), 0.0);
    // n.xy carries the directed lens falloff; pulls inward toward the optical center
    return -n.xy * bend * slab_depth;
}

fn fresnel_schlick(nov: f32, ior: f32) -> f32 {
    let f0 = (ior - 1.0) / (ior + 1.0);
    let f0_2 = f0 * f0;
    let t = clamp(1.0 - nov, 0.0, 1.0);
    return f0_2 + (1.0 - f0_2) * t * t * t * t * t;
}

fn fresnel_schlick_f0(ior: f32) -> vec3<f32> {
    let f0 = (ior - 1.0) / (ior + 1.0);
    return vec3<f32>(f0 * f0);
}

fn ggx_spec_f0(v: vec3<f32>, n: vec3<f32>, roughness: f32, f0: vec3<f32>) -> vec3<f32> {
    let a = max(roughness, 0.001);
    let a2 = a * a;
    let nov = max(dot(n, v), 0.0);
    let d = (a2 - 1.0) * nov * nov + 1.0;
    let ndf = a2 / (3.14159265 * d * d);
    let f = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - nov, 5.0);
    return f * ndf * nov;
}

fn ggx_spec_light(v: vec3<f32>, n: vec3<f32>, roughness: f32, f0: vec3<f32>, l: vec3<f32>) -> vec3<f32> {
    let a = max(roughness, 0.001);
    let a2 = a * a;
    let h = normalize(l + v);
    let noh = max(dot(n, h), 0.0);
    let nol = max(dot(n, l), 0.0);
    let nov = max(dot(n, v), 0.0);
    let d = (a2 - 1.0) * noh * noh + 1.0;
    let ndf = a2 / (3.14159265 * d * d);
    let voh = clamp(dot(v, h), 0.0, 1.0);
    let f = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - voh, 5.0);
    return f * ndf * nol / (4.0 * max(nov, 0.25));
}

fn transmit_path_len(ctx: EffectCtx) -> f32 {
    return max(ctx.path_length, ctx_edge_dist(ctx));
}

fn transmit_refract_slab(ctx: EffectCtx) -> f32 {
    if (ctx.path_length > 0.0) {
        return ctx.path_length;
    }
    return 24.0; // Ample slab depth for punchy liquid distortion
}

fn transmit_offset(n: vec3<f32>, ior: f32, slab_depth: f32) -> vec2<f32> {
    let bend = max(1.0 - 1.0 / max(ior, 1.0), 0.0);
    return n.xy * bend * slab_depth;
}

fn transmit_tap(sharp: texture_2d<f32>, refr: vec2<f32>, off: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(sharp));
    let uv = clamp((refr + off) / max(dims, vec2<f32>(1.0)), vec2<f32>(0.0), vec2<f32>(1.0));
    let s = textureSampleLevel(sharp, effect_sampler, uv, 0.0);
    let a = max(s.a, 1.0 / 255.0);
    return s.rgb / a;
}

fn transmit_sample(sharp: texture_2d<f32>, refr: vec2<f32>, frost: f32, disp: f32, n_xy: vec2<f32>) -> vec3<f32> {
    // Dispersion direction aligns with normal tilt; flat area (n.xy == 0) has 0 dispersion
    let disp_vec = n_xy * disp;
    if (frost > 0.0) {
        let t = mat2x2<f32>(vec2<f32>(-0.7373688, 0.6754903), vec2<f32>(-0.6754903, -0.7373688));
        var acc = transmit_tap(sharp, refr, vec2<f32>(0.0));
        var dir = vec2<f32>(1.0, 0.0);
        for (var i = 0; i < 4; i = i + 1) {
            acc += transmit_tap(sharp, refr, dir * frost);
            dir = t * dir;
        }
        let base = acc / 5.0;
        if (disp > 0.0) {
            let r = transmit_tap(sharp, refr, disp_vec);
            let b = transmit_tap(sharp, refr, -disp_vec);
            return vec3<f32>(r.r, base.g, b.b);
        }
        return base;
    }
    if (disp > 0.0) {
        let c = transmit_tap(sharp, refr, vec2<f32>(0.0));
        let r = transmit_tap(sharp, refr, disp_vec);
        let b = transmit_tap(sharp, refr, -disp_vec);
        return vec3<f32>(r.r, c.g, b.b);
    }
    return transmit_tap(sharp, refr, vec2<f32>(0.0));
}

fn transmit_beer(c: vec3<f32>, tint: vec3<f32>, density: f32, path: f32) -> vec3<f32> {
    if (density <= 0.0) {
        return c * tint;
    }
    let absorb = -log(max(tint, vec3<f32>(1e-3))) * density * path;
    return c * exp(-absorb);
}

fn field_hash(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

fn field_vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (vec2<f32>(3.0) - vec2<f32>(2.0) * f);
    let a = field_hash(i);
    let b = field_hash(i + vec2<f32>(1.0, 0.0));
    let c = field_hash(i + vec2<f32>(0.0, 1.0));
    let d = field_hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn field_noise_2oct(p: vec2<f32>, scale: f32) -> f32 {
    let q = p * scale;
    return (field_vnoise(q) * 0.667 + field_vnoise(q * 2.03 + vec2<f32>(17.0, 9.0)) * 0.333) - 0.5;
}

fn field_grid_mask(p: vec2<f32>, spacing: f32, width: f32) -> f32 {
    let s = max(spacing, 1e-3);
    let g = abs(fract(p / s) - vec2<f32>(0.5)) * s;
    let d = min(g.x, g.y);
    return 1.0 - smoothstep(max(width * 0.5 - 1.0, 0.0), width * 0.5 + 1.0, d);
}

fn point_lamp_dir(anchor_px: vec2<f32>, frag_px: vec2<f32>, height: f32) -> vec3<f32> {
    let h = max(height, 1.0);
    return normalize(vec3<f32>(anchor_px.x - frag_px.x, frag_px.y - anchor_px.y, h));
}
"#;

fn lower_material(expr: &MaterialExpr) -> Surface {
    let mut cx = LowerCx { uniforms: vec![] };
    let _ = ggx_spec_f0_decl();
    let radiance = cx.term(expr);
    let source = format!(
        "{PRELUDE_WGSL}\nfn apply(\n    ctx: EffectCtx,\n    backdrop: texture_2d<f32>,\n    sharp: texture_2d<f32>,\n    u: CustomUniforms,\n) -> EffectCtx {{\n    var out = ctx;\n    let card_a = ctx.color.a;\n    let radiance: vec3<f32> = {radiance};\n    out.color = vec4<f32>(radiance * card_a, card_a);\n    return out;\n}}\n"
    );
    let backdrop = if LowerCx::reads_backdrop(expr) {
        let sigma = LowerCx::max_frost(expr);
        if sigma > 0.0 {
            BackdropNeed::ReadBlurred { sigma }
        } else {
            BackdropNeed::ReadSharp
        }
    } else {
        BackdropNeed::None
    };
    let pad = if LowerCx::reads_backdrop(expr) {
        let frost = LowerCx::max_frost(expr);
        let disp = LowerCx::max_dispersion(expr);
        Some((frost + disp + 16.0).max(1.0))
    } else {
        None
    };
    Surface {
        wgsl: source,
        uniforms: cx.uniforms,
        backdrop,
        pad,
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
// Effect plumbing
// ---------------------------------------------------------------------------

use crate::reactive::IntoProp as _MaterialIntoProp;
use crate::reactive::Prop as _MaterialProp;

impl crate::foundation::IntoEffect for Surface {
    fn into_effect(self) -> Box<dyn crate::foundation::Effect> {
        Box::new(Surface::into_effect(self))
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

pub struct GeoFn<F: Fn(Geo) -> Surface>(pub F);
impl<F: Fn(Geo) -> Surface + 'static> _MaterialIntoProp<EffectStack> for GeoFn<F> {
    fn into_prop(self) -> _MaterialProp<EffectStack> {
        _MaterialProp::Static(self.0(Geo::surface()).into_stack())
    }
}
