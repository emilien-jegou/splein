// Single responsibility: the spectral (chromatic-dispersion) response of
// the glass refraction — the rainbow a dispersive medium throws at its rim.
//
// This is deliberately NOT part of `Glass`. Dispersion is the *wavelength-
// dependence of refraction*, not a lens-material knob: the glass material
// (`Glass`) is monochromatic (frost, tone, fill, rim), and it outputs a
// single refraction offset. `Spectral` optionally *widens that offset
// across a spectrum* and recombines it. The rainbow is the OUTPUT of
// spectral refraction, never an input to the glass material — the same
// separation `Shadow` (Phase 1) and `Backdrop` (Phase 0) established.
//
// The shader consumes these knobs through the packed bank's spectral
// slots (the Q block, retired in Phase 2). When `spread` is 0/absent the
// shader takes the single-tap monochromatic path, so an unset `Spectral`
// costs nothing and changes nothing.

use super::constants::GlassBank;

/// The spectral response: how the glass refraction splits by wavelength.
///
/// `None` on every field (or [`Spectral::NONE`]) means monochrome — the
/// glass refracts a single sample, exactly as before. A rainbow needs a
/// nonzero [`Self::spread`] (and a [`Self::reach`] that gates its extent);
/// [`Self::ior`] sets the base index the spread deviates from.
///
/// ```ignore
/// Spectral::NONE                                  // monochrome (off)
/// Spectral::new().spread(0.02).reach(1.0)         // a rim rainbow
/// Spectral::new().ior(1.52).spread(0.03)          // a denser crown glass
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Spectral {
    /// Base index of refraction the spread deviates from (`None` ⇒ 1.5,
    /// ordinary glass). Only meaningful with a nonzero [`Self::spread`].
    pub ior: Option<f32>,
    /// Dispersion strength: how much the index varies across the spectrum
    /// (the reference's `uDisp`). 0/`None` = monochrome, no rainbow.
    pub spread: Option<f32>,
    /// How far the rainbow extends relative to the lens displacement
    /// (1.0 = the full lens bend spreads; smaller reigns it in toward the
    /// neutral bend). `None` ⇒ 1.0.
    pub reach: Option<f32>,
}

impl Default for Spectral {
    fn default() -> Self {
        Self::NONE
    }
}

impl Spectral {
    /// Monochrome: every field neutral, the glass refracts a single sample.
    pub const NONE: Self = Self {
        ior: None,
        spread: None,
        reach: None,
    };

    /// The neutral starting point. `Spectral::new().spread(0.02)` turns on
    /// a rainbow and leaves `ior`/`reach` at their defaults.
    pub const fn new() -> Self {
        Self::NONE
    }

    /// Base index of refraction (`None` ⇒ 1.5, ordinary glass).
    pub const fn ior(mut self, v: f32) -> Self {
        self.ior = Some(v.max(1.0));
        self
    }

    /// Dispersion strength (0 = monochrome, no rainbow).
    pub const fn spread(mut self, v: f32) -> Self {
        self.spread = Some(v);
        self
    }

    /// How far the rainbow extends relative to the lens displacement.
    pub const fn reach(mut self, v: f32) -> Self {
        self.reach = Some(v.max(0.0));
        self
    }

    /// Whether this response produces a rainbow (a nonzero spread).
    pub fn is_active(&self) -> bool {
        matches!(self.spread, Some(s) if s != 0.0)
    }

    /// Writes the spectral knobs into a neutral `GlassBank`'s spectral
    /// slots. A monochrome response (spread `None`/0) writes nothing, so
    /// the packed bank — and the wire — stay byte-identical to the
    /// no-spectral case.
    pub fn apply_to(&self, v: &mut GlassBank) {
        if let Some(s) = self.spread {
            if s != 0.0 {
                v.spectral_spread = s;
                v.spectral_ior = self.ior.unwrap_or(1.5).max(1.0);
                v.spectral_reach = self.reach.unwrap_or(1.0).max(0.0);
            }
        }
    }

    /// Bit-stable identity for chunk-cache hashing and content hashes.
    /// Field order is part of the contract; append, never reorder.
    pub fn write_bits(&self, out: &mut impl FnMut(u32)) {
        for opt in [self.ior, self.spread, self.reach] {
            match opt {
                None => out(0),
                Some(v) => {
                    out(1);
                    out(v.to_bits());
                }
            }
        }
    }
}
