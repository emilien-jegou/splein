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
//
// `Closure` is radiance generation only. Fresnel is orthogonal
// ([`Fresnel`]: dielectric IOR vs complex-IOR conductor — so anisotropic
// metals exist), intensity/tint are linear factors (`Modulator`), and
// dispersion/attenuation are `Transmit` modifiers (`.disperse(..)` /
// `.attenuate(..)`), not sibling presets. `Unlit` is the opaque-plaque
// default: no PBR math, no ×30 compensation — `Color` stays `Color`.
#[derive(Clone, Debug, PartialEq)]
pub enum Closure {
    Warp { frequency: Value, amplitude: Value },
    /// Opaque unlit plaque: bare `Color` stays on screen, no PBR lobe.
    Unlit {
        r: Value,
        g: Value,
        b: Value,
    },
    /// Transmission: backdrop sample with optional blur, spectral spread,
    /// and Beer's-law attenuation — modifiers on one node, not presets.
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
    /// Dielectric interface (Phase 5c): one IOR, Schlick mix of the
    /// reflection half (`ggx_spec_*`) and the transmission half
    /// (`transmit_*`) — energy conservation by construction, so demos
    /// never hand-write `mix(trans, refl, fresnel(n, X))` with the IOR
    /// repeated twice and drifting out of sync.
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

/// Fresnel formulation: dielectric IOR vs complex-IOR conductor.
/// Orthogonal to the NDF — anisotropic metals work.
#[derive(Clone, Debug, PartialEq)]
pub enum Fresnel {
    Dielectric { ior: Value },
    Conductor { n: Value, k: Value },
}

/// Microfacet distribution: isotropic GGX or anisotropic.
#[derive(Clone, Debug, PartialEq)]
pub enum SpecularDist {
    Isotropic { roughness: Value },
    Anisotropic { rx: Value, ry: Value },
}

// ---------------------------------------------------------------------------
// Terms
// ---------------------------------------------------------------------------

/// Environment lighting term.
#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    pub gain: Value,
}

/// Virtual studio light feeding `.lit_by(..)` on Specular/Interface
/// builders (Phase 5e). Numbers ride uniforms like every other numeric;
/// `point` takes a card-local anchor: pass `geo.pointer_px()`-style
/// weights (see `Weight::GeoField::Pointer*`) or plain px via
/// `Lights::point_xy(x, y, height)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Light {
    Directional { x: Value, y: Value, z: Value },
    Point { x: Weight, y: Weight, height: Value },
}

/// The virtual-light namespace.
pub struct Lights;
impl Lights {
    /// Studio key light: direction surface→light in surface coords
    /// (x right, y down, z out of screen).
    pub fn directional(dir: (f32, f32, f32)) -> Light {
        Light::Directional { x: Value::Dyn(dir.0), y: Value::Dyn(dir.1), z: Value::Dyn(dir.2) }
    }
    /// Pointer lamp: card-local anchor + height above the card in px.
    pub fn point(x: Weight, y: Weight, height: impl Into<Value>) -> Light {
        Light::Point { x, y, height: height.into() }
    }
    /// Pointer lamp at a fixed card-local position (px from top-left).
    pub fn point_xy(x: impl Into<Value>, y: impl Into<Value>, height: impl Into<Value>) -> Light {
        Light::Point {
            x: Weight::Scalar(x.into()),
            y: Weight::Scalar(y.into()),
            height: height.into(),
        }
    }
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
/// card-local UVs and px plus the legacy silhouette/depth/curvature reads.
///
/// V3 note: `Position` (device-space radial sweep) is deprecated — it made
/// patterns swim when cards scrolled. Prefer `geo.uv()` / `geo.local_px()`
/// (card-local, travel with the card) or `geo.edge_dist()`.
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
    /// The card's resolved corner radius (device px): the box shape
    /// follows the shader. Reads the host-seeded slot-8 corner radii
    /// and reduces to one scalar (the largest corner — equal corners
    /// for uniform/pill boxes), so `Lens::beveled(.., geo.radius())`
    /// bands the dome to the card's actual corner size. Edits to the
    /// box's `radius` prop reach the shader as a uniform refill — the
    /// value is a `Value::GeoField`, never a literal slot.
    Radius,
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
    /// Host-provided bevel: card-local edge profile at `radius` px.
    /// Lowers like `Normal::from_height(Field::edge_dist(), radius)` —
    /// a finite-difference gradient over the analytic edge field.
    pub fn bevel(self, radius: impl Into<Value>) -> Normal {
        Normal::FromHeight {
            field: Field::DistanceToEdge,
            strength: radius.into(),
        }
    }
    /// Card-local UVs `[0,1]²` (travel with the card; replaces `position`).
    /// A vec2: destructure (`let (u, v) = geo.uv();`) — it cannot be a
    /// scalar `Weight` (a `mix` weight needs one float, not two).
    pub fn uv(self) -> (Weight, Weight) {
        (Weight::GeoField(GeoField::Uv), Weight::GeoField(GeoField::UvY))
    }
    /// Card-local px relative to the card top-left (travel with the card).
    /// A vec2: destructure (`let (x, y) = geo.local_px();`).
    pub fn local_px(self) -> (Weight, Weight) {
        (Weight::GeoField(GeoField::LocalPx), Weight::GeoField(GeoField::LocalPxY))
    }
    /// Card-local distance to the nearest edge in px.
    pub fn edge_dist(self) -> Weight {
        Weight::GeoField(GeoField::EdgeDist)
    }
    /// The card's resolved corner radius in device px, as a shader-side
    /// value (`Value::GeoField`): the box shape feeds the material, so
    /// `Lens::beveled(gain, geo.radius())` bands the bevel dome to the
    /// card's corner size and editing the box's `radius` prop edits the
    /// shader value (uniform refill, zero recompile). Scalar = the
    /// largest corner; uniform / pill boxes resolve equal corners.
    pub fn radius(self) -> Value {
        Value::GeoField(GeoField::Radius)
    }
    /// Card-local pointer position in px (feeds `Lights::point` lamps).
    pub fn pointer_px(self) -> (Weight, Weight) {
        (Weight::GeoField(GeoField::PointerPxX), Weight::GeoField(GeoField::PointerPxY))
    }
    /// Card-local pointer position in UV [0,1]².
    pub fn pointer_uv(self) -> (Weight, Weight) {
        (Weight::GeoField(GeoField::PointerUvX), Weight::GeoField(GeoField::PointerUvY))
    }
    /// Card velocity in px/s (drives drag slosh / motion-reactive glass).
    pub fn velocity(self) -> (Weight, Weight) {
        (Weight::GeoField(GeoField::VelocityX), Weight::GeoField(GeoField::VelocityY))
    }
    pub fn edge(self) -> Weight {
        Weight::GeoField(GeoField::Edge)
    }
    pub fn thickness(self) -> Weight {
        Weight::GeoField(GeoField::Thickness)
    }
    #[deprecated(note = "device-space; patterns swim when the card moves — use geo.uv() / geo.local_px()")]
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
///
/// V3: `Closure` is radiance generation only. `Unlit` carries the plaque
/// default (bare `Color` stays on screen); `Lit(c, env)` is the legacy
/// env-carrying shape kept compiling — new code uses
/// `Specular { env_gain }` / `.lit_by(..)` instead. `Modulated` is linear
/// factors only (tint, scalar); `Sum` is additive; `Mix` blends on `Weight`.
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
    /// Light a specular term: explicit env hookup (replaces `term * Sky`).
    /// Distributes over `Sum`/`Mix`; wraps bare closures in `Lit`.
    pub fn lit_by(self, gain: impl Into<Value>) -> MaterialExpr {
        apply_env(self, gain.into())
    }
    /// Light a term with a virtual studio light (Phase 5e).
    pub fn lit_by_light(self, light: Light) -> MaterialExpr {
        apply_light(self, light)
    }
    /// Linear radiance scale (value-level hover; same as `term * scalar`).
    pub fn scaled(self, k: impl Into<Value>) -> MaterialExpr {
        MaterialExpr::Modulated(
            Modulator::Intensity {
                lumens: k.into(),
            },
            Box::new(self),
        )
    }
    /// Beer attenuation on a transmit term (replaces `Absorption * term`).
    pub fn attenuate(self, color: crate::foundation::Color, density: impl Into<Value>) -> MaterialExpr {
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

// ---------------------------------------------------------------------------
// Linear factors + explicit env (`*` discipline — no `* == +`)
//
// `*` is linear scaling only: `Color * term` (tint), `term * scalar`
// (gain). Lighting uses `.lit_by(..)`; attenuation uses `.attenuate(..)`;
// term×term is a hard error (add with `+`). The old `attach_color`
// general-product fallback (product == sum) is removed.
// ---------------------------------------------------------------------------

/// Tint a term with a `Color` (linear RGB multiply).
fn tint(term: MaterialExpr, color: crate::foundation::Color) -> MaterialExpr {
    match term {
        MaterialExpr::Modulated(m, inner) => MaterialExpr::Modulated(
            m,
            Box::new(tint(*inner, color)),
        ),
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

/// `term * Sky::gain(g)` / `Sky::gain(g) * term`: legacy env hookup.
/// Prefer `.lit_by(..)` — same lowering, explicit name.
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

/// `term.lit_by_light(light)`: virtual studio light hookup (Phase 5e).
/// Distributes over `Sum`/`Mix` like `apply_env`.
fn apply_light(term: MaterialExpr, light: Light) -> MaterialExpr {
    match term {
        MaterialExpr::Closure(c) => MaterialExpr::LitLight(c, light),
        MaterialExpr::Lit(c, e) => MaterialExpr::LitLight(c, Light::Directional {
            x: Value::Dyn(0.0), y: Value::Dyn(0.0), z: Value::Dyn(1.0),
        }),
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
// V3 closure constructors — minimal orthogonal set
//
// `Transmit` (backdrop sample + frost + dispersion + attenuation in one
// node), `Specular` (NDF × Fresnel × env — metals work, incl. anisotropic),
// `Emission` (color × gain × optional field mask), `Unlit` (opaque plaque).
// `Interface::dielectric` binds one IOR and wires reflection + transmission
// with automatic Schlick mixing (energy-conserving by construction).
// ---------------------------------------------------------------------------

/// Transmission constructor: backdrop sample with optional blur,
/// dispersion, and Beer's-law attenuation — modifiers, not presets.
/// Phase 5b gate: panics until displaced reads land.
pub struct Transmit;
impl Transmit {
    /// Backdrop sample with optional blur, dispersion, and Beer's-law
    /// attenuation — modifiers, not presets. Lowers to displaced
    /// backdrop reads (Phase 5b); `.attenuate` geometry rides the 5d
    /// path-length plan.
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
        let d = self
            .atten
            .map(|(_, d)| d)
            .unwrap_or(Value::Dyn(0.0));
        self.atten = Some((c, d));
        self
    }
    /// Spectral spread in px (0 = none; >0 fans R/B around the base IOR).
    /// A modifier — no separate `Dispersion` preset node.
    pub fn disperse(mut self, spread_px: impl Into<Value>) -> Self {
        self.dispersion = spread_px.into();
        self
    }
    /// Beer's-law attenuation (replaces `Absorption * term`).
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

/// Dielectric interface: one IOR, automatic Schlick mix of reflection +
/// transmission. Eliminates the manual `mix(trans, refl, fresnel(n, X))`
/// IOR-repeated-twice boilerplate and makes energy conservation structural.
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
    /// Virtual studio light for the reflection half (Phase 5e).
    pub fn lit_by_light(mut self, light: Light) -> Self {
        self.light = Some(light);
        self
    }
    pub fn build(self) -> MaterialExpr {
        // Phase 5c: bind one IOR into both halves — the Schlick Fresnel
        // for the reflection weight and the Snell bend for the
        // transmission offset share the same `ior` Value, so they cannot
        // drift out of sync the way hand-written `mix(trans, refl,
        // fresnel(n, X))` pairs did (same normal rides both halves too).
        let (ar, ag, ab, dens) = match &self.atten {
            Some((c, d)) => (Value::Dyn(c.r), Value::Dyn(c.g), Value::Dyn(c.b), d.clone()),
            None => (Value::Dyn(1.0), Value::Dyn(1.0), Value::Dyn(1.0), Value::Dyn(0.0)),
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

/// Microfacet specular: NDF × [`Fresnel`] × env. Dielectric or conductor —
/// anisotropic metals work (the old `Conductor` silo is gone).
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
    /// Beckmann NDF (Phase 6 gate).
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
    /// Complex-IOR conductor: brushed/tilted metals (the old `Conductor`
    /// silo, now orthogonal to the NDF so anisotropic metals work).
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
    /// Explicit env hookup (replaces `term * Sky::gain`).
    pub fn lit_by(mut self, gain: impl Into<Value>) -> Self {
        self.env_gain = gain.into();
        self
    }
    /// Virtual studio light (Phase 5e): directional key or pointer lamp.
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
        MaterialExpr::Modulated(
            Modulator::Color { r, g, b },
            Box::new(spec),
        )
    }
    pub fn into_surface(self) -> Surface {
        self.build().into_surface()
    }
}

/// Unwrap the single closure out of a `Closure(..)` term (internal:
/// `SpecularBuilder::build` above wraps only its own fresh closure).
fn spec_into_closure(e: MaterialExpr) -> Closure {
    match e {
        MaterialExpr::Closure(c) => c,
        _ => panic!("material DSL: internal error — SpecularBuilder wrapped a non-closure"),
    }
}

/// Emission: color × gain, optionally masked by a [`Field`]
/// (replaces `Emission::grid` — any pattern can drive emission).
pub struct Emission;
impl Emission {
    pub fn glow(color: crate::foundation::Color, gain: impl Into<Value>) -> EmissionBuilder {
        EmissionBuilder {
            color,
            gain: gain.into(),
            mask: None,
        }
    }
    /// Deprecated: use `Emission::glow(..).mask(Field::grid(..))`.
    pub fn grid(_s: impl Into<Value>, _w: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Emission::grid` is `Emission::glow(..).mask(Field::grid(..))` — lowers with extended closures");
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

/// Diffuse lobe (Phase 6 gate).
pub struct Diffuse;
impl Diffuse {
    pub fn burley(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Diffuse::burley` lowers with extended closures");
    }
}

/// Charlie sheen (Phase 6 gate).
pub struct Sheen;
impl Sheen {
    pub fn charlie(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Sheen::charlie` lowers with extended closures");
    }
}

// --- Deprecated constructors (kept compiling, forward to V3) ---

/// Deprecated: use `Transmit::sample()` or `Interface::dielectric(..)`.
pub struct Transmission;
impl Transmission {
    #[deprecated(note = "use Transmit::sample() or Interface::dielectric(n, ior)")]
    pub fn new(_normal: Normal, _ior: impl Into<Value>) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5b): `Transmission::new` is `Transmit::sample()` — lowers with displaced backdrop reads + frost blur"
        );
    }
    #[deprecated(note = "use Transmit::sample().frost(..)")]
    pub fn frost(self, _r: impl Into<Value>) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5b): `Transmission` is `Transmit` — lowers with displaced backdrop reads + frost blur"
        );
    }
}

/// Deprecated: use `Normal::from_height(..)` + `Specular::ggx(..)`.
pub struct Microfacet;
impl Microfacet {
    pub fn ggx(normal: Normal, roughness: impl Into<Value>) -> MaterialExpr {
        Specular::ggx(normal, roughness).build()
    }
    #[deprecated(note = "use Specular::beckmann(..)")]
    pub fn beckmann(_normal: Normal, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Microfacet::beckmann` is `Specular::beckmann` — lowers with extended closures");
    }
    #[deprecated(note = "use Specular::anisotropic(..)")]
    pub fn anisotropic(
        normal: Normal,
        ax: impl Into<Value>,
        ay: impl Into<Value>,
    ) -> MaterialExpr {
        Specular::anisotropic(normal, ax, ay).build()
    }
}

/// Deprecated: builder is `.attenuate(color, density)` on the term.
pub struct Absorption;
impl Absorption {
    #[deprecated(note = "use term.attenuate(color, density)")]
    pub fn beer(_color: crate::foundation::Color, _density: impl Into<Value>) -> Modulator {
        panic!("material DSL (Phase 5d): `Absorption::beer` is `.attenuate(..)` on `Transmit` — lowers with displaced reads");
    }
}

/// Deprecated: thin-film is a `Specular` weight, not a modulator.
pub struct ThinFilm;
impl ThinFilm {
    #[deprecated(note = "thin-film is a Specular weight in V3")]
    pub fn interference(_t: impl Into<Value>, _ior: impl Into<Value>) -> Modulator {
        panic!("material DSL (Phase 6): `ThinFilm::interference` lowers with extended closures");
    }
}

/// Deprecated: complex-IOR is `Specular::..conductor(..)`, not a silo.
pub struct Conductor;
impl Conductor {
    #[deprecated(note = "use Specular::ggx/anisotropic(..).conductor(..)")]
    pub fn new(_normal: Normal, _ior: ComplexIor, _roughness: impl Into<Value>) -> MaterialExpr {
        panic!("material DSL (Phase 6): `Conductor::new` is `.conductor(..)` on `Specular` — lowers with extended closures");
    }
}

/// Deprecated: dispersion is `.disperse(..)` on `Transmit`/`Interface`.
pub struct Dispersion;
impl Dispersion {
    #[deprecated(note = "use Transmit::sample().disperse(..) / Interface::dielectric(..).disperse(..)")]
    pub fn prism(_spread: impl Into<Value>) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5b): `Dispersion::prism` is `.disperse(..)` on `Transmit` — lowers with the 3-wavelength split"
        );
    }
}

/// Deprecated: slab is `Interface::dielectric(..).attenuate(..)`.
pub struct Slab;
impl Slab {
    #[deprecated(note = "use Interface::dielectric(..).transmit(..).attenuate(..)")]
    pub fn thick(
        _thickness: impl Into<Value>,
        _tint: crate::foundation::Color,
        _absorb: impl Into<Value>,
    ) -> MaterialExpr {
        panic!(
            "material DSL (Phase 5c): `Slab::thick` is `Interface::dielectric(..)` + `.attenuate(..)` — lowers with refraction + Beer absorption"
        );
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

/// `Color * Intensity` — bare-factor product in either order: tinted
/// unlit plaque, scaled. V3: no degenerate GGX ambient, no ×30 hack —
/// `Color` lowers to `Closure::Unlit` and stays bright.
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

/// `Color * term` — tint a term (linear RGB multiply).
impl std::ops::Mul<MaterialExpr> for crate::foundation::Color {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        tint(rhs, self)
    }
}

/// `term * Color` — tint is commutative.
impl std::ops::Mul<crate::foundation::Color> for MaterialExpr {
    type Output = MaterialExpr;
    fn mul(self, rhs: crate::foundation::Color) -> MaterialExpr {
        tint(self, rhs)
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

/// Legacy env hookup (`term * Sky::gain(g)`): prefer `.lit_by(g)`.
/// Kept compiling so pre-V3 demos migrate row by row.
impl std::ops::Mul<MaterialExpr> for Env {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        apply_env(rhs, self.gain)
    }
}

/// Legacy env hookup (`Sky::gain(g) * term` order): prefer `.lit_by(g)`.
impl std::ops::Mul<Env> for MaterialExpr {
    type Output = MaterialExpr;
    fn mul(self, env: Env) -> MaterialExpr {
        apply_env(self, env.gain)
    }
}

/// `scalar * term` — uniform scale (commutative).
impl std::ops::Mul<MaterialExpr> for f32 {
    type Output = MaterialExpr;
    fn mul(self, rhs: MaterialExpr) -> MaterialExpr {
        rhs * self
    }
}

/// NOTE: there is deliberately NO `Mul<MaterialExpr> for MaterialExpr`
/// impl. `term * term` is a hard compile error — radiance additions must
/// use `+`. (The old `attach_color` product==sum fallback is removed.)

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
// Warp
// ---------------------------------------------------------------------------

impl crate::foundation::Warp {
    /// Lower this warp stage to a shippable `Surface` (whole-stage module,
    /// `ReadSharp` + pad = amplitude).
    pub fn into_surface(self) -> Surface {
        lower_warp(self.frequency, self.amplitude)
    }
}

// ---------------------------------------------------------------------------
// Lowering (runtime — moved out of the macro)
// ---------------------------------------------------------------------------

/// User uniform budget: DSL `Value::Dyn` slots 0..28. Slots 28..36
/// (the last two custom-uniform vec4s, `u.data[7]`/`u.data[8]`) are
/// host-reserved — `run_custom_pass` appends the card rect `(x, y, w, h)`
/// and the resolved corner radii `(tl, tr, br, bl)` there, and the entry
/// scaffolds seed `ctx.card_rect` from the rect. If user lowering ever
/// passes 28 slots the shape genuinely needs a bigger ABI — panic loudly
/// (D-M10b: no silent truncation), don't spill into the host vec4.
const MAX_USER_UNIFORMS: usize = 28;

fn uniform_ref(slot: u32) -> String {
    format!("u.data[{}][{}]", slot / 4, slot % 4)
}

/// WGSL read of the host-seeded card rect (slot 7 = f32 slots 28..32).
fn card_rect_ref() -> &'static str {
    "u.data[7]"
}

/// WGSL read of the host-seeded corner radii (slot 8 = f32 slots
/// 32..36): `(top-left, top-right, bottom-right, bottom-left)` in
/// device px, W3C-resolved and clamped to half the shorter side by the
/// executor. Zero-filled by unwired producers — the sharp-rect
/// baseline.
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
                    "material DSL: expression needs {} user uniforms (cap is {MAX_USER_UNIFORMS}); \
                     slots 28..36 are host-reserved for card_rect + corner radii — simplify the expression or grow the ABI",
                    self.uniforms.len() + 1,
                );
                let slot = self.uniforms.len() as u32;
                self.uniforms.push(*f);
                uniform_ref(slot)
            }
        }
    }

    /// Per-pixel geometry fields read shader-side (never a uniform
    /// slot): [`Geo::radius`] feeds bevel bands and any numeric slot.
    fn geo_field_value(&mut self, field: GeoField) -> String {
        match field {
            GeoField::Radius => {
                "(max(max(u.data[8].x, u.data[8].y), max(u.data[8].z, u.data[8].w)))".to_string()
            }
            other => panic!(
                "material DSL: value-level geo field {other:?} has no scalar lowering \
                 (geo.radius() is the only one); read it as a `Weight` instead"
            ),
        }
    }

    fn normal(&mut self, n: &Normal) -> String {
        match n {
            Normal::Flat => "vec3<f32>(0.0, 0.0, 1.0)".to_string(),
            Normal::Geo => "ctx.normal".to_string(),
            // Height-derived normals: central-difference gradients over
            // card-local fields (Phase 5a). Two taps per axis at ±1px in
            // card-local px; sine/edge-dist are analytic, noise/grids
            // gate their phases. `strength` scales the slope.
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
            // Deprecated structure: kept compiling, same 5a lowering as
            // their V3 replacement. Messages point at the V3 spelling.
            Normal::Card { gain } => {
                let s = self.value(gain);
                let px = "ctx_local_px(ctx)";
                let hx1 = format!("(ctx_edge_dist_at((({px}) + vec2<f32>(1.0, 0.0)), ({}), ({})))", card_rect_ref(), card_radius_ref());
                let hx0 = format!("(ctx_edge_dist_at((({px}) - vec2<f32>(1.0, 0.0)), ({}), ({})))", card_rect_ref(), card_radius_ref());
                let hy1 = format!("(ctx_edge_dist_at((({px}) + vec2<f32>(0.0, 1.0)), ({}), ({})))", card_rect_ref(), card_radius_ref());
                let hy0 = format!("(ctx_edge_dist_at((({px}) - vec2<f32>(0.0, 1.0)), ({}), ({})))", card_rect_ref(), card_radius_ref());
                format!(
                    "(normalize(vec3<f32>((-(({hx1}) - ({hx0})) * 0.5 * ({s})), (-(({hy1}) - ({hy0})) * 0.5 * ({s})), 1.0)))"
                )
            }
            Normal::Beveled { gain, radius_px } | Normal::Meniscus { radius_px, curvature: gain } => {
                let (s, r) = (self.value(gain), self.value(radius_px));
                let px = "ctx_local_px(ctx)";
                let card = card_rect_ref();
                // Smooth dome inside `radius` px of the edge: height is 0
                // at the rim and 1 at the band interior (flat inside) so
                // the derived normal (-dh/dx, -dh/dy) points OUTWARD like
                // the reference SDF gradient (SDF grows outward). The old
                // `(0.5 + 0.5*cos)` peaked at the rim (1 at edge, 0
                // inside): height fell inward, the gradient pointed
                // INWARD, and `transmit_offset` (-n*bend*path) pushed
                // OUTWARD — the inverted-bevel bug (weak/outward bend,
                // squared look, interior smear).
                let prof = |e: &str| {
                    format!("(0.5 - 0.5 * cos(3.14159265 * clamp(({e}) / max(({r}), 1.0), 0.0, 1.0)))")
                };
                let ex1 = format!("(ctx_edge_dist_at((({px}) + vec2<f32>(1.0, 0.0)), ({card}), ({})))", card_radius_ref());
                let ex0 = format!("(ctx_edge_dist_at((({px}) - vec2<f32>(1.0, 0.0)), ({card}), ({})))", card_radius_ref());
                let ey1 = format!("(ctx_edge_dist_at((({px}) + vec2<f32>(0.0, 1.0)), ({card}), ({})))", card_radius_ref());
                let ey0 = format!("(ctx_edge_dist_at((({px}) - vec2<f32>(0.0, 1.0)), ({card}), ({})))", card_radius_ref());
                let (hx1, hx0, hy1, hy0) = (prof(&ex1), prof(&ex0), prof(&ey1), prof(&ey0));
                format!(
                    "(normalize(vec3<f32>((-(({hx1}) - ({hx0})) * 0.5 * ({s})), (-(({hy1}) - ({hy0})) * 0.5 * ({s})), 1.0)))"
                )
            }
            Normal::FluidTension { radius_px } => {
                let r = self.value(radius_px);
                let px = "ctx_local_px(ctx)";
                let card = card_rect_ref();
                // Meniscus ring: peak slope at half radius. Same gradient
                // machinery, sine-ring profile over the edge band.
                let prof = |e: &str| {
                    format!("(sin(3.14159265 * clamp(({e}) / max(({r}), 1.0), 0.0, 1.0)))")
                };
                let ex1 = format!("(ctx_edge_dist_at((({px}) + vec2<f32>(1.0, 0.0)), ({card}), ({})))", card_radius_ref());
                let ex0 = format!("(ctx_edge_dist_at((({px}) - vec2<f32>(1.0, 0.0)), ({card}), ({})))", card_radius_ref());
                let ey1 = format!("(ctx_edge_dist_at((({px}) + vec2<f32>(0.0, 1.0)), ({card}), ({})))", card_radius_ref());
                let ey0 = format!("(ctx_edge_dist_at((({px}) - vec2<f32>(0.0, 1.0)), ({card}), ({})))", card_radius_ref());
                let (hx1, hx0, hy1, hy0) = (prof(&ex1), prof(&ex0), prof(&ey1), prof(&ey0));
                format!("(normalize(vec3<f32>((-(({hx1}) - ({hx0})) * 0.5), (-(({hy1}) - ({hy0})) * 0.5), 1.0)))")
            }
            Normal::Ribbed { freq_per_px } => {
                let f = self.value(freq_per_px);
                // Vertical sine ribs, card-local x. Analytic slope: no taps needed.
                format!("(normalize(vec3<f32>(-(6.2831853 * ({f}) * cos(6.2831853 * ({f}) * ctx_local_px(ctx).x)), 0.0, 1.0)))")
            }
            Normal::Perlin { .. } => {
                panic!("material DSL (Phase 5e): `Bump::perlin_noise` is `Normal::from_height(Field::noise(..))` — lowers with extended closures")
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
                panic!("material DSL (Phase 6): conductor Fresnel lowers with extended closures (complex IOR)")
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
                // Pointer lamp: scalar anchor weights (uniform slots —
                // refill on reactive re-eval, the ex-12 hover pattern)
                // over card-local px. Direction goes through the shared
                // `point_lamp_dir` prelude helper — one computation, no
                // inline duplicate to drift. `geo.pointer_px()` per-frame
                // shader routing gates Phase 9e (needs a host uniform tail
                // + invalidation contract — new ABI surface, not this batch).
                let (x, y) = (self.weight(x), self.weight(y));
                let h = self.value(height);
                format!("(point_lamp_dir(vec2<f32>(({x}), ({y})), ctx_local_px(ctx), ({h})))")
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
            // Unlit plaque: bare `Color` stays on screen, no PBR math.
            // `Lit(Unlit, g)` (via `.lit_by(..)`) acts as a plain gain.
            Closure::Unlit { r, g, b } => {
                let (r, g, b) = (self.value(r), self.value(g), self.value(b));
                let base = format!("vec3<f32>(({r}), ({g}), ({b}))");
                if env == "1.0" {
                    format!("({base})")
                } else {
                    format!("(({base}) * ({env}))")
                }
            }
            // Transmit (Phase 5b): Snell refraction of the backdrop.
            // Refraction offset along the surface slope: for view V=(0,0,1)
            // and a surface normal n, the transmitted direction is approx
            // `-(1 - 1/ior) · n.xy` per px of optical travel; travel is the
            // host `path_length` when a glass producer published it
            // (refract.wgsl) or the card-local edge-distance proxy
            // otherwise (so custom stages refract standalone — the depth
            // field exists on glass stages only).
            // Samples ride `sharp` (raw snapshot — like the glass tex()
            // path unpremultiplied): `backdrop` may carry the engine
            // pre-blur, never the snapshot itself. All reads clamp to the
            // surface dims (region-edge clamp, D-M8).
            // `.frost(s)`: quad-tap rotated spiral around the refracted
            // point at radius s (the 5b frost kernel). `.disperse(s)`:
            // 3-tap chromatic fan R/B ±s around the base offset (the
            // continuous-Cauchy shape is a 5b-future refinement; today
            // the taps are linear in s). `.attenuate/tint` travels the
            // 5d Beer modulator path — builders stash it, lowering below
            // applies exp(-k·path) on the sampled color.
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
                let (ar, ag, ab) = (self.value(atten_r), self.value(atten_g), self.value(atten_b));
                let kd = self.value(atten_density);
                let path = "transmit_path_len(ctx)".to_string();
                // `refr` is the ABSOLUTE sample point: the pixel plus the
                // Snell displacement (`transmit_offset` is a relative
                // shift — passing it bare sampled near the snapshot
                // origin; flat normals are still an exact no-op since the
                // shift is zero).
                let refr = format!("(ctx.px + transmit_offset(({n}), ({ior}), ({path})))");
                let base = format!("transmit_sample(sharp, ({refr}), ({fr}), ({dsp}))");
                if env == "1.0" {
                    format!("transmit_beer(({base}), vec3<f32>(({ar}), ({ag}), ({ab})), ({kd}), ({path}))")
                } else {
                    format!("(transmit_beer(({base}), vec3<f32>(({ar}), ({ag}), ({ab})), ({kd}), ({path})) * ({env}))")
                }
            }
            // Specular: the one path that lowers today (isotropic GGX +
            // dielectric Fresnel, the old Flat+Ggx shape). Anisotropic NDF
            // and conductor Fresnel gate Phase 6. `env` is the `Lit`
            // override when present; otherwise the struct's own `env_gain`.
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
                let gain = if env == "1.0" {
                    self.value(env_gain)
                } else {
                    env.to_string()
                };
                format!("(ggx_spec_f0(vec3<f32>(0.0, 0.0, 1.0), ({n}), ({rough}), ({f0})) * ({gain}))")
            }
            // Interface (Phase 5c): one IOR, Schlick mix of the reflection
            // half and the Transmit half — energy conservation by
            // construction. `fresnel_schlick(nov, ior)` binds the viewing
            // angle (V=(0,0,1), so nov = n.z) to the weight: F at grazing,
            // (1-F) transmitted at normal incidence. The Transmit half
            // reuses the exact 5b path (offset/refr from THIS normal and
            // THIS ior, frost fan, Beer) — no hand-written mix pair to
            // drift out of sync.
            // `env`/`light` select the reflection function: builder
            // `.lit_by_light(l)` (stored on the node — 5e lights the
            // Interface reflection half directly) or the outer
            // `LitLight` wrapper both route to `ggx_spec_light`; plain
            // `.lit_by(g)` / `Lit` routes to `ggx_spec_f0` scaled by the
            // gain. `LitLight` wins over the stored light (explicit outer
            // hookup overrides the builder default).
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
                let (ar, ag, ab) = (self.value(atten_r), self.value(atten_g), self.value(atten_b));
                let kd = self.value(atten_density);
                let rough = self.value(roughness);
                let gain = if env == "1.0" {
                    self.value(env_gain)
                } else {
                    env.to_string()
                };
                let path = "transmit_path_len(ctx)".to_string();
                let refr = format!("(ctx.px + transmit_offset(({n}), ({ior}), ({path})))");
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
            Closure::Emission { r, g, b, gain, mask } => {
                // Phase 5e: emissive core — color × gain, optionally
                // masked by any `Field` (grid scan-lines, noise breakup,
                // edge falloff). No backdrop read, no PBR darkening: the
                // glow adds onto transmit/specular stacks with `+`.
                // `.lit_by(..)` scales the gain like the Unlit plaque.
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
                // Phase 5d: Beer's-law travel over ANY inner term — the
                // same `transmit_beer` helper the Transmit/Interface
                // paths use, over the same travel
                // (`transmit_path_len(ctx)` = host path when a glass
                // producer published it, card-local edge-dist proxy
                // otherwise). Density 0 is an exact no-op (white tint,
                // zero attenuation). `.attenuate` on Transmit/Interface
                // builders stashes into the node and lowers inline; this
                // arm covers `.attenuate(..)` applied to any OTHER term
                // (e.g. a lit Specular stack under smoked glass).
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
                // Card-local fields over the host-seeded `ctx.card_rect`
                // (uniform slot 7, filled by `run_custom_pass` from the
                // unpadded card rect). Travel with the card — retire the
                // G1b device-space swim defect. Scalar `Weight`s must stay
                // scalar: `U`/`LocalPx` are tuples (see `GeoVec2`).
                // `GeoVec2` component reads for `ctx_uv(ctx)` / `ctx_local_px(ctx)`.
                GeoField::Uv => "(ctx_uv(ctx).x)".to_string(),
                GeoField::UvY => "(ctx_uv(ctx).y)".to_string(),
                GeoField::LocalPx => "(ctx_local_px(ctx).x)".to_string(),
                GeoField::LocalPxY => "(ctx_local_px(ctx).y)".to_string(),
                // Rounded-rect SDF over the host-seeded card rect +
                // corner radii (slots 7/8): the edge field follows the
                // card's actual silhouette, corners included.
                GeoField::EdgeDist => format!(
                    "(ctx_edge_dist_at(ctx_local_px(ctx), ({card}), ({radius})))",
                    card = card_rect_ref(),
                    radius = card_radius_ref(),
                ),
                GeoField::PointerPxX => {
                    panic!("material DSL (Phase 9e): `geo.pointer_px()` needs the per-frame host uniform tail + invalidation contract — new ABI surface")
                }
                GeoField::PointerPxY => {
                    panic!("material DSL (Phase 9e): `geo.pointer_px()` needs the per-frame host uniform tail + invalidation contract — new ABI surface")
                }
                GeoField::PointerUvX => {
                    panic!("material DSL (Phase 9e): `geo.pointer_uv()` needs the per-frame host uniform tail + invalidation contract — new ABI surface")
                }
                GeoField::PointerUvY => {
                    panic!("material DSL (Phase 9e): `geo.pointer_uv()` needs the per-frame host uniform tail + invalidation contract — new ABI surface")
                }
                GeoField::VelocityX => {
                    panic!("material DSL (Phase 9e): `geo.velocity()` needs the per-frame host uniform tail + invalidation contract — new ABI surface")
                }
                GeoField::VelocityY => {
                    panic!("material DSL (Phase 9e): `geo.velocity()` needs the per-frame host uniform tail + invalidation contract — new ABI surface")
                }
                GeoField::Position => "(fract(length(ctx.px) / 80.0))".to_string(),
                GeoField::Curvature => "(length(ctx.normal.xy))".to_string(),
                // Value-level field read in a `Weight` slot: `geo.radius()`
                // is publically a `Value`, so this arm is only reachable
                // by hand-built `Weight::GeoField(Radius)` — lower it
                // through the same scalar read rather than crashing.
                GeoField::Radius => self.geo_field_value(GeoField::Radius),
            },
        }
    }

    fn field_expr(&mut self, f: &Field) -> String {
        self.field_expr_at(f, "ctx_local_px(ctx)")
    }

    /// Scalar field sampled at an explicit card-local point expression
    /// (finite-difference taps re-sample here; the plain body samples the
    /// current fragment). `at` is a `vec2<f32>` WGSL expression.
    fn field_expr_at(&mut self, f: &Field, at: &str) -> String {
        match f {
            Field::Constant(v) => format!("({})", self.value(v)),
            // Height ripples over card-local px: `sin(2π·f·x)` (or y when
            // `vertical`). Device px, not UV — the frequency knob is
            // per-px like every other lens radius in the DSL.
            Field::Sine { freq_per_px, vertical } => {
                let k = self.value(freq_per_px);
                let coord = if *vertical {
                    format!("(({at}).y)")
                } else {
                    format!("(({at}).x)")
                };
                format!("(sin(6.2831853 * ({k}) * {coord}))")
            }
            Field::Noise { scale } => {
                // Phase 5e: two-octave hash value noise over card-local px
                // (frost microstructure, glow-mask breakup, ripple detail).
                // `scale` is in 1/px (matches sine's per-px frequency knob);
                // the noise is zero-mean (±0.5) so `FromHeight` gradients
                // read slope, not bias. Deterministic, tile-free.
                let k = self.value(scale);
                format!("(field_noise_2oct(({at}), ({k})))")
            }
            Field::Grid { spacing_px, width_px } => {
                // Phase 5e: periodic grid mask over card-local px (HUD
                // scan geometry, etched-decal line work). 1 inside a line,
                // 0 in the cell interior; feeds `Emission::mask(..)` or
                // `Normal::from_height(..)` alike.
                let (s, w) = (self.value(spacing_px), self.value(width_px));
                format!("(field_grid_mask(({at}), ({s}), ({w})))")
            }
            // Analytic card-local edge distance (px to the nearest edge)
            // over the rounded-rect SDF: the corner radii ride the
            // host-seeded slot 8, so the field follows the card's actual
            // silhouette (`geo.edge_dist()` lowers through the same
            // three-tap helper).
            Field::DistanceToEdge => {
                format!(
                    "(ctx_edge_dist_at({at}, ({card}), ({radius})))",
                    at = at,
                    card = card_rect_ref(),
                    radius = card_radius_ref(),
                )
            }
        }
    }

    /// Does this term read the backdrop (Transmit/Interface anywhere inside)?
    /// Drives `BackdropNeed::ReadSharp/ReadBlurred` in `lower_material`.
    fn reads_backdrop(expr: &MaterialExpr) -> bool {
        match expr {
            MaterialExpr::Closure(Closure::Transmit { .. } | Closure::Interface { .. }) => true,
            MaterialExpr::Closure(_) => false,
            MaterialExpr::Lit(c, _) => matches!(c, Closure::Transmit { .. } | Closure::Interface { .. }),
            MaterialExpr::LitLight(c, _) => matches!(c, Closure::Transmit { .. } | Closure::Interface { .. }),
            MaterialExpr::Modulated(_, inner) => Self::reads_backdrop(inner),
            MaterialExpr::Sum(a, b) | MaterialExpr::Mix(a, b, _) => {
                Self::reads_backdrop(a) || Self::reads_backdrop(b)
            }
        }
    }

    /// Strongest frost sigma inside any `Transmit`/`Interface` (device px).
    /// `> 0` requests the pre-blurred backdrop; `== 0` reads sharp.
    fn max_frost(expr: &MaterialExpr) -> f32 {
        match expr {
            MaterialExpr::Closure(Closure::Transmit { frost, .. } | Closure::Interface { frost, .. }) => match frost {
                Value::Dyn(f) => *f,
                Value::Lit(f) => *f,
                // Per-pixel field (e.g. `geo.radius()` as frost): no
                // static sigma — report 0 so pad falls back to the
                // displacement bound; the shader still reads the field.
                Value::GeoField(_) => 0.0,
            },
            MaterialExpr::Closure(_) => 0.0,
            MaterialExpr::Lit(c, _) => match c {
                Closure::Transmit { frost, .. } | Closure::Interface { frost, .. } => match frost {
                    Value::Dyn(f) => *f,
                    Value::Lit(f) => *f,
                    Value::GeoField(_) => 0.0,
                },
                _ => 0.0,
            },
            MaterialExpr::LitLight(c, _) => match c {
                Closure::Transmit { frost, .. } | Closure::Interface { frost, .. } => match frost {
                    Value::Dyn(f) => *f,
                    Value::Lit(f) => *f,
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

    /// Strongest dispersion spread inside any `Transmit`/`Interface` (px).
    /// Feeds the D-M8 pad: displaced reads reach `spread` past the edge.
    fn max_dispersion(expr: &MaterialExpr) -> f32 {
        match expr {
            MaterialExpr::Closure(Closure::Transmit { dispersion, .. } | Closure::Interface { dispersion, .. }) => match dispersion {
                Value::Dyn(f) => *f,
                Value::Lit(f) => *f,
                // Per-pixel field (e.g. `geo.radius()` as spread): no
                // static spread — same contract as `max_frost` above.
                Value::GeoField(_) => 0.0,
            },
            MaterialExpr::Closure(_) => 0.0,
            MaterialExpr::Lit(c, _) => match c {
                Closure::Transmit { dispersion, .. } | Closure::Interface { dispersion, .. } => match dispersion {
                    Value::Dyn(f) => *f,
                    Value::Lit(f) => *f,
                    Value::GeoField(_) => 0.0,
                },
                _ => 0.0,
            },
            MaterialExpr::LitLight(c, _) => match c {
                Closure::Transmit { dispersion, .. } | Closure::Interface { dispersion, .. } => match dispersion {
                    Value::Dyn(f) => *f,
                    Value::Lit(f) => *f,
                    Value::GeoField(_) => 0.0,
                },
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
                Closure::Specular { normal, distribution, fresnel, env_gain } => {
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
                // Outer `.lit_by_light(l)` wins over the builder's stored
                // light: clone the Interface with the override in place.
                Closure::Interface { normal, ior, frost, dispersion, atten_r, atten_g, atten_b, atten_density, roughness, env_gain, light: _ } => {
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

const PRELUDE_WGSL: &str = r#"
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

// Phase 5b — Transmit helpers: Snell offset, frost taps, chromatic fan,
// Beer's-law travel. `sharp` is the raw parent snapshot (like the glass
// `tex()` path: unpremultiplied, `textureSampleLevel` LOD 0 — the
// entry scaffold guarantees non-uniform control flow is safe).
// `refr` is the ABSOLUTE refracted sample point in device px (callers
// pass `ctx.px + transmit_offset(..)`); all reads clamp to the surface
// dims (region-edge clamp, D-M8) exactly like `tex`.
fn transmit_path_len(ctx: EffectCtx) -> f32 {
    // Host path_length when a glass producer published it; the card-local
    // edge-distance proxy otherwise (standalone custom stages have no
    // Refract stage to write the depth field). Sharp-rect proxy on
    // purpose: Beer travel is a bulk estimate, not a rim field — the
    // corner-radius-aware field lives in `ctx_edge_dist_at`, which the
    // normal/edge lowerings call inline (module-scope helpers cannot
    // read the uniform block).
    return max(ctx.path_length, ctx_edge_dist(ctx));
}

fn transmit_offset(n: vec3<f32>, ior: f32, path: f32) -> vec2<f32> {
    // Snell at V=(0,0,1): transmitted dir ≈ -(1 - 1/ior)·n.xy per px of
    // travel. Flat (n.xy = 0) is an exact no-op; ior 1.0 degenerates to
    // no bending regardless of slope.
    let bend = max(1.0 - 1.0 / max(ior, 1.0), 0.0);
    return -n.xy * bend * path;
}

fn transmit_tap(sharp: texture_2d<f32>, refr: vec2<f32>, off: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(sharp));
    let uv = clamp((refr + off) / max(dims, vec2<f32>(1.0)), vec2<f32>(0.0), vec2<f32>(1.0));
    let s = textureSampleLevel(sharp, effect_sampler, uv, 0.0);
    let a = max(s.a, 1.0 / 255.0);
    return s.rgb / a;
}

fn transmit_sample(sharp: texture_2d<f32>, refr: vec2<f32>, frost: f32, disp: f32) -> vec3<f32> {
    // Quad-tap rotated spiral around the refracted point at radius
    // `frost` (the 5b frost kernel — the 8-tap golden-angle tail stays
    // future work in 5b-polish), then 3-tap R/B chromatic fan ±`disp`
    // around the base offset.
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
            let r = transmit_tap(sharp, refr, vec2<f32>(disp, 0.0));
            let b = transmit_tap(sharp, refr, vec2<f32>(-disp, 0.0));
            return vec3<f32>(r.r, base.g, b.b);
        }
        return base;
    }
    if (disp > 0.0) {
        let c = transmit_tap(sharp, refr, vec2<f32>(0.0));
        let r = transmit_tap(sharp, refr, vec2<f32>(disp, 0.0));
        let b = transmit_tap(sharp, refr, vec2<f32>(-disp, 0.0));
        return vec3<f32>(r.r, c.g, b.b);
    }
    return transmit_tap(sharp, refr, vec2<f32>(0.0));
}

fn transmit_beer(c: vec3<f32>, tint: vec3<f32>, density: f32, path: f32) -> vec3<f32> {
    // Beer's law over the travel: exp(-k·path) per channel via the tint
    // color; density 0 is an exact no-op (white tint, zero attenuation).
    if (density <= 0.0) {
        return c * tint;
    }
    let absorb = -log(max(tint, vec3<f32>(1e-3))) * density * path;
    return c * exp(-absorb);
}

// Phase 5e — procedural fields + emission. Hash-based value noise over
// card-local px (deterministic, tile-free; two octaves are enough for
// frost microstructure and glow-mask breakup), a periodic grid mask,
// and the shared point-lamp direction helper.
fn field_hash(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

fn field_vnoise(p: vec2<f32>) -> f32 {
    // Value noise: bilinear blend of hashed corners (smoothstep easing).
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
    // Two octaves: base + half-amplitude double-frequency detail,
    // centered on 0.5 so `FromHeight` gradients see zero mean.
    let q = p * scale;
    return (field_vnoise(q) * 0.667 + field_vnoise(q * 2.03 + vec2<f32>(17.0, 9.0)) * 0.333) - 0.5;
}

fn field_grid_mask(p: vec2<f32>, spacing: f32, width: f32) -> f32 {
    // Periodic grid lines: 1 inside a line, 0 in the cell interior.
    // Spacing/width in px; width >= spacing fills the cell (solid mask).
    let s = max(spacing, 1e-3);
    let g = abs(fract(p / s) - vec2<f32>(0.5)) * s;
    let d = min(g.x, g.y);
    return 1.0 - smoothstep(max(width * 0.5 - 1.0, 0.0), width * 0.5 + 1.0, d);
}

fn point_lamp_dir(anchor_px: vec2<f32>, frag_px: vec2<f32>, height: f32) -> vec3<f32> {
    // Card-local anchor → surface-space direction: the lamp hovers
    // `height` px above the anchor; +x right, +y DOWN in device px so
    // the y leg flips sign (surface +y is up).
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
        // D-M8 (Phase 5b): displacement reach — refraction travel over the
        // steepest expected slope plus frost radius plus dispersion fan.
        // Travel ≤ card half-diagonal is the conservative bound; frost
        // and dispersion ride on top. Region-edge clamping keeps the
        // taps inside the snapshot regardless.
        let frost = LowerCx::max_frost(expr);
        let disp = LowerCx::max_dispersion(expr);
        Some((frost + disp + 8.0).max(1.0))
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
// Effect plumbing: Surface rides the stack transport
// ---------------------------------------------------------------------------

use crate::reactive::IntoProp as _MaterialIntoProp;
use crate::reactive::Prop as _MaterialProp;

impl crate::foundation::IntoEffect for Surface {
    fn into_effect(self) -> Box<dyn crate::foundation::Effect> {
        // Inherent `Surface::into_effect` (the `CustomEffect` builder
        // above) — NOT this trait method (that would recurse forever and
        // blow the stack on every surface → empty card, floating text).
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
