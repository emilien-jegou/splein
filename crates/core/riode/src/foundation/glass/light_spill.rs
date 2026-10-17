// Single responsibility: the soft pastel light a card spills onto the
// background beneath its bottom edge. Takes the card's own colour (the
// blurred finished card replayed underneath), extends downward only with
// decreasing opacity, dimmer than the card itself — a subtle ambient glow.

/// Light below the card: blurred card-coloured spill, downward only.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LightSpill {
    /// Spill gain. 0/`None` = no spill. Keep < 1 so the spill stays
    /// dimmer than the card.
    pub strength: Option<f32>,
    /// Downward extent in logical px. `None` ⇒ 48.
    pub length: Option<f32>,
    /// Blur taps across the spill (1, 3 or 5). `None` ⇒ 3.
    pub softness: Option<f32>,
}

impl Default for LightSpill {
    fn default() -> Self {
        Self::NONE
    }
}

impl LightSpill {
    /// Off: no spill.
    pub const NONE: Self = Self {
        strength: None,
        length: None,
        softness: None,
    };

    /// Spill starting point: gain only, length and softness neutral.
    pub const fn new(strength: f32) -> Self {
        Self {
            strength: Some(strength.max(0.0)),
            length: None,
            softness: None,
        }
    }

    /// Downward extent in logical px (neutral 48).
    pub const fn length(mut self, v: f32) -> Self {
        self.length = Some(v.max(0.0));
        self
    }

    /// Blur taps: 1 (tight), 3 (blended), 5 (diffuse).
    pub const fn softness(mut self, v: f32) -> Self {
        self.softness = Some(v.clamp(1.0, 5.0));
        self
    }

    /// Whether this spills any light (nonzero gain + nonzero length).
    pub fn is_active(&self) -> bool {
        matches!(self.strength, Some(s) if s != 0.0)
            && self.length.unwrap_or(48.0) > 0.0
    }

    /// Resolved shaping as the packer consumes it:
    /// (length_logical_px, tap_count, boost, alpha_cap).
    pub fn resolve_rule(&self) -> [f32; 4] {
        let taps = match self.softness.unwrap_or(3.0) {
            t if t < 2.0 => 1.0,
            t if t < 4.0 => 3.0,
            _ => 5.0,
        };
        [self.length.unwrap_or(48.0).max(0.0), taps, 0.6, 0.5]
    }

    /// Writes the spill gain into a neutral `GlassBank`.
    pub fn apply_to(&self, v: &mut super::constants::GlassBank) {
        if let Some(s) = self.strength {
            if s != 0.0 {
                v.refraction_strength = s;
            }
        }
    }

    /// Bit-stable identity. Field order is part of the contract; append,
    /// never reorder.
    pub fn write_bits(&self, out: &mut impl FnMut(u32)) {
        for opt in [self.strength, self.length, self.softness] {
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
