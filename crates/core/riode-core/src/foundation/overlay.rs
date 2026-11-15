// Single responsibility: Container overlay stage chain applied after children, clipped to shape.

use crate::foundation::material::{Material, Stage};
use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct Overlay {
    pub stages: Arc<Vec<Box<dyn Stage>>>,
}

impl Overlay {
    pub fn empty() -> Self {
        Self {
            stages: Arc::new(Vec::new()),
        }
    }

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

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }

    pub fn to_material(&self) -> Material {
        Material {
            stages: Arc::clone(&self.stages),
        }
    }
}

impl PartialEq for Overlay {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }
}

impl fmt::Debug for Overlay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Overlay")
            .field("stages", &self.stages.len())
            .finish()
    }
}

impl Default for Overlay {
    fn default() -> Self {
        Self::empty()
    }
}

impl<S: Stage> From<S> for Overlay {
    fn from(stage: S) -> Self {
        Self::collect([stage])
    }
}

impl<T: Stage, const N: usize> From<[T; N]> for Overlay {
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

impl<T: Stage> From<Vec<T>> for Overlay {
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

impl From<Vec<Box<dyn Stage>>> for Overlay {
    fn from(stages: Vec<Box<dyn Stage>>) -> Self {
        Self::from_boxes(stages)
    }
}

impl From<Box<dyn Stage>> for Overlay {
    fn from(stage: Box<dyn Stage>) -> Self {
        Self::from_boxes(vec![stage])
    }
}
