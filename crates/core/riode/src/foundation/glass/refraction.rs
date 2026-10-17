// Single responsibility: backward-compat alias. `Refraction` (the old
// theatre-mode halo all around the card) is removed; use `LightSpill`
// (soft pastel light below the bottom edge). This alias keeps old call
// sites compiling while they migrate.

pub use super::light_spill::LightSpill;

/// Removed theatre halo: alias to [`LightSpill`] so existing
/// `Refraction::new(..).reach(..).softness(..)` code keeps building.
///
/// Mapping: `reach` (old edge-σ multiples) → `length` (logical px,
/// scaled ×32 at the call boundary is unnecessary — prefer
/// `LightSpill::new(..).length(..)` directly).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Refraction {
    pub strength: Option<f32>,
    pub reach: Option<f32>,
    pub softness: Option<f32>,
}

impl Default for Refraction {
    fn default() -> Self {
        Self::NONE
    }
}

impl Refraction {
    pub const NONE: Self = Self {
        strength: None,
        reach: None,
        softness: None,
    };
    pub const fn new(strength: f32) -> Self {
        Self {
            strength: Some(strength.max(0.0)),
            reach: None,
            softness: None,
        }
    }
    pub const fn reach(mut self, v: f32) -> Self {
        self.reach = Some(v.max(0.0));
        self
    }
    pub const fn softness(mut self, v: f32) -> Self {
        self.softness = Some(v.clamp(1.0, 5.0));
        self
    }
    pub fn is_active(&self) -> bool {
        matches!(self.strength, Some(s) if s != 0.0)
    }
    pub fn resolve_rule(&self) -> [f32; 4] {
        let taps = match self.softness.unwrap_or(3.0) {
            t if t < 2.0 => 1.0,
            t if t < 4.0 => 3.0,
            _ => 5.0,
        };
        // Old reach was edge-σ multiples (~0.75 neutral); map to a modest
        // downward length so migrated code renders a spill, not a halo.
        // reach_px ≈ reach * 32 logical px.
        [self.reach.unwrap_or(0.75).max(0.0) * 32.0, taps, 0.6, 0.5]
    }
    pub fn apply_to(&self, v: &mut super::constants::GlassBank) {
        if let Some(s) = self.strength {
            if s != 0.0 {
                v.refraction_strength = s;
            }
        }
    }
    pub fn write_bits(&self, out: &mut impl FnMut(u32)) {
        for opt in [self.strength, self.reach, self.softness] {
            match opt {
                None => out(0),
                Some(v) => {
                    out(1);
                    out(v.to_bits());
                }
            }
        }
    }
    pub fn as_spill(&self) -> LightSpill {
        LightSpill {
            strength: self.strength,
            length: self.reach.map(|r| r * 32.0),
            softness: self.softness,
        }
    }
}
