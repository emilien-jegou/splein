// Single responsibility: Module boundary and re-exports for the Vello GPU compute backend.

pub use command::{compile_vello_chunk, VelloImageCache};
pub use context::GpuContext;
pub use renderer::VelloRenderer;
pub use shadow::VelloShadowCache;
pub use svg_paint::convert_usvg_paint;

pub mod command;
pub mod context;
pub mod path;
pub mod renderer;
pub mod shader;
pub mod shadow;
pub mod svg;
pub mod svg_paint;
pub mod text;
