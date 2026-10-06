// Single responsibility: Vello GPU rasterization backend for the Aurora UI engine.
//
// Depends on `aurora` for the display list, geometry, and typography types; implements
// the GPU presentation backend.

pub fn backend_name() -> &'static str {
    "vello"
}
