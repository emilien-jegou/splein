// Single responsibility: Module boundary and re-exports for the TinySkia CPU rasterization backend.

pub mod clip;
pub mod command;
pub mod image;
pub mod layer;
pub mod path;
pub mod renderer;
pub mod shader;
pub mod shadow;
pub mod stroke;
pub mod svg;
pub mod text;

pub use clip::ClipStack;
pub use command::{execute_commands, CommandContext};
pub use layer::LayerCompositor;
pub use path::build_rounded_path;
pub use renderer::TinySkiaRenderer;
pub use shader::build_paint;
pub use shadow::ShadowRasterizer;
pub use stroke::render_stroke;
pub use svg::SvgCache;
pub use text::{render_text, TextRenderParams};
