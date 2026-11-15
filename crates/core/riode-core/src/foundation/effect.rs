// Single responsibility: Container subtree post-processing effect and composite blend chain.

use crate::foundation::blend::BlendMode;
use crate::foundation::geo::Geo;
use std::fmt;
use std::sync::Arc;

pub trait EffectStage: 'static {
    /// Injected WGSL fragment logic or stage function.
    fn wgsl_source(&self) -> &'static str;

    /// Uniform byte alignment & size requirements.
    fn uniform_size(&self) -> usize;

    /// Writes raw uniform bytes for this frame against the resolved layout Geo.
    fn write_uniforms(&self, geo: &Geo, buffer: &mut [u8]);

    /// Returns a subtree blend mode if this effect stage dictates layer compositing blend.
    fn blend_mode(&self) -> Option<BlendMode> {
        None
    }

    /// Whether this effect needs the parent backdrop snapshot.
    fn requires_backdrop(&self) -> bool {
        self.blend_mode().is_some_and(|m| !m.is_normal())
    }
}

/// BlendMode acts as a first-class EffectStage:
impl EffectStage for BlendMode {
    fn wgsl_source(&self) -> &'static str {
        ""
    }
    fn uniform_size(&self) -> usize {
        0
    }
    fn write_uniforms(&self, _geo: &Geo, _buffer: &mut [u8]) {}
    fn blend_mode(&self) -> Option<BlendMode> {
        Some(*self)
    }
    fn requires_backdrop(&self) -> bool {
        !self.is_normal()
    }
}

/// An ordered chain of post-processing effects and composite blends.
#[derive(Clone)]
pub struct Effect {
    pub stages: Arc<Vec<Box<dyn EffectStage>>>,
}

impl Effect {
    pub fn empty() -> Self {
        Self {
            stages: Arc::new(Vec::new()),
        }
    }

    pub fn collect<I, S>(stages: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: EffectStage,
    {
        Self {
            stages: Arc::new(
                stages
                    .into_iter()
                    .map(|s| Box::new(s) as Box<dyn EffectStage>)
                    .collect(),
            ),
        }
    }

    pub fn from_boxes(stages: Vec<Box<dyn EffectStage>>) -> Self {
        Self {
            stages: Arc::new(stages),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    pub fn stages(&self) -> &[Box<dyn EffectStage>] {
        &self.stages
    }

    pub fn composite_blend(&self) -> BlendMode {
        for s in self.stages.iter().rev() {
            if let Some(m) = s.blend_mode() {
                return m;
            }
        }
        BlendMode::Normal
    }

    pub fn requires_backdrop(&self) -> bool {
        self.stages.iter().any(|s| s.requires_backdrop())
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Effect")
            .field("stages", &self.stages.len())
            .field("blend", &self.composite_blend())
            .finish()
    }
}

impl Default for Effect {
    fn default() -> Self {
        Self::empty()
    }
}

impl<S: EffectStage> From<S> for Effect {
    fn from(stage: S) -> Self {
        Self::collect([stage])
    }
}

impl From<Vec<Box<dyn EffectStage>>> for Effect {
    fn from(stages: Vec<Box<dyn EffectStage>>) -> Self {
        Self::from_boxes(stages)
    }
}

impl From<Box<dyn EffectStage>> for Effect {
    fn from(stage: Box<dyn EffectStage>) -> Self {
        Self::from_boxes(vec![stage])
    }
}

impl<T: EffectStage, const N: usize> From<[T; N]> for Effect {
    fn from(stages: [T; N]) -> Self {
        Self {
            stages: Arc::new(
                stages
                    .into_iter()
                    .map(|s| Box::new(s) as Box<dyn EffectStage>)
                    .collect(),
            ),
        }
    }
}

impl<T: EffectStage> From<Vec<T>> for Effect {
    fn from(stages: Vec<T>) -> Self {
        Self {
            stages: Arc::new(
                stages
                    .into_iter()
                    .map(|s| Box::new(s) as Box<dyn EffectStage>)
                    .collect(),
            ),
        }
    }
}
