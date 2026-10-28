//! a set of common imports, useful when writing pipelines.
//!
//! intended to be used in files where pipelines are defined like this:
//!
//! `use riode_shader::prelude::*`
use crate as riode_shader;

// make these traits usable
pub use riode_shader::GpuIndex as _; // gpu_array.index(4u32)
pub use riode_shader::GpuLayout as _; // t.gpu_layout()
pub use riode_shader::CpuLayout as _; // t.cpu_layout()
pub use riode_shader::ToGpuType as _; // t.to_gpu()
pub use riode_shader::ToScalar as _; // 1.0_f32.splat()

pub use riode_shader::{x1, x2, x3, x4};
pub use riode_shader::Size;

pub use riode_shader::GradPrecision::Coarse;
pub use riode_shader::GradPrecision::Fine;
