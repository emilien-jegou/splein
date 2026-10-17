// Single responsibility: the emissive gradient of a glass slab — the
// orange→pink→purple→blue wash of the liquid-glass recorder reference
// (`liquid-glass-faithful/glass.frag: glowField`,
// `uLobes/uLobeCols/uGlowY/uGlow/uGlowInside`).
//
// This is deliberately NOT part of `Glass` or `Spectral`: dispersion
// splits backdrop light already passing through the lens (a grey field
// stays grey), while the glow is *emissive* light the slab carries
// itself — visible on a plain white field with no backdrop at all.
// Same separation `Shadow`/`Backdrop`/`Spectral` established: the lens
// stays monochromatic, the glow optionally adds light inside it. What
// the card throws *outside* its edge is not the glow's business at all —
// that is `Refraction` (the theatre-mode halo), a separate layer-owned
// concept that works with or without a glow.
//
// v2 is a real gradient definition, not gains on a hardcoded wash: the
// stop list, anchor edge, span and thickness live here in userland, the
// CPU derives the 6 Gaussian lobes from them at pack time, and the
// shader only evaluates the uniform lobes. Static (no mic/voice bands).

use super::super::{BlendMode, Color};

/// How many lobes the gradient resolves to. Fixed: the wire carries
/// exactly this many, so the shader loop is constant-bounded and the
/// layout never shifts with the stop count.
pub const GLOW_LOBES: usize = 6;

/// The reference palette (`liquid-glass-faithful/demo.html`
/// `DEFAULTS.glowColors`): amber, orange-red, pink, purple, blue, cyan.
/// Used when no explicit `.stops()` is given, so `Glow::new()` keeps
/// working with zero configuration.
pub const GLOW_DEFAULT_STOPS: [Color; GLOW_LOBES] = [
    Color::hex(0xFFB340),
    Color::hex(0xFF6A3D),
    Color::hex(0xFF3D84),
    Color::hex(0xB04DFF),
    Color::hex(0x4F7DFF),
    Color::hex(0x36D3FF),
];

/// Which edge the gradient anchors to. The lobe line sits just outside
/// the edge (the reference `uGlowY = −1.02·radius`): half the wash inside
/// the slab, half bleeding out — which is why the reference colour crosses
/// the bottom rim instead of stopping at it.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GlowAnchor {
    Bottom,
    Top,
}

impl Default for GlowAnchor {
    fn default() -> Self {
        Self::Bottom
    }
}

/// The emissive gradient of a glass slab: a stop list resolved to lobes
/// along an anchor edge, plus overall/inside gains. Interior only — the
/// halo outside the edge is `Refraction`, not a glow knob.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Glow {
    /// Overall gain (`uGlow`). 0/`None` = off.
    pub strength: Option<f32>,
    /// Gain of the glow inside the slab (`uGlowInside`).
    /// `None` ⇒ 1.5 (the reference default).
    pub inside: Option<f32>,
    /// Explicit stops as (position 0..1 across the span, colour).
    /// `None` = the reference palette, evenly spaced.
    pub stops: Option<[(f32, Color); GLOW_LOBES]>,
    /// Which edge the lobe line sits on. `None` ⇒ bottom.
    pub anchor: Option<GlowAnchor>,
    /// Lobe spread as a fraction of the shape extent along the edge.
    /// `None` ⇒ 0.35 (the reference `s·1.4` coverage, `s=max(L·0.5,20)`).
    pub span: Option<f32>,
    /// Cross-edge sigma as a fraction of the shape extent. `None` ⇒ 0.17
    /// (the reference `r·0.34` against the full cross extent).
    pub thickness: Option<f32>,
    /// Sample the wash at the lens-displaced point instead of the
    /// geometric pixel (`None`/`false` ⇒ geometric, the reference
    /// behaviour). Opt-in deviation: the glow bends with the refraction
    /// like light inside the glass, instead of lying pasted over it.
    pub refracted: Option<bool>,
    /// How the emissive wash composites with the glass base
    /// (`u_glow_geom.w`): `Normal` (default) is plain source-over;
    /// `Overlay`/`SoftLight`/… tone the slab like ink on glass. See
    /// [`BlendMode`]. Appended — wire offsets above never move.
    pub blend: BlendMode,
}

impl Default for Glow {
    fn default() -> Self {
        Self::NONE
    }
}

impl Glow {
    /// Off: no emissive light, the shader takes the no-glow path.
    pub const NONE: Self = Self {
        strength: None,
        inside: None,
        stops: None,
        anchor: None,
        span: None,
        thickness: None,
        refracted: None,
        blend: BlendMode::Normal,
    };

    /// The neutral starting point. `Glow::new().strength(1.0)` turns on
    /// the reference wash and leaves everything else at reference defaults.
    pub const fn new() -> Self {
        Self::NONE
    }

    /// Overall gain (0 = off).
    pub const fn strength(mut self, v: f32) -> Self {
        self.strength = Some(v.max(0.0));
        self
    }

    /// Gain inside the slab (`None` ⇒ 1.5).
    pub const fn inside(mut self, v: f32) -> Self {
        self.inside = Some(v.max(0.0));
        self
    }

    /// Evenly spaced stops across the span.
    pub const fn stops(mut self, colors: [Color; GLOW_LOBES]) -> Self {
        let mut s = [(0.0f32, colors[0]); GLOW_LOBES];
        let mut i = 0;
        while i < GLOW_LOBES {
            // (i + 0.5) / 6 − 0.5: the reference lobe offsets (nj).
            s[i].0 = (i as f32 + 0.5) / (GLOW_LOBES as f32) - 0.5;
            s[i].1 = colors[i];
            i += 1;
        }
        self.stops = Some(s);
        self
    }

    /// Stops at explicit positions (0..1 across the span).
    pub const fn stops_at(mut self, stops: [(f32, Color); GLOW_LOBES]) -> Self {
        self.stops = Some(stops);
        self
    }

    /// Which edge the lobe line sits on (`None` ⇒ bottom).
    pub const fn anchor(mut self, anchor: GlowAnchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Lobe spread as a fraction of the shape extent (`None` ⇒ 0.75).
    pub const fn span(mut self, v: f32) -> Self {
        self.span = Some(v.max(0.0));
        self
    }

    /// Cross-edge sigma as a fraction of the shape extent (`None` ⇒ 0.20).
    pub const fn thickness(mut self, v: f32) -> Self {
        self.thickness = Some(v.max(0.0));
        self
    }

    /// Evaluate the wash at the lens-displaced sample point instead of
    /// the geometric pixel (deviation from the reference: the glow bends
    /// with the refraction). `false` (default) keeps reference behaviour.
    pub const fn refracted(mut self, v: bool) -> Self {
        self.refracted = Some(v);
        self
    }

    /// How the wash composites with the glass base (`u_glow_geom.w`):
    /// `Normal` (default) is plain source-over; `Overlay`/`SoftLight`/…
    /// tone the slab like ink on glass. See [`BlendMode`].
    pub const fn blend(mut self, v: BlendMode) -> Self {
        self.blend = v;
        self
    }

    /// Whether this glow emits any light (nonzero strength).
    pub fn is_active(&self) -> bool {
        matches!(self.strength, Some(s) if s != 0.0)
    }

    /// Writes the glow gains into a neutral `GlassBank`. An inactive
    /// glow writes nothing, so the packed bank — and the v4 wire prefix —
    /// stay byte-identical to the no-glow case.
    pub fn apply_to(&self, v: &mut super::constants::GlassBank) {
        if let Some(s) = self.strength {
            if s != 0.0 {
                v.glow_voice = s;
                v.glow_inside = self.inside.unwrap_or(1.5).max(0.0);
            }
        }
    }

    /// Resolves the stop list to shader lobes against a shape rect in
    /// device px: per-lobe (centre, sigma, amplitude) along the x axis,
    /// srgb colours, the cross-edge line + sigma, and the axis flag
    /// (always 0: lobes run along x for bottom/top). Sigma is ~1.6× the
    /// lobe spacing so neighbours overlap into one continuous ribbon
    /// (the reference coverage), and amplitudes carry the reference
    /// `exp(−nj²/0.32)` edge falloff over a 0.45 floor.
    pub fn resolve(&self, rect: (f32, f32, f32, f32)) -> ResolvedGlow {
        let (x, y, w, h) = rect;
        let anchor = self.anchor.unwrap_or(GlowAnchor::Bottom);
        let extent = w.max(1.0);
        let cross = h.max(1.0);
        let span = self.span.unwrap_or(0.35).max(0.0) * extent;
        let spacing = span / (GLOW_LOBES as f32);
        let sigma = (spacing * 1.6).max(1.0);
        let line_sigma = (self.thickness.unwrap_or(0.17).max(0.0) * cross).max(1.0);
        let origin = x + w * 0.5;
        // Reference line sits ~2% of a radius outside the slab so the
        // wash straddles the rim instead of stopping at it.
        let outset = 0.01 * cross;
        let line = match anchor {
            GlowAnchor::Bottom => y + h + outset,
            GlowAnchor::Top => y - outset,
        };
        let stops = self.stops.unwrap_or_else(|| {
            let mut s = [(0.0f32, GLOW_DEFAULT_STOPS[0]); GLOW_LOBES];
            let mut i = 0;
            while i < GLOW_LOBES {
                s[i].0 = (i as f32 + 0.5) / (GLOW_LOBES as f32) - 0.5;
                s[i].1 = GLOW_DEFAULT_STOPS[i];
                i += 1;
            }
            s
        });
        let mut lobes = [(0.0f32, 1.0f32, 0.0f32); GLOW_LOBES];
        let mut colors = [[0.0f32; 3]; GLOW_LOBES];
        let mut i = 0;
        while i < GLOW_LOBES {
            let (pos, color) = stops[i];
            lobes[i] = (
                origin + pos * span,
                sigma,
                // The reference edge falloff (no floor: edge lobes fade out).
                0.55 * (-(pos * pos) / 0.32).exp(),
            );
            colors[i] = [color.r, color.g, color.b];
            i += 1;
        }
        ResolvedGlow {
            lobes,
            colors,
            line,
            line_sigma,
            // Geometry slot doubling as the through-glass flag: 0 keeps
            // reference behaviour (geometric sampling), 1 samples the wash
            // at the lens-displaced point (opt-in deviation).
            axis: if self.refracted.unwrap_or(false) { 1.0 } else { 0.0 },
            blend: self.blend,
        }
    }

    /// Bit-stable identity for chunk-cache hashing and content hashes.
    /// Field order is part of the contract; append, never reorder.
    pub fn write_bits(&self, out: &mut impl FnMut(u32)) {
        for opt in [self.strength, self.inside, self.span, self.thickness] {
            match opt {
                None => out(0),
                Some(v) => {
                    out(1);
                    out(v.to_bits());
                }
            }
        }
        match self.stops {
            None => out(0),
            Some(stops) => {
                out(1);
                for (pos, color) in stops {
                    out(pos.to_bits());
                    out(color.r.to_bits());
                    out(color.g.to_bits());
                    out(color.b.to_bits());
                }
            }
        }
        match self.anchor {
            None => out(0),
            Some(a) => {
                out(1);
                out(match a {
                    GlowAnchor::Bottom => 0,
                    GlowAnchor::Top => 1,
                })
            }
        }
        match self.refracted {
            None => out(0),
            Some(v) => {
                out(1);
                out(if v { 1 } else { 0 })
            }
        }
        // Appended blend mode (see struct doc: append-only, never reorder).
        // `Normal` = 0 keeps every pre-blend hash preimage byte-identical.
        out(self.blend as u8 as u32);
    }
}

/// A gradient resolved against one shape rect, ready for the wire:
/// lobe (centre, sigma, amplitude) triples in device px along the x
/// axis, srgb lobe colours, the cross-edge line + sigma, the axis
/// flag (always 0: lobes run along x for bottom/top), and the blend
/// mode id (`BlendMode::as_uniform_id`, packed into the geom vec4's
/// free `w` slot).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ResolvedGlow {
    pub lobes: [(f32, f32, f32); GLOW_LOBES],
    pub colors: [[f32; 3]; GLOW_LOBES],
    pub line: f32,
    pub line_sigma: f32,
    pub axis: f32,
    pub blend: BlendMode,
}
