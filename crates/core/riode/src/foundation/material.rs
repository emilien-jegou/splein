// Single responsibility: the `Material` container and the `Stage` contract.
//
// `riode` owns only the *contract*: a material is an ordered list of stages,
// a stage is something that can emit its WGSL fragment block and stream its
// uniform bytes against a resolved [`Geo`]. Concrete stages (shape, fill,
// shadow, stroke, frost, …) live in `riode-shader`, which depends on us.

use crate::foundation::geo::Geo;
use std::fmt;
use std::sync::Arc;

// Add to Stage trait in material.rs:
pub trait Stage: 'static {
    fn wgsl_source(&self) -> &'static str;
    fn uniform_size(&self) -> usize;
    fn write_uniforms(&self, geo: &Geo, buffer: &mut [u8]);
    fn requires_backdrop(&self) -> bool {
        false
    }
    fn blend_mode(&self) -> crate::foundation::blend::BlendMode {
        crate::foundation::blend::BlendMode::Normal
    }
}

/// A stage wrapper that attaches a shader-level blend mode.
pub struct BlendedStage<S> {
    pub stage: S,
    pub mode: crate::foundation::blend::BlendMode,
}

impl<S: Stage> Stage for BlendedStage<S> {
    fn wgsl_source(&self) -> &'static str {
        self.stage.wgsl_source()
    }
    fn uniform_size(&self) -> usize {
        self.stage.uniform_size()
    }
    fn write_uniforms(&self, geo: &Geo, buffer: &mut [u8]) {
        self.stage.write_uniforms(geo, buffer);
    }
    fn requires_backdrop(&self) -> bool {
        self.stage.requires_backdrop() || !self.mode.is_normal()
    }
    fn blend_mode(&self) -> crate::foundation::blend::BlendMode {
        self.mode
    }
}

/// Stage extension allowing `.blend(Blend::...)` on any stage.
pub trait StageExt: Stage + Sized {
    fn blend(self, mode: crate::foundation::blend::BlendMode) -> BlendedStage<Self> {
        BlendedStage { stage: self, mode }
    }
}

impl<S: Stage> StageExt for S {}

/// The ordered shading chain carried by a node — the sole carrier of visual
/// identity. The stage list is immutable once authored: equality and hashing
/// are identity of the shared list, so a `Material` clones for free.
#[derive(Clone)]
pub struct Material {
    pub stages: Arc<Vec<Box<dyn Stage>>>,
}

impl Material {
    pub fn empty() -> Self {
        Self {
            stages: Arc::new(Vec::new()),
        }
    }

    /// Builds a chain from any list of stages.
    pub fn collect<I, S>(stages: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Stage,
    {
        Self {
            stages: Arc::new(
                stages
                    .into_iter()
                    .map(|s| Box::new(s) as Box<dyn Stage>)
                    .collect(),
            ),
        }
    }

    /// Builds a chain from already-boxed stages (the `material=[…]` list form).
    pub fn from_boxes(stages: Vec<Box<dyn Stage>>) -> Self {
        Self {
            stages: Arc::new(stages),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    pub fn stages(&self) -> &[Box<dyn Stage>] {
        &self.stages
    }

    /// Identity equality: the chain is immutable once authored.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }
}

impl<T: Stage> From<Vec<T>> for Material {
    fn from(stages: Vec<T>) -> Self {
        Self {
            stages: Arc::new(
                stages
                    .into_iter()
                    .map(|s| Box::new(s) as Box<dyn Stage>)
                    .collect(),
            ),
        }
    }
}

impl<T: Stage, const N: usize> From<[T; N]> for Material {
    fn from(stages: [T; N]) -> Self {
        Self {
            stages: Arc::new(
                stages
                    .into_iter()
                    .map(|s| Box::new(s) as Box<dyn Stage>)
                    .collect(),
            ),
        }
    }
}

impl From<Vec<Box<dyn Stage>>> for Material {
    fn from(stages: Vec<Box<dyn Stage>>) -> Self {
        Self::from_boxes(stages)
    }
}

impl From<Box<dyn Stage>> for Material {
    fn from(stage: Box<dyn Stage>) -> Self {
        Self::from_boxes(vec![stage])
    }
}

impl PartialEq for Material {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }
}

impl fmt::Debug for Material {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Material")
            .field("stages", &self.stages.len())
            .finish()
    }
}

impl Default for Material {
    fn default() -> Self {
        Self::empty()
    }
}

impl<S: Stage> From<S> for Material {
    fn from(stage: S) -> Self {
        Self::collect([stage])
    }
}

/// `material={|geo: &Geo| …}`: the macro wraps the closure so it re-evaluates
/// per frame against the fresh `Geo`.
pub struct GeoFn<F>(pub F);

impl<F, S> Stage for GeoFn<F>
where
    F: Fn(&Geo) -> S + 'static,
    S: Stage,
{
    fn wgsl_source(&self) -> &'static str {
        (self.0)(&Geo::default()).wgsl_source()
    }

    fn uniform_size(&self) -> usize {
        (self.0)(&Geo::default()).uniform_size()
    }

    fn write_uniforms(&self, geo: &Geo, buffer: &mut [u8]) {
        (self.0)(geo).write_uniforms(geo, buffer);
    }

    fn requires_backdrop(&self) -> bool {
        (self.0)(&Geo::default()).requires_backdrop()
    }
}
