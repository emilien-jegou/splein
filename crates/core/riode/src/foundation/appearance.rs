// Aggregate visual surface appearance specification.

use crate::foundation::blend::BlendMode;
use crate::foundation::fill::Fill;
use crate::foundation::filter::Filter;
use crate::foundation::glass::{Backdrop, Glass, Glow, Refraction, Spectral};
use crate::foundation::light::LightEnvironment;
use crate::foundation::radius::Radius;
use crate::foundation::shadow::Shadow;
use crate::foundation::stroke::Stroke;

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    pub radius: Radius,
    /// One shadow per node, internally layered.
    pub shadow: Option<Shadow>,
    /// Per-subtree light override. `None` (the case for ~all nodes) inherits
    /// the ambient light resolved during the paint/damage traversal; the
    /// resolved value lives in tree state, never here, so overrides never
    /// dirty unaffected nodes.
    pub light_override: Option<LightEnvironment>,
    /// Element filter chain (CSS `filter`). `None` renders unfiltered;
    /// `Some(Filter::EMPTY)` is a no-op filter that still forces a layer
    /// (useful for tests). A filter implies an offscreen layer: the node's
    /// content renders isolated, the filter applies to that isolated
    /// result, then it composites — so blur extends past the element's
    /// own bounds without smearing whatever is behind it.
    pub filter: Option<Filter>,
    /// Liquid-glass lens material. `Some` turns the node into a
    /// backdrop-consuming lens layer: the engine snapshots the pixels
    /// behind the element, refracts them through the material's SDF
    /// height field, and composites the result — the node's own content
    /// draws above it, unrefracted. Orthogonal to `filter`.
    pub glass: Option<Glass>,
    /// The non-glass backdrop stages the lens samples (core blur, wash,
    /// saturation, tone, ambient, fill). Rides alongside `glass` rather
    /// than inside it: every stage here stays meaningful if the lens is
    /// removed entirely, which is the test for "can work without the
    /// glass logic". `None` means the neutral backdrop (sharp,
    /// unsaturated, untoned).
    pub backdrop: Option<Backdrop>,
    /// The spectral (chromatic-dispersion) response of the glass
    /// refraction — the rainbow a dispersive medium throws at its rim.
    /// Rides alongside `glass` rather than inside it: it is the
    /// wavelength-dependence of the refraction, not a lens-material
    /// knob, so the glass stays monochromatic and this optionally widens
    /// its refraction across a spectrum. `None` means monochrome (off).
    pub spectral: Option<Spectral>,
    /// The emissive bottom glow of the glass slab. Rides alongside
    /// `glass` like `spectral`: light the slab emits itself, so it needs
    /// no backdrop. `None` means off.
    pub glow: Option<Glow>,
    /// The outer refraction halo: blurred card light past the silhouette
    /// (theatre mode). Independent of `glow` — works with or without it.
    /// `None` means off.
    pub refraction: Option<Refraction>,
    pub opacity: f32,
    /// How the node's isolated layer composites against its backdrop
    /// (CSS blend modes). `Normal` (default) is plain source-over — the
    /// only mode a fixed-function blend can express — and takes the
    /// pre-blend path untouched. Anything else forces an offscreen layer
    /// whose content composites in-shader against a snapshot of the
    /// backdrop. Ignored on glass layers (see the wgpu engine: glass-card
    /// blend is separate work).
    pub blend: BlendMode,
}

impl Appearance {
    pub const EMPTY: Self = Self {
        fill: None,
        stroke: None,
        radius: Radius::ZERO,
        shadow: None,
        light_override: None,
        filter: None,
        glass: None,
        backdrop: None,
        spectral: None,
        glow: None,
        refraction: None,
        opacity: 1.0,
        blend: BlendMode::Normal,
    };

    pub fn with_fill(mut self, fill: Fill) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub fn with_radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    pub fn with_light(mut self, light: LightEnvironment) -> Self {
        self.light_override = Some(light);
        self
    }

    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn with_glass(mut self, glass: Glass) -> Self {
        self.glass = Some(glass);
        self
    }

    pub fn with_backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    pub fn with_spectral(mut self, spectral: Spectral) -> Self {
        self.spectral = Some(spectral);
        self
    }

    pub fn with_glow(mut self, glow: Glow) -> Self {
        self.glow = Some(glow);
        self
    }

    pub fn with_refraction(mut self, refraction: Refraction) -> Self {
        self.refraction = Some(refraction);
        self
    }

    pub fn with_light_spill(mut self, spill: crate::foundation::LightSpill) -> Self {
        self.refraction = Some(Refraction {
            strength: spill.strength,
            reach: match spill.length {
                Some(l) => Some(l / 32.0),
                None => None,
            },
            softness: spill.softness,
        });
        self
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn with_blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self::EMPTY
    }
}
