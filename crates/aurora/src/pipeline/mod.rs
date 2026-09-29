// Single responsibility: Module boundary and re-exports for the 7-stage engine pipeline.

pub mod stage1_invalidate;
pub mod stage2_layout;
pub mod stage3_damage;
pub mod stage4_overlap;
pub mod stage5_compile;
pub mod stage6_raster;
pub mod stage7_present;

pub use stage2_layout::LayoutResult;
pub use stage3_damage::DamagePlan;
pub use stage4_overlap::OverlapPlan;
pub use stage5_compile::{compile_scene_instrumented, CompileResult};
pub use stage6_raster::RasterReceipt;
pub use stage7_present::PresentReceipt;
