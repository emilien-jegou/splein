// Single responsibility: Module boundary for backend-agnostic display list primitives.

pub mod chunk;
pub mod command;
pub mod context;
pub mod image;
pub mod scene;
pub mod vector;

pub use chunk::SceneChunk;
pub use command::{LayerId, SceneCommand};
pub use context::PaintContext;
pub use image::ImageSource;
pub use scene::Scene;
pub use vector::VectorGraphic;
