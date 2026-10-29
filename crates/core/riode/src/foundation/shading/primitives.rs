// Single responsibility: the Chain-spec stage primitives — the
// author-facing names (`Refract`, `Frost`, …) as `IntoStage` data, plus
// the 256-byte `GlassUniformBuffer` packer (plan-better-glass-setup.md
// Part 2 normative primitive set).
//
// Status (Step 4, slice M1): only `Refract` is ready. The remaining
// Chain-spec primitives land with their modular_glass.wgsl stages —
// their names are reserved here, not half-built on the old path.
//
// Authoring (Chain spec Patterns B/C):
//
// ```ignore
// material=[
//     |geo: &Geo| Refract::new().band(geo.half_min() * 0.35),
//     Frost::new().blur(8.0),
// ]
// ```
//
// Closures run per frame — the closure IS the path (plan Part 3 (4)):
// keep them pure and O(1). Branching on geometry is a plain Rust `if`
// inside the closure. There is no uniform-slot stamping machinery, no
// `Value::GeoField`, no expression system: the CPU resolves `Geo` per
// layout tick and each stage writes its bank into the fixed block.

use std::sync::Arc;

use crate::foundation::color::Color;
use crate::foundation::geo::Geo;

/// Bits of the `stage_mask` — authoritative (plan P1). The GPU's
/// single static shader branches `if ((mask & STAGE_*) != 0)`.
/// (`Refract`=1 grows with each primitive; the Chain-spec `StageId`
/// scale tops out at `MouseHighlight`=128.)
pub mod stage_mask {
    /// `stageRefract` — the lens displacement + 3-D normal + slab path.
    pub const REFRACT: u32 = 1;
    /// `stageFrost` — the backdrop pre-blur + post-lens Jacobian blur
    /// + wide tail (Q6b: stub stage until the frost math lands).
    pub const FROST: u32 = 2;
    /// `stageBeer` — Beer-Lambert tint over the sampled backdrop.
    pub const BEER: u32 = 4;
    /// `stageFill` — the solid-colour fill: the purge's one paint
    /// primitive. Returns the authored colour at the silhouette's
    /// coverage instead of reading the backdrop.
    pub const FILL: u32 = 8;
    // Reserved bits for the remaining Chain-spec primitives (land with
    // their shader stages; do not hand out casually):
    // DIRECTIONAL_RIM = 16, SPECULAR_GLINT = 32,
    // ENV_REFLECT = 64, TONE_CURVE = 128, MOUSE_HIGHLIGHT = 256.
}

/// Fixed block size: 16 × vec4 = 256 B (plan H5: one card → one 256 B
/// block → one pass).
pub const K_MODULAR_FLOATS: usize = 64;
pub const K_MODULAR_VEC4S: usize = K_MODULAR_FLOATS / 4;

// Block bank slots (each stage owns its slot range; the WGSL
// `modular_glass.wgsl` mirror is named identically — keep in sync).
// F32 indices into the flat 64-float block: vec4 row `n` occupies
// floats 4n..4n+4, so a bank at vec4 row `n` is `4*n`:
//   w0 (f32 0..4):   stage_mask (as f32), card w, card h (device px), dpr
//   w1 (f32 4..8):   corner tl, tr, br, bl (device px)
//   w2 (f32 8..12):  Refract A — strength, band px, decay px, ior
//   w3 (f32 12..16): Refract B — size ref px, lens edge px, edge decay
//        px, normal radius scale
//   w4 (f32 16..20): card origin x, y (device px), high-contrast flag,
//        corner exponent (0 = the WGSL global 2.0) — written by the
//        pass, not the stages: position is frame context, not geometry.
//   w5 (f32 20..24):  Frost A — blur px, tail radius px, tail strength,
//        spare
//   w6 (f32 24..28):  Frost spare
//   w7 (f32 28..32):  Beer — tint r, g, b, density
//   w8 (f32 32..36):  Fill — r, g, b, a (straight)
//   w9–w15 (f32 36..64): reserved banks (Rim 36–43, Glint 44–51, Env
//        52–59, spare 60–63)
pub const W_STAGEMASK: usize = 0;
pub const W_CORNERS: usize = 4;
pub const W_REFRACT_A: usize = 8;
pub const W_REFRACT_B: usize = 12;
pub const W_ORIGIN: usize = 16;
pub const W_FROST: usize = 20;
pub const W_BEER: usize = 28;
pub const W_FILL: usize = 32;

/// A data stage: writes its bank into the fixed block and ORs its
/// `stage_mask` bit into `block[W_STAGEMASK]` itself (the mask row is
/// a float; WGSL reads it via `u32()`). `Refract`, `Frost`, …
/// implement this; closures returning a stage are adapted into it by
/// [`IntoStage`] (see [`StageFn`]). `pad` is the stage's outward reach
/// in logical px (the offscreen AABB margin), `sigma` the backdrop
/// pre-blur it needs (Refract deviates none — M1 ships 0 until Frost
/// lands with a backdrop-needing surface).
pub trait Stage: 'static {
    fn shader(&self) -> &'static str;
    fn write_block(&self, geo: &Geo, block: &mut [f32; K_MODULAR_FLOATS], dpr: f32);
    /// Outward reach past the element rect, logical px (AA fringe
    /// included by the chain, not per stage).
    fn pad(&self, geo: &Geo) -> f32 {
        let _ = geo;
        0.0
    }
    /// Backdrop pre-blur the pass must compose before this stage's
    /// pass (0 = the raw snapshot). Author-time metadata: geo-
    /// independent by contract, so the pass can size its pre-blur
    /// before any frame resolves.
    fn sigma(&self) -> f32 {
        0.0
    }
}

/// The author-side surface (plan H3): everything a `material=[…]`
/// item can be — a primitive, or a closure `|geo: &Geo| -> stage`.
/// The compiled form is `Box<dyn Stage>`.
pub trait IntoStage {
    fn into_stage(self) -> Box<dyn Stage>;
}

impl<S: Stage> IntoStage for S {
    fn into_stage(self) -> Box<dyn Stage> {
        Box::new(self)
    }
}

/// The closure adapter: evaluated per frame against the fresh `Geo` —
/// the closure IS the compile path (plan Part 3 (4)). Pure + O(1):
/// a multiply, a struct literal, an `if` off geometry.
pub struct StageFn<F>(pub F);

impl<F, S> Stage for StageFn<F>
where
    F: Fn(&Geo) -> S + 'static,
    S: Stage,
{
    fn shader(&self) -> &'static str {
        self.0(&Geo::new(0.0, 0.0, [0.0; 4], 0.0)).shader()
    }
    fn write_block(&self, geo: &Geo, block: &mut [f32; K_MODULAR_FLOATS], dpr: f32) {
        (self.0)(geo).write_block(geo, block, dpr);
    }

    fn pad(&self, geo: &Geo) -> f32 {
        (self.0)(geo).pad(geo)
    }

    fn sigma(&self) -> f32 {
        let g = geo_placeholder();
        (self.0)(&g).sigma()
    }
}

// `sigma` is author-time metadata (the pass decides its pre-blur
// before any frame resolves). Closures must publish it statically: a
// placeholder Geo (1×1 capsule) evaluates the closure once at chain
// build. Documented contract: a stage's `sigma` is geo-independent.
fn geo_placeholder() -> Geo {
    Geo::new(1.0, 1.0, [0.5, 0.5, 0.5, 0.5], 0.0)
}

/// The compiled-per-frame uniform block contents.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ChainFrame {
    /// Shader source to use for this material pass.
    pub shader: &'static str,
    /// `stage_mask` bit union of every stage in the chain.
    pub mask: u32,
    /// The fixed 256 B block (packed, device px lengths).
    pub block: [f32; K_MODULAR_FLOATS],
    /// Union outward reach (logical px) + the AA fringe.
    pub pad: f32,
    /// Backdrop pre-blur sigma (logical px); 0 = raw snapshot.
    pub sigma: f32,
}

/// Compile an ordered stage list against the fresh `Geo` — the shared
/// compile body behind [`Material::resolve`]: zeroed block, base rows
/// (size/dpr/corners), then each stage writes its bank and ORs its
/// mask bit.
fn resolve_stages(stages: &[Box<dyn Stage>], geo: &Geo, dpr: f32) -> ChainFrame {
    let mut block = [0.0f32; K_MODULAR_FLOATS];
    let mut pad = 0.0f32;
    let mut sigma = 0.0f32;
    let shader = stages.first().map(|s| s.shader()).unwrap_or(riode_shader::materials::MODULAR_GLASS_WGSL);
    block[W_STAGEMASK + 1] = geo.width() * dpr;
    block[W_STAGEMASK + 2] = geo.height() * dpr;
    block[W_STAGEMASK + 3] = dpr;
    let corners = geo.corners;
    block[W_CORNERS] = corners[0] * dpr;
    block[W_CORNERS + 1] = corners[1] * dpr;
    block[W_CORNERS + 2] = corners[2] * dpr;
    block[W_CORNERS + 3] = corners[3] * dpr;
    for stage in stages {
        stage.write_block(geo, &mut block, dpr);
        pad = pad.max(stage.pad(geo));
        sigma = sigma.max(stage.sigma());
    }
    ChainFrame {
        shader,
        mask: block[W_STAGEMASK] as u32,
        block,
        pad: pad + 2.0,
        sigma,
    }
}

/// The material carrier (plan Q2): an owned, clone-cheap ordered stage
/// chain. `Clone` shares the boxes via `Arc` — stages are immutable
/// after authoring; their closures re-evaluate per frame against the
/// fresh `Geo`, so sharing is sound. (`Prop<Material>` needs exactly
/// this: one `Arc` bump per clone.)
///
/// The plan's Q2 shape reads `Arc<Vec<Box<dyn IntoStage>>>`; the stored
/// form here is the *compiled* stage box (`IntoStage::into_stage` has
/// run once at authoring). The authoring surface stays `IntoStage` /
/// [`IntoMaterial`] — per-frame resolution needs `write_block` on a
/// `&self` (no re-boxing per frame), which only `Box<dyn Stage>`
/// provides.
#[derive(Clone, Default)]
pub struct Material {
    pub stages: Arc<Vec<Box<dyn Stage>>>,
    /// Whether the lens reacts to touch (press deformation).
    /// `false` by default; `material.interactive()` opts in.
    /// (Interaction feedback lives in the chain too — see
    /// `HighlightOverMouse` — this flag only gates the press deformation.)
    pub interactive: bool,
}

impl Material {
    /// Flatten any number of author-side items into one chain (Q3):
    /// primitives, `|geo|` closures, presets (nested `Material`s), and
    /// nested lists — in order.
    pub fn collect<I>(items: I) -> Self
    where
        I: IntoIterator,
        I::Item: IntoMaterial,
    {
        let mut stages = Vec::new();
        for item in items {
            item.push_stages(&mut stages);
        }
        Self {
            stages: Arc::new(stages),
            interactive: false,
        }
    }

    /// Assemble a chain from already-boxed stages — the macro's
    /// `material=[…]` block form pushes into a `Vec` one item at a
    /// time (heterogeneous items, ordered) and hands it here.
    pub fn from_stages(stages: Vec<Box<dyn Stage>>) -> Self {
        Self {
            stages: Arc::new(stages),
            interactive: false,
        }
    }

    /// Opt this material's lens into press deformation (the old
    /// `Glass::interactive`): the router press machinery deforms the
    /// card shape while the press is active.
    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    #[allow(dead_code)]
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Compile against the fresh `Geo` (plan P2): the frame's uniform
    /// block, per-frame, closures included.
    pub fn resolve(&self, geo: &Geo, dpr: f32) -> ChainFrame {
        resolve_stages(&self.stages, geo, dpr)
    }
}

/// A `Material` composes as one nested chain stage: its `write_block`
/// loops its own stages, so presets collect as single stages and
/// resolution recurses through them — flat semantics, no box cloning.
impl Stage for Material {
    fn shader(&self) -> &'static str {
        if let Some(f) = self.stages.first() {
            f.shader()
        } else {
            riode_shader::materials::MODULAR_GLASS_WGSL
        }
    }
    fn write_block(
        &self,
        geo: &Geo,
        block: &mut [f32; K_MODULAR_FLOATS],
        dpr: f32,
    ) {
        for stage in self.stages.iter() {
            stage.write_block(geo, block, dpr);
        }
    }

    fn pad(&self, geo: &Geo) -> f32 {
        self.stages
            .iter()
            .map(|stage| stage.pad(geo))
            .fold(0.0, f32::max)
    }

    fn sigma(&self) -> f32 {
        self.stages
            .iter()
            .map(|stage| stage.sigma())
            .fold(0.0, f32::max)
    }
}

impl PartialEq for Material {
    /// Stages are opaque closures — equality is identity of the shared
    /// list, the honest cheap comparison for a reference carrier.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }
}

impl std::fmt::Debug for Material {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Material")
            .field("stage_count", &self.stages.len())
            .finish_non_exhaustive()
    }
}

/// The `material=[…]` item surface (Q3): anything the bracket list can
/// collect — a stage primitive, a `|geo|` closure, or a preset (a
/// nested `Material`, which composes as one nested chain stage) —
/// flattened in order into one chain. Mirrors [`IntoStage`]'s blanket
/// shape; a `Stage` is never a closure, so the impls never overlap.
pub trait IntoMaterial {
    fn push_stages(self, out: &mut Vec<Box<dyn Stage>>);
}

impl<S: Stage> IntoMaterial for S {
    fn push_stages(self, out: &mut Vec<Box<dyn Stage>>) {
        out.push(Box::new(self));
    }
}

impl<F, S> IntoMaterial for GeoFn<F>
where
    F: Fn(&Geo) -> S + 'static,
    S: Stage,
{
    fn push_stages(self, out: &mut Vec<Box<dyn Stage>>) {
        out.push(Box::new(StageFn(self.0)));
    }
}

/// A `|geo: &Geo| -> …` closure authored in `material={…}`. The
/// `view!` macro wraps the closure at `::riode::foundation::GeoFn`;
/// the closure re-evaluates per frame against the fresh `Geo` — the
/// closure IS the compile path (plan Part 3 (4)). Its return can be
/// any `Stage` (a primitive) or a whole `Material` (a preset / a
/// geometry branch — `if geo.is_capsule() { … } else { … }`).
pub struct GeoFn<F>(pub F);

impl<F, S> GeoFn<F>
where
    F: Fn(&Geo) -> S + 'static,
    S: Stage,
{
    /// Evaluate against a specific `Geo` — the compile pass calls this
    /// per frame with the node's resolved geometry.
    #[allow(dead_code)]
    pub fn eval(&self, geo: &Geo) -> S {
        (self.0)(geo)
    }
}
