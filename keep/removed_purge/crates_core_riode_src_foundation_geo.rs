// Single responsibility: the Chain-spec `Geo` — the CPU-resolved
// geometry snapshot (plan-better-glass-setup.md Part 2, P2 verbatim).
//
// One struct, one home. Stamped at layout/tick from the node's actual
// bounds; every per-frame restamp flows through the same invalidation
// path as every other prop. Material closures read it —
// `material=[|geo: &Geo| …]` — and compile GPU-uniform *data*; the
// shader never changes per composition. Branching on geometry is a
// plain Rust `if` inside the closure, not a value language:
//
// ```ignore
// material=[|geo: &Geo| {
//     if geo.is_capsule() { Frost::new().blur(8.0) } else { Frost::new().blur(5.9) }
// }]
// ```
//
// Prohibited bug class (plan Part 3): caching any Geo-derived number
// keyed off anything other than the current layout tick — there is no
// "resolved at first render" geometry. `Geo` owns no cache, Weak, or
// signal: it is a plain value the emitter re-derives every frame, and
// closures re-compile their stage data through it.

/// The geometry snapshot a material stage compiles against: size, the
/// resolved per-corner radii, and the superellipse exponent — all in
/// logical px, plain data, `Copy`.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Geo {
    /// The node's actual layout width / height (logical px).
    pub size: [f32; 2],
    /// Resolved corner radii (tl, tr, br, bl) — the W3C resolution of
    /// the node's declared corners, each capped at half the shorter
    /// side, in logical px. Pill ⇄ asymmetric transitions re-resolve
    /// this per frame; the 4-corner SDF consumes it directly.
    pub corners: [f32; 4],
    /// Superellipse exponent of the shape (0 = use the engine global).
    pub exponent: f32,
}

impl Geo {
    /// A snapshot from the node's actual bounds. `corners` should come
    /// from the same resolution the renderer performs (W3C resolve +
    /// half-side cap) so closures author against the drawn silhouette.
    pub fn new(width: f32, height: f32, corners: [f32; 4], exponent: f32) -> Self {
        Self {
            size: [width, height],
            corners,
            exponent,
        }
    }

    /// Width / height in logical px.
    pub fn width(&self) -> f32 {
        self.size[0]
    }

    pub fn height(&self) -> f32 {
        self.size[1]
    }

    /// Half the shorter side — the band/frost size unit the Chain spec
    /// fits against (`band = geo.half_min() * 0.35`).
    pub fn half_min(&self) -> f32 {
        self.size[0].min(self.size[1]) / 2.0
    }

    /// Aspect ratio (width / height), clamped away from 0.
    pub fn aspect(&self) -> f32 {
        self.size[1].max(1e-6) / self.size[0].max(1e-6)
    }

    /// Whether the shape renders as a capsule/pill: every corner at the
    /// cap (half the shorter side). Radius is static at the DSL (H2);
    /// it is *geometry* per frame here because width signals drive it.
    pub fn is_capsule(&self) -> bool {
        let cap = self.size[0].min(self.size[1]) / 2.0;
        self.corners.iter().all(|c| *c >= cap.max(0.0) - 1e-3)
    }
}
