// Single responsibility: Tiny-Skia CPU rasterization backend for the Aurora UI engine.
//
// Depends on `aurora` for the display list, geometry, and typography types; implements
// the CPU presentation backend plus the headless rasterizer and CPU test harness.

pub fn backend_name() -> &'static str {
    "tiny-skia"
}
