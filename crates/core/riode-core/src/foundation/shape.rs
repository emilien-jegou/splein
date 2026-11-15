// Single responsibility: Abstract shape container contract (exact twin of Material).

use crate::foundation::Stage;
use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct ShapeContainer {
    pub stages: Arc<Vec<Box<dyn Stage>>>,
}

impl ShapeContainer {
    pub fn empty() -> Self {
        Self {
            stages: Arc::new(Vec::new()),
        }
    }

    pub fn new<S: Stage>(stage: S) -> Self {
        Self {
            stages: Arc::new(vec![Box::new(stage)]),
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
}

impl Default for ShapeContainer {
    fn default() -> Self {
        Self::empty()
    }
}

impl<S: Stage> From<S> for ShapeContainer {
    fn from(stage: S) -> Self {
        Self::new(stage)
    }
}

impl<S: Stage, const N: usize> From<[S; N]> for ShapeContainer {
    fn from(stages: [S; N]) -> Self {
        Self::collect(stages)
    }
}

impl From<Vec<Box<dyn Stage>>> for ShapeContainer {
    fn from(stages: Vec<Box<dyn Stage>>) -> Self {
        Self::from_boxes(stages)
    }
}

impl PartialEq for ShapeContainer {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stages, &other.stages)
    }
}

impl fmt::Debug for ShapeContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shape")
            .field("stages", &self.stages.len())
            .finish()
    }
}
