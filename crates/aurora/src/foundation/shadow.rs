// Single responsibility: Elevation-based shadow specification with layered falloff.

use crate::foundation::color::Color;
use crate::foundation::falloff::FalloffCurve;
use crate::foundation::light::LightEnvironment;

/// How a shadow composites with the surface beneath it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    /// Standard source-over alpha blending (CSS default).
    #[default]
    Normal,
    /// Subtractive blending: deepens the underlying color instead of
    /// washing it toward grey.
    Multiply,
}

/// Which silhouette the shadow is cast from.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ShadowShape {
    /// Bounding-box silhouette (CSS `box-shadow`).
    #[default]
    Box,
    /// Alpha-channel silhouette (CSS `filter: drop-shadow()`).
    ///
    /// Accepted in v1 but rasterized as `Box`; the offscreen alpha
    /// pass is deferred work.
    Alpha,
}

/// Where the shadow offset comes from.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ShadowOffset {
    /// Derived from elevation + light direction via [`LightEnvironment::offset_for`].
    Elevation,
    /// Explicit pixel offset (CSS `box-shadow` semantics).
    Explicit { offset_x: f32, offset_y: f32 },
}

/// One composited falloff layer inside a [`Shadow`].
///
/// Internal to the engine: users add layers through [`Shadow::ease_out`]
/// and [`Shadow::physical`], never by constructing this directly.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ShadowLayer {
    pub falloff: FalloffCurve,
    /// Blur radius at the occluder edge, px.
    pub softness: f32,
    /// How far the shadow reaches, px.
    pub extent: f32,
    /// Peak opacity at the occluder, 0..1.
    pub opacity: f32,
    /// Explicit per-layer offset; `None` inherits the shadow-level offset
    /// (elevation-derived or `web_like`). Needed for CSS-style stacked
    /// shadows where each layer carries its own offset.
    pub offset: Option<(f32, f32)>,
}

/// Complete shadow specification: one shadow per node, internally layered.
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow {
    /// Height above the surface, px. Drives derived softness/extent and,
    /// with [`ShadowOffset::Elevation`], the offset direction.
    pub elevation: f32,
    pub color: Color,
    pub blend: BlendMode,
    pub shape: ShadowShape,
    pub offset: ShadowOffset,
    layers: Vec<ShadowLayer>,
}

impl Shadow {
    /// The common case: elevation only, everything else derived.
    ///
    /// Produces a single `EaseOut` layer (`softness = 0.75 * elevation`,
    /// `extent = 2 * elevation`, `opacity = 0.25`) with `Multiply` blending.
    pub fn elevation(elevation: f32) -> Self {
        Self {
            elevation,
            color: Color::BLACK,
            blend: BlendMode::Multiply,
            shape: ShadowShape::Box,
            offset: ShadowOffset::Elevation,
            layers: vec![ShadowLayer {
                falloff: FalloffCurve::EaseOut,
                softness: elevation * 0.75,
                extent: elevation * 2.0,
                opacity: 0.25,
                offset: None,
            }],
        }
    }

    /// CSS-compatible shadow with explicit offset and tight `Linear` falloff.
    ///
    /// Escape hatch for `box-shadow: offset_x offset_y blur color` parity;
    /// prefer [`Shadow::elevation`] for new code.
    ///
    /// The kernel radius is half the CSS blur radius: the rasterizer runs
    /// two box iterations (sigma ≈ 1.15r, support ±2r), so r = blur/2 lands
    /// sigma ≈ 0.58R with support ±R, close to the browser's Gaussian.
    pub fn web_like(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            elevation: 0.0,
            color,
            blend: BlendMode::Normal,
            shape: ShadowShape::Box,
            offset: ShadowOffset::Explicit {
                offset_x,
                offset_y,
            },
            layers: vec![ShadowLayer {
                falloff: FalloffCurve::Linear,
                softness: (blur * 0.5).max(0.0),
                extent: blur.max(0.0),
                // Alpha lives in `color`; the layer multiplier stays neutral
                // so the peak matches the declared color exactly once.
                opacity: 1.0,
                offset: None,
            }],
        }
    }

    /// Empty multi-layer shadow with CSS compositing (`Normal` blend).
    ///
    /// Starting point for stacked drop-shadows; add layers with [`Shadow::drop`].
    pub fn layered(color: Color) -> Self {
        Self {
            elevation: 0.0,
            color,
            blend: BlendMode::Normal,
            shape: ShadowShape::Box,
            offset: ShadowOffset::Explicit {
                offset_x: 0.0,
                offset_y: 0.0,
            },
            layers: Vec::new(),
        }
    }

    /// Adds one CSS `drop-shadow(offset_x offset_y blur)` layer sharing the
    /// shadow color (including its alpha) with `Linear` falloff.
    pub fn drop(mut self, offset_x: f32, offset_y: f32, blur: f32) -> Self {
        self.layers.push(ShadowLayer {
            falloff: FalloffCurve::Linear,
            softness: (blur * 0.5).max(0.0),
            extent: blur.max(0.0),
            opacity: 1.0,
            offset: Some((offset_x, offset_y)),
        });
        self
    }

    /// Adds an `EaseOut` falloff layer.
    pub fn ease_out(mut self, softness: f32, extent: f32, opacity: f32) -> Self {
        self.layers.push(ShadowLayer {
            falloff: FalloffCurve::EaseOut,
            softness,
            extent,
            opacity,
            offset: None,
        });
        self
    }

    /// Adds an inverse-square-inspired `Physical` falloff layer.
    pub fn physical(mut self, softness: f32, extent: f32, opacity: f32) -> Self {
        self.layers.push(ShadowLayer {
            falloff: FalloffCurve::Physical,
            softness,
            extent,
            opacity,
            offset: None,
        });
        self
    }

    pub fn multiply(mut self) -> Self {
        self.blend = BlendMode::Multiply;
        self
    }

    pub fn normal(mut self) -> Self {
        self.blend = BlendMode::Normal;
        self
    }

    pub fn alpha(mut self) -> Self {
        self.shape = ShadowShape::Alpha;
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }

    /// Internal layers for the renderer.
    pub fn layers(&self) -> &[ShadowLayer] {
        &self.layers
    }

    /// Resolves the pixel offset under `light`.
    pub fn resolve_offset(&self, light: &LightEnvironment) -> (f32, f32) {
        match self.offset {
            ShadowOffset::Elevation => light.offset_for(self.elevation),
            ShadowOffset::Explicit {
                offset_x,
                offset_y,
            } => (offset_x, offset_y),
        }
    }

    /// Resolves one layer's pixel offset: its explicit offset wins,
    /// otherwise the shadow-level offset.
    pub fn layer_offset(
        &self,
        light: &LightEnvironment,
        layer: &ShadowLayer,
    ) -> (f32, f32) {
        layer.offset.unwrap_or_else(|| self.resolve_offset(light))
    }

    /// Kernel padding for one layer: covers the declared reach and the
    /// blur kernel support, whichever is larger. Shared by damage bounds
    /// and both rasterizers so all three agree on the footprint.
    pub fn layer_pad(layer: &ShadowLayer) -> f32 {
        layer.extent.max(3.0 * layer.softness.max(0.5))
    }

    /// Largest layer extent, for damage bounds and kernel padding.
    pub fn max_extent(&self) -> f32 {
        self.layers
            .iter()
            .map(|l| l.extent.max(l.softness))
            .fold(0.0, f32::max)
    }

    /// Largest layer softness, for blur kernel sizing.
    pub fn max_softness(&self) -> f32 {
        self.layers
            .iter()
            .map(|l| l.softness)
            .fold(0.0, f32::max)
    }
}

/// Polymorphic shadow constructor for the DSL: `f32`, `(f32, Color)`, or `Shadow`.
pub trait IntoShadow {
    fn into_shadow(self) -> Shadow;
}

impl IntoShadow for f32 {
    fn into_shadow(self) -> Shadow {
        Shadow::elevation(self)
    }
}

impl IntoShadow for (f32, Color) {
    fn into_shadow(self) -> Shadow {
        Shadow::elevation(self.0).color(self.1)
    }
}

impl IntoShadow for Shadow {
    fn into_shadow(self) -> Shadow {
        self
    }
}
