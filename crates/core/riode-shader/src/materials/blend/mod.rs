pub const BLEND_WGSL: &str = include_str!("blend.wgsl");
pub const BLEND_SEPARABLE_WGSL: &str = include_str!("blend_separable.wgsl");
pub const BLEND_NONSEPARABLE_WGSL: &str = include_str!("blend_nonseparable.wgsl");
pub const COMPOSITE_BLEND_WGSL: &str = include_str!("composite_blend.wgsl");

/// The complete backdrop-blend composite module:
/// separable + nonseparable + dispatcher + composite_blend.
pub const BLEND_COMPOSITE_COMBINED_WGSL: &str = concat!(
    include_str!("blend_nonseparable.wgsl"),
    "\n",
    include_str!("blend_separable.wgsl"),
    "\n",
    include_str!("blend.wgsl"),
    "\n",
    include_str!("composite_blend.wgsl"),
);
