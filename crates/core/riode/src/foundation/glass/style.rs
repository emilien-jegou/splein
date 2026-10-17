// Single responsibility: a complete glass look as one engine value.
//
// `GlassStyle` is the application mechanism for userland presets: one
// struct holding the lens material plus its backdrop stages, expanded
// onto a group with `GroupDef::preset` (static or dynamic — a computed
// style drives the studio laboratory). The engine defines no looks
// itself; it only knows how to apply one.

use super::backdrop::Backdrop;
use super::refraction::Refraction;
use super::spectral::Spectral;
use super::glow::Glow;
use crate::foundation::shadow::Shadow;
use crate::foundation::Glass;

/// A complete glass look: the lens, its backdrop stages, the spectral
/// response, and the cast shadow, all explicit. `None` stages resolve
/// exactly like their absent-prop counterparts (neutral for
/// blur/wash/saturation/ambient, the reference curve for tone, absent for
/// the tone curve).
///
/// The stages live on a [`Backdrop`] rather than inline because every
/// one of them stays meaningful with the lens removed — which is the
/// test for "can work without the glass logic". The spectral response
/// rides here too: the rainbow is the wavelength-dependence of the
/// refraction, not a lens-material knob (v2 Phase 3), so the glass stays
/// monochromatic and `Spectral` optionally widens its refraction across
/// a spectrum. The cast shadow rides here as well: it is a plain drop
/// shadow owned by the general [`Shadow`] API (v2 Phase 1).
#[derive(Clone, Debug, PartialEq)]
pub struct GlassStyle {
    /// Lens + reference-geometry knobs.
    pub glass: Glass,
    /// The backdrop stages this look samples (core blur, wash,
    /// saturation, tone, ambient, fill). Explicit props win on
    /// conflict — last write is the prop layer.
    pub backdrop: Backdrop,
    /// The spectral (chromatic-dispersion) response: how the refraction
    /// splits by wavelength. `None`/neutral is monochrome (off).
    pub spectral: Spectral,
    /// The emissive bottom glow: light the slab carries itself, visible
    /// with no backdrop at all. `None`/neutral is off.
    pub glow: Glow,
    /// The outer refraction halo: blurred card light past the silhouette.
    /// Independent of `glow` (works with or without it). `None`/neutral
    /// is off.
    pub refraction: Refraction,
    /// The cast shadow, painted behind the glass by the general `Shadow`
    /// path. `None` casts no shadow (the old in-shader stage is retired).
    pub shadow: Option<Shadow>,
}

impl GlassStyle {
    /// A bare lens: neutral stages everywhere, reference tone curve,
    /// monochrome refraction, no cast shadow.
    pub const fn bare(glass: Glass) -> Self {
        Self {
            glass,
            backdrop: Backdrop::NONE,
            spectral: Spectral::NONE,
            glow: Glow::NONE,
            refraction: Refraction::NONE,
            shadow: None,
        }
    }

    /// A look carrying an explicit backdrop alongside its lens.
    pub const fn with_backdrop(glass: Glass, backdrop: Backdrop) -> Self {
        Self {
            glass,
            backdrop,
            spectral: Spectral::NONE,
            glow: Glow::NONE,
            refraction: Refraction::NONE,
            shadow: None,
        }
    }

    /// A look carrying an explicit spectral (rainbow) response.
    pub const fn with_spectral(mut self, spectral: Spectral) -> Self {
        self.spectral = spectral;
        self
    }

    /// A look carrying an explicit emissive bottom glow.
    pub const fn with_glow(mut self, glow: Glow) -> Self {
        self.glow = glow;
        self
    }

    /// A look carrying an explicit outer refraction halo.
    pub const fn with_refraction(mut self, refraction: Refraction) -> Self {
        self.refraction = refraction;
        self
    }

    /// A look carrying soft pastel light spilling below the bottom edge.
    /// `length` is stored as the equivalent halo reach (÷32) on the
    /// shared wire; prefer this over `with_refraction` for new code.
    pub const fn with_light_spill(mut self, spill: super::light_spill::LightSpill) -> Self {
        self.refraction = Refraction {
            strength: spill.strength,
            reach: match spill.length {
                Some(l) => Some(l / 32.0),
                None => None,
            },
            softness: spill.softness,
        };
        self
    }
}
