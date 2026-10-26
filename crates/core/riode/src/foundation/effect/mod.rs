pub mod shaders;

// Single responsibility: composable effect-layer vocabulary — primitives the
// renderer executes in stack order, builtins and program-supplied customs.
//
// An effect is one render pass with the standard plug shape (WGSL):
//   fn apply(ctx: EffectCtx, backdrop, sharp, uniforms) -> EffectCtx
// `EffectCtx { color, uv, depth, normal, coverage }` travels between stages;
// `depth`/`coverage` is what makes ClipRound-as-effect work (the clip writes
// them, later effects read them). Order in `EffectStack` is execution order.

/// Stable cache key for an effect: pipeline cache + compositor IPC cache.
/// Builtins hash (stage, params); customs hash the WGSL source + uniforms.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectId(pub u64);

/// What backdrop texture a custom shader needs, if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackdropNeed {
    None,
    ReadSharp,
    ReadBlurred { sigma: f32 },
}

/// One program-supplied custom shader stage: arbitrary WGSL implementing the
/// fixed `apply` ABI, plus packed uniforms (vec4-array convention).
#[derive(Clone, Debug, PartialEq)]
pub struct CustomShader {
    pub wgsl: String,
    pub uniforms: Vec<f32>,
    pub backdrop: BackdropNeed,
}

use crate::foundation::{
    Backdrop, BlendMode, Filter, FilterOp, Glass, GlassLayer, Glow, LightEnvironment, Radius,
    Refraction, Shadow, Spectral,
};

/// Builtin primitive stages. Param-carrying variants (`Glass`, `Blend`,
/// `Shadow`, `ClipRound`) own the material data the executor lowers into
/// passes; the fine-grained glass stages (`Refract`…`Press`) are the
/// Phase C split of today's monolithic optics pass.
#[derive(Clone, Debug, PartialEq)]
pub enum BuiltinStage {
    Blur { sigma: f32 },
    ColorMatrix { matrix: [f32; 20] },
    ClipRound { radius: Radius },
    Glass(GlassParams),
    Blend { mode: BlendMode },
    Shadow(ShadowParams),
    // Phase C split of the glass optics pass. Authorable unit items
    // (`backdrop_effects=[Refract(), …]`): they carry no knobs — the
    // material rides `Glass`, the fold collects their stage ids into the
    // pass's dispatch list (see `glass_stage_ids`).
    Refract,
    Frost,
    Spectral,
    Tone,
    Saturation,
    Wash,
    Dim,
    Tint,
    Ambient,
    FillDimTint,
    Glow,
    Rim,
    Press,
    Fresnel,
    Ggx,
    EnvReflect,
    LightSpill,
    CardBlend { mode_id: u32 },
    /// Group-root marker: merges descendant members' glass into one
    /// shared layer (spacing = smin merge radius factor input). Consumed
    /// by compile; the executor emits no pass for it.
    GlassGroup { spacing: f32 },
}

impl BuiltinStage {
    /// The glass-pass stage ids this unit contributes to the fold's
    /// dispatch list (`fs_main`'s switch: 1 refract … 13 press), in
    /// canonical run order. `Tone` carries its small-shape partner
    /// (shapeTone rides the same author item). Empty for stages that are
    /// not listable glass units (material, blur, blend, markers).
    pub fn glass_stage_ids(&self) -> &'static [u32] {
        match self {
            BuiltinStage::Refract => &[1],
            BuiltinStage::Frost => &[2],
            BuiltinStage::Spectral => &[3],
            BuiltinStage::Saturation => &[4],
            BuiltinStage::Tone => &[5, 8],
            BuiltinStage::Wash => &[6],
            BuiltinStage::Dim => &[7],
            BuiltinStage::Tint => &[9],
            BuiltinStage::Ambient => &[10],
            BuiltinStage::Glow => &[11],
            BuiltinStage::Rim => &[12],
            BuiltinStage::Press => &[13],
            BuiltinStage::Fresnel => &[14],
            BuiltinStage::Ggx => &[15],
            BuiltinStage::EnvReflect => &[16],
            BuiltinStage::FillDimTint => &[6, 7, 9],
            _ => &[],
        }
    }
}

/// Material bundle a `Glass` effect refracts through: today's `GlassLayer`
/// value verbatim, so the executor's lowering is byte-identical to the
/// current optics path until Phase C splits it.
#[derive(Clone, Debug, PartialEq)]
pub struct GlassParams {
    pub layer: GlassLayer,
}

/// `BlendMode` carrier for the in-shader card/content composite.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowParams {
    pub shadow: Shadow,
    pub light: LightEnvironment,
}

/// Builtin (known WGSL source) vs custom (program-supplied WGSL).
#[derive(Clone, Debug, PartialEq)]
pub enum EffectShader {
    Builtin(BuiltinStage),
    Custom(CustomShader),
}

/// One composable effect layer. Object-safe so stacks are heterogeneous
/// (`Vec<Box<dyn Effect>>`); call sites write plain structs, no enum wrapper.
pub trait Effect: Send + Sync + std::fmt::Debug {
    fn id(&self) -> EffectId;
    fn needs_backdrop(&self) -> bool;
    fn pad(&self, scale: f32) -> f32;
    fn shader(&self) -> EffectShader;
    /// Object-safe clone for `EffectStack: Clone` (needed by `Prop<T>`).
    fn clone_box(&self) -> Box<dyn Effect>;
}

/// Convert anything effect-like into a boxed trait object — the single
/// vocabulary for heterogeneous stacks, replacing manual `.boxed()`
/// calls: `vec![Blur::new(4.0).into_effect(), Saturate::new(1.3).into_effect()]`.
/// (A blanket `From<T> for Box<dyn Effect>` is impossible: it would
/// overlap the reflexive `From` once any downstream crate implements
/// `Effect` for a boxed type.)
pub trait IntoEffect {
    fn into_effect(self) -> Box<dyn Effect>;
}

impl<T: Effect + 'static> IntoEffect for T {
    fn into_effect(self) -> Box<dyn Effect> {
        Box::new(self)
    }
}

/// Sheet-safe vocabulary: effects that make sense on the layer's own
/// rasterized content (`layer_effects`, CSS `filter`). Cannot read the
/// backdrop — a transparent sheet has nothing to refract.
pub trait LayerEffect: Effect {
    /// Box for a heterogeneous `layer_effects=[...]` list.
    fn into_layer_effect(self) -> Box<dyn Effect>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// Backdrop-side vocabulary: effects that transform the sampled
/// background (`backdrop_effects`, CSS `backdrop-filter` plus the glass
/// lens). The lens card is what the layer's sheet composites over.
/// `Glass` lives here only — it is a backdrop phenomenon.
pub trait BackdropEffect: Effect {
    /// Box for a heterogeneous `backdrop_effects=[...]` list.
    fn into_backdrop_effect(self) -> Box<dyn Effect>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// Convert anything stack-like into an [`EffectStack`]: a single effect
/// becomes a singleton, a stack passes through, a boxed vec collects.
pub trait IntoStack {
    fn into_stack(self) -> EffectStack;
}

impl IntoStack for EffectStack {
    fn into_stack(self) -> EffectStack {
        self
    }
}

impl<T: IntoEffect> IntoStack for T {
    fn into_stack(self) -> EffectStack {
        EffectStack(vec![self.into_effect()])
    }
}

impl IntoStack for Vec<Box<dyn Effect>> {
    fn into_stack(self) -> EffectStack {
        EffectStack(self)
    }
}

/// Fluent chaining for every effect primitive:
/// `Blur::new(6.0).then(Glass::clear())`,
/// `effect.then(stack)` and `stack.then(effect)` all yield stacks.
pub trait ThenExt: IntoEffect + Sized {
    fn then<I: IntoStack>(self, other: I) -> EffectStack {
        let mut stack = self.into_stack();
        stack.append(other.into_stack());
        stack
    }
}

impl<T: IntoEffect> ThenExt for T {}

/// Ordered effect stack: index order is execution order (bottom → top).
#[derive(Debug, Default)]
pub struct EffectStack(pub Vec<Box<dyn Effect>>);

impl Clone for EffectStack {
    fn clone(&self) -> Self {
        Self(self.0.iter().map(|e| e.clone_box()).collect())
    }
}

impl PartialEq for EffectStack {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(other.0.iter())
                .all(|(a, b)| a.id() == b.id())
    }
}

impl From<Vec<Box<dyn Effect>>> for EffectStack {
    fn from(items: Vec<Box<dyn Effect>>) -> Self {
        Self(items)
    }
}

impl EffectStack {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push<E: Effect + 'static>(&mut self, e: E) {
        self.0.push(Box::new(e));
    }

    /// Concatenate another stack behind this one (execution order).
    pub fn append(&mut self, other: EffectStack) {
        self.0.extend(other.0);
    }

    /// Builder form of [`Self::append`]: `a.then(b)` runs `a` first.
    /// Accepts stacks, single effects, and boxed vecs alike.
    pub fn then<I: IntoStack>(mut self, other: I) -> Self {
        self.append(other.into_stack());
        self
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Total offscreen margin in device px: max over member pads.
    pub fn pad_total(&self, scale: f32) -> f32 {
        self.0
            .iter()
            .map(|e| e.pad(scale))
            .fold(0.0, f32::max)
    }

    /// Whether any stage snapshots the backdrop.
    pub fn needs_backdrop_layer(&self) -> bool {
        self.0.iter().any(|e| e.needs_backdrop())
    }

    /// Group-root spacing, if the stack declares a glass group.
    pub fn glass_group_spacing(&self) -> Option<f32> {
        self.0.iter().find_map(|e| match e.shader() {
            EffectShader::Builtin(BuiltinStage::GlassGroup { spacing }) => Some(spacing),
            _ => None,
        })
    }

    /// Lowering for engines until Phase D executes the stack directly:
    /// the stack's blur sigmas as `Blur` ops and each color stage as a
    /// raw `Matrix` op, in order. `None` when the stack carries no
    /// filter-like stage (the engine then skips the offscreen).
    pub fn to_filter(&self) -> Option<Filter> {
        let mut ops = Vec::new();
        for e in self.0.iter() {
            match e.shader() {
                EffectShader::Builtin(BuiltinStage::Blur { sigma }) => {
                    if sigma > 0.0 {
                        ops.push(FilterOp::Blur(sigma));
                    }
                }
                EffectShader::Builtin(BuiltinStage::ColorMatrix { matrix }) => {
                    ops.push(FilterOp::Matrix(matrix));
                }
                _ => {}
            }
        }
        if ops.is_empty() {
            None
        } else {
            Some(Filter::from_ops(ops))
        }
    }

    /// First `Glass` stage's single-material components, if the stack's
    /// glass is a lone lens (not a group): `(glass, backdrop, spectral,
    /// glow, refraction)`.
    pub fn glass_single(
        &self,
    ) -> Option<(Glass, Backdrop, Spectral, Glow, Refraction)> {
        self.0.iter().find_map(|e| match e.shader() {
            EffectShader::Builtin(BuiltinStage::Glass(p)) => match &p.layer {
                GlassLayer::Single(g, b, s, gl, r) => Some((*g, *b, *s, *gl, *r)),
                GlassLayer::Group(_) => None,
            },
            _ => None,
        })
    }

    /// First `Glass` stage params, if the stack refracts a backdrop.
    /// Engines and island extraction lower through this until Phase D
    /// executes the stack directly.
    pub fn glass_layer(&self) -> Option<GlassLayer> {
        self.0.iter().find_map(|e| match e.shader() {
            EffectShader::Builtin(BuiltinStage::Glass(p)) => Some(p.layer.clone()),
            _ => None,
        })
    }

    /// First `Blend` stage mode, if the stack composites with a blend mode.
    pub fn blend_mode(&self) -> Option<BlendMode> {
        self.0.iter().find_map(|e| match e.shader() {
            EffectShader::Builtin(BuiltinStage::Blend { mode }) => Some(mode),
            _ => None,
        })
    }
}

/// FNV-1a 64 over bytes: stable `EffectId` without external deps.
fn hash_id(bytes: &[u8]) -> EffectId {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    EffectId(h)
}

/// Separable Gaussian blur primitive (H then V passes in the executor).
#[derive(Clone, Debug, PartialEq)]
pub struct Blur {
    pub sigma: f32,
}

impl Blur {
    pub fn new(sigma: f32) -> Self {
        Self {
            sigma: sigma.max(0.0),
        }
    }
}

impl Effect for Blur {
    fn id(&self) -> EffectId {
        { let mut b = vec![0x01]; b.extend_from_slice(&self.sigma.to_bits().to_le_bytes()); hash_id(&b) }
    }

    fn needs_backdrop(&self) -> bool {
        false
    }

    fn pad(&self, scale: f32) -> f32 {
        3.0 * self.sigma * scale
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Blur { sigma: self.sigma })
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Raw 4x5 color-matrix primitive (a `ColorMatrix` pass in the executor).
/// Carries a pre-combined matrix (e.g. from `Filter::color_matrix()`).
#[derive(Clone, Debug, PartialEq)]
pub struct ColorMatrix {
    pub matrix: [f32; 20],
}

impl ColorMatrix {
    pub fn new(matrix: [f32; 20]) -> Self {
        Self { matrix }
    }
}

impl Effect for ColorMatrix {
    fn id(&self) -> EffectId {
        let mut b = vec![0x03];
        for x in self.matrix {
            b.extend_from_slice(&x.to_bits().to_le_bytes());
        }
        hash_id(&b)
    }

    fn needs_backdrop(&self) -> bool {
        false
    }

    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::ColorMatrix {
            matrix: self.matrix,
        })
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Saturation adjustment primitive (a `ColorMatrix` pass in the executor).
#[derive(Clone, Debug, PartialEq)]
pub struct Saturate {
    pub amount: f32,
}

impl Saturate {
    pub fn new(amount: f32) -> Self {
        Self { amount }
    }

    /// Row-major 4x5 CSS saturate matrix.
    pub fn matrix(&self) -> [f32; 20] {
        let s = self.amount;
        let m = [
            0.213 + 0.787 * s,
            0.715 - 0.715 * s,
            0.072 - 0.072 * s,
            0.213 - 0.213 * s,
            0.715 + 0.285 * s,
            0.072 - 0.072 * s,
            0.213 - 0.213 * s,
            0.715 - 0.715 * s,
            0.072 + 0.928 * s,
        ];
        [
            m[0], m[1], m[2], 0.0, 0.0, //
            m[3], m[4], m[5], 0.0, 0.0, //
            m[6], m[7], m[8], 0.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }
}

impl Effect for Saturate {
    fn id(&self) -> EffectId {
        { let mut b = vec![0x02]; b.extend_from_slice(&self.amount.to_bits().to_le_bytes()); hash_id(&b) }
    }

    fn needs_backdrop(&self) -> bool {
        false
    }

    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::ColorMatrix {
            matrix: self.matrix(),
        })
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

fn debug_id(tag: u8, v: &impl std::fmt::Debug) -> EffectId {
    let mut b = vec![tag];
    b.extend_from_slice(format!("{v:?}").as_bytes());
    hash_id(&b)
}

/// Liquid-glass refraction effect: snapshots the backdrop and refracts it
/// through the material's SDF. Carries today's `GlassLayer` value so the
/// executor's lowering is byte-identical until Phase C splits the optics.
#[derive(Clone, Debug, PartialEq)]
pub struct GlassEffect {
    pub params: GlassParams,
}

impl GlassEffect {
    pub fn single(
        glass: Glass,
        backdrop: Backdrop,
        spectral: Spectral,
        glow: Glow,
        refraction: Refraction,
    ) -> Self {
        Self {
            params: GlassParams {
                layer: GlassLayer::Single(glass, backdrop, spectral, glow, refraction),
            },
        }
    }

    pub fn new(layer: GlassLayer) -> Self {
        Self {
            params: GlassParams { layer },
        }
    }
}

impl Effect for GlassEffect {
    fn id(&self) -> EffectId {
        debug_id(0x10, &self.params.layer)
    }

    fn needs_backdrop(&self) -> bool {
        true
    }

    fn pad(&self, scale: f32) -> f32 {
        self.params.layer.device_pad(scale)
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Glass(self.params.clone()))
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// CSS mix-blend-mode composite against the backdrop snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendEffect {
    pub mode: BlendMode,
}

impl BlendEffect {
    pub fn new(mode: BlendMode) -> Self {
        Self { mode }
    }
}

impl Effect for BlendEffect {
    fn id(&self) -> EffectId {
        debug_id(0x11, &self.mode)
    }

    fn needs_backdrop(&self) -> bool {
        !self.mode.is_normal()
    }

    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Blend { mode: self.mode })
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Alpha-silhouette shadow cast from the layer's finished content.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowEffect {
    pub params: ShadowParams,
}

impl ShadowEffect {
    pub fn new(shadow: Shadow, light: LightEnvironment) -> Self {
        Self {
            params: ShadowParams { shadow, light },
        }
    }
}

impl Effect for ShadowEffect {
    fn id(&self) -> EffectId {
        debug_id(0x12, &self.params.shadow)
    }

    fn needs_backdrop(&self) -> bool {
        !self.params.shadow.blend.is_normal()
    }

    fn pad(&self, scale: f32) -> f32 {
        self.params.shadow.max_extent() * scale
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Shadow(self.params.clone()))
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Rounded-rect clip as an effect: writes SDF `depth`/`coverage` into the
/// context for downstream stages (glass refracts inside it, shadows cast
/// from it). Replaces `Radius`-as-prop on the layer path.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipRound {
    pub radius: Radius,
}

impl ClipRound {
    pub fn new(radius: Radius) -> Self {
        Self { radius }
    }
}

impl Effect for ClipRound {
    fn id(&self) -> EffectId {
        debug_id(0x13, &self.radius)
    }

    fn needs_backdrop(&self) -> bool {
        false
    }

    fn pad(&self, _scale: f32) -> f32 {
        1.5
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::ClipRound {
            radius: self.radius,
        })
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Glass-group root marker: merges descendant members' glass into one
/// shared layer. Consumed by compile (`enclosing_glass_group`); the
/// executor emits no pass for it.
#[derive(Clone, Debug, PartialEq)]
pub struct GlassGroupEffect {
    pub spacing: f32,
}

impl GlassGroupEffect {
    pub fn new(spacing: f32) -> Self {
        Self {
            spacing: spacing.max(0.0),
        }
    }
}

impl Effect for GlassGroupEffect {
    fn id(&self) -> EffectId {
        debug_id(0x14, &self.spacing.to_bits())
    }

    fn needs_backdrop(&self) -> bool {
        false
    }

    fn pad(&self, scale: f32) -> f32 {
        self.spacing * scale
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::GlassGroup {
            spacing: self.spacing,
        })
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

// Unit items of the glass pass: authorable in `backdrop_effects=[...]`,
// no knobs of their own — the material (strength, sigmas, colours) rides
// the stack's `Glass` item, the fold collects their stage ids (one per
// item, `Tone` two) into the pass's dispatch list. They never open a
// pass of their own: `Glass` owns the snapshot (`needs_backdrop`).

/// Lens displacement — the backdrop read position (`stageRefract`).
#[derive(Clone, Debug, PartialEq)]
pub struct Refract();

impl Effect for Refract {
    fn id(&self) -> EffectId {
        debug_id(0x20, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Refract)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Frost core / post-lens / wide taps — also the pass's backdrop read
/// (`stageFrost` fills `col` from the sample point).
#[derive(Clone, Debug, PartialEq)]
pub struct Frost();

impl Effect for Frost {
    fn id(&self) -> EffectId {
        debug_id(0x21, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Frost)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Per-wavelength widening of the sample (`stageSpectral`).
#[derive(Clone, Debug, PartialEq)]
pub struct Disperse();

impl Effect for Disperse {
    fn id(&self) -> EffectId {
        debug_id(0x22, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Spectral)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Luma-keyed chroma grade (`stageSaturation`; distinct from the
/// `Saturate` ColorMatrix item — that one is a `layer_effects` pass).
#[derive(Clone, Debug, PartialEq)]
pub struct Saturation();

impl Effect for Saturation {
    fn id(&self) -> EffectId {
        debug_id(0x23, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Saturation)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Tone LUT plus the small-shape tone curve (`stageToneCurve` +
/// `stageShapeTone` — one author item, two stage ids).
#[derive(Clone, Debug, PartialEq)]
pub struct Tone();

impl Effect for Tone {
    fn id(&self) -> EffectId {
        debug_id(0x24, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Tone)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Fill wash over the card (`stageWash`).
#[derive(Clone, Debug, PartialEq)]
pub struct Wash();

impl Effect for Wash {
    fn id(&self) -> EffectId {
        debug_id(0x25, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Wash)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Post-tone multiply (`stageDim`).
#[derive(Clone, Debug, PartialEq)]
pub struct Dim();

impl Effect for Dim {
    fn id(&self) -> EffectId {
        debug_id(0x26, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Dim)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Flat tint mix (`stageTint`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tint();

impl Effect for Tint {
    fn id(&self) -> EffectId {
        debug_id(0x27, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Tint)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Backdrop hue pickup (`stageAmbient`).
#[derive(Clone, Debug, PartialEq)]
pub struct Ambient();

impl Effect for Ambient {
    fn id(&self) -> EffectId {
        debug_id(0x28, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Ambient)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Emissive voice-glow wash (`stageGlow`). Named `GlowEffect` because
/// `Glow` is the material struct.
#[derive(Clone, Debug, PartialEq)]
pub struct GlowEffect();

impl Effect for GlowEffect {
    fn id(&self) -> EffectId {
        debug_id(0x29, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Glow)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Specular streak + isotropic rim (`stageRim`).
#[derive(Clone, Debug, PartialEq)]
pub struct Rim();

impl Effect for Rim {
    fn id(&self) -> EffectId {
        debug_id(0x2A, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Rim)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Schlick Fresnel edge glint (`stageFresnel`) — the first consumer of
/// the D1 substrate widen: `F = F0 + (1−F0)·(1−N·V)^power` off the
/// 3-D normal's z against the flat view. Knobs ride the `L` bank
/// (`fresnel` on `Glass`); the material carries them, the item is the
/// stack position.
#[derive(Clone, Debug, PartialEq)]
pub struct Fresnel();

impl Effect for Fresnel {
    fn id(&self) -> EffectId {
        debug_id(0x2B, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Fresnel)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// GGX microfacet specular glint (`stageGGX`) — the directional lobe the
/// dot-product rim streak fakes. Knobs ride the appended GGX tail
/// (`ggx`/`ggx_roughness`/`ggx_light_dir` on `Glass`); the material
/// carries them, the item is the stack position.
#[derive(Clone, Debug, PartialEq)]
pub struct Ggx();

impl Effect for Ggx {
    fn id(&self) -> EffectId {
        debug_id(0x2C, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::Ggx)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

/// Environment reflection (`stageEnvReflect`) — the backdrop mirror on
/// the steep bevel plus the sky lift on the flat face, blended by
/// Fresnel off `n3` (the blend darkens the transmitted image where the
/// mirror takes over — what separates this from every additive fake).
/// Knobs ride the appended env tail (`env_reflect` + `env_sky` on
/// `Glass`); the material carries them, the item is the stack position.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvReflect();

impl Effect for EnvReflect {
    fn id(&self) -> EffectId {
        debug_id(0x2D, self)
    }
    fn needs_backdrop(&self) -> bool {
        false
    }
    fn pad(&self, _scale: f32) -> f32 {
        0.0
    }
    fn shader(&self) -> EffectShader {
        EffectShader::Builtin(BuiltinStage::EnvReflect)
    }
    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

// Vocabulary impls: what may be authored in `layer_effects=[...]` (the
// sheet) versus `backdrop_effects=[...]` (the sampled backdrop + lens).
// `Glass` is backdrop-only: it refracts the backdrop, so it has no
// meaning on a transparent sheet.

impl LayerEffect for Blur {}
impl LayerEffect for ColorMatrix {}
impl LayerEffect for Saturate {}
impl LayerEffect for ShadowEffect {}

impl BackdropEffect for Blur {}
impl BackdropEffect for ColorMatrix {}
impl BackdropEffect for Saturate {}
impl BackdropEffect for GlassEffect {}
impl BackdropEffect for BlendEffect {}
impl BackdropEffect for Refract {}
impl BackdropEffect for Frost {}
impl BackdropEffect for Disperse {}
impl BackdropEffect for Saturation {}
impl BackdropEffect for Tone {}
impl BackdropEffect for Wash {}
impl BackdropEffect for Dim {}
impl BackdropEffect for Tint {}
impl BackdropEffect for Ambient {}
impl BackdropEffect for GlowEffect {}
impl BackdropEffect for Rim {}
impl BackdropEffect for Fresnel {}
impl BackdropEffect for Ggx {}
impl BackdropEffect for EnvReflect {}

/// A program-supplied custom stage as an effect — the material DSL's
/// lowering target. Phase 0 hand-writes the WGSL (a passthrough surface
/// for the mechanism demo); Phase 1's `view!` material form emits the
/// same shape from the closure algebra. Backdrop-side: it reads the
/// snapshot/backdrop like glass, so it rides `backdrop_effects`.
///
/// Identity (D-M5): the `EffectId` folds the WGSL source *and* the
/// uniform values, so a value change (hover) repaints without a
/// recompile, while the pipeline cache keys on the source alone.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomEffect {
    pub shader: CustomShader,
}

impl CustomEffect {
    pub fn new(
        wgsl: impl Into<String>,
        uniforms: Vec<f32>,
        backdrop: BackdropNeed,
    ) -> Self {
        Self {
            shader: CustomShader {
                wgsl: wgsl.into(),
                uniforms,
                backdrop,
            },
        }
    }
}

impl Effect for CustomEffect {
    fn id(&self) -> EffectId {
        let mut b = vec![0x30];
        b.extend_from_slice(self.shader.wgsl.as_bytes());
        for u in &self.shader.uniforms {
            b.extend_from_slice(&u.to_bits().to_le_bytes());
        }
        hash_id(&b)
    }

    fn needs_backdrop(&self) -> bool {
        !matches!(self.shader.backdrop, BackdropNeed::None)
    }

    fn pad(&self, _scale: f32) -> f32 {
        // Conservative refraction reach (D-M8). Phase 0's passthrough
        // displaces nothing; a real surface reports its own reach here
        // so the snapshot region covers displaced samples. 0 until the
        // DSL's IR wires one through.
        0.0
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Custom(self.shader.clone())
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

impl BackdropEffect for CustomEffect {}
