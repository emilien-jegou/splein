// Single responsibility: how a source colour pass composites with its backdrop.

/// How a colour pass composites with the backdrop beneath it — the PDF 1.4
/// / CSS Compositing and Blending Level 1 blend-mode set, in the spec's
/// wire order (`Normal` = 0 … `Luminosity` = 15).
///
/// The wire id (the discriminant) is packed verbatim into float slots and
/// switches on in the shaders — never renumber an existing variant; append
/// at the end only.
///
/// Separable modes blend each channel pair independently; the four
/// non-separable modes (`Hue`, `Saturation`, `Color`, `Luminosity`) blend
/// in a perceptual colour space via luminance and saturation helpers.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum BlendMode {
    /// Source-over: the source replaces the backdrop by coverage (CSS
    /// default; the only mode wgpu's fixed-function blend can express).
    #[default]
    Normal,
    /// Multiplicative darkening — deepens the backdrop toward black.
    Multiply,
    /// Additive lightening through inverted multiply — lifts the backdrop
    /// toward white.
    Screen,
    /// Multiply where the backdrop is dark, screen where it is light —
    /// the classic print overprint. `Overlay(base, src) = HardLight(src, base)`.
    Overlay,
    /// Keeps the darker of the two per channel.
    Darken,
    /// Keeps the lighter of the two per channel.
    Lighten,
    /// Divides the backdrop by the inverted source — burns through.
    ColorDodge,
    /// Inverted divide — deepens, the opposite of dodge.
    ColorBurn,
    /// Multiply where the source is dark, screen where it is light.
    /// `HardLight(base, src) = Overlay(src, base)`.
    HardLight,
    /// Softened overlay: screened over light backdrops, multiplied over
    /// dark ones, continuous through the midpoint.
    SoftLight,
    /// Absolute per-channel difference — inverts where the two are equal.
    Difference,
    /// Softer difference: `base + src − 2·base·src`.
    Exclusion,
    /// Source hue over the backdrop's saturation and luminance.
    Hue,
    /// Source saturation over the backdrop's hue and luminance.
    Saturation,
    /// Source hue and saturation over the backdrop's luminance —
    /// colourise without changing lightness.
    Color,
    /// Source luminance over the backdrop's hue and saturation —
    /// recolour the backdrop with the source's lightness.
    Luminosity,
}

impl BlendMode {
    /// The spec's channel-count helper: separable modes are the first 12
    /// (0..=11); the last four walk through `Lum`/`SetLum` instead.
    pub const fn is_separable(self) -> bool {
        (self as u8) <= 11
    }

    /// `Normal` — the mode that needs no shader work (source-over).
    pub const fn is_normal(self) -> bool {
        (self as u8) == 0
    }

    /// The wire id packed into float uniform slots; the shaders switch on it.
    pub const fn as_uniform_id(self) -> f32 {
        self as u8 as f32
    }

    /// Inverse of [`Self::as_uniform_id`]; out-of-range values fold to `Normal`.
    pub const fn from_uniform_id(id: f32) -> Self {
        match id as i32 {
            1 => Self::Multiply,
            2 => Self::Screen,
            3 => Self::Overlay,
            4 => Self::Darken,
            5 => Self::Lighten,
            6 => Self::ColorDodge,
            7 => Self::ColorBurn,
            8 => Self::HardLight,
            9 => Self::SoftLight,
            10 => Self::Difference,
            11 => Self::Exclusion,
            12 => Self::Hue,
            13 => Self::Saturation,
            14 => Self::Color,
            15 => Self::Luminosity,
            _ => Self::Normal,
        }
    }
}

/// DSL literal parser for `blend="…"` — the CSS keyword set (dashes and
/// case insensitive). `None` on unknown keywords; the literal plumbing
/// logs and falls back to `Normal`.
pub fn parse_blend(s: &str) -> Option<BlendMode> {
    match s.trim().to_lowercase().replace('-', "").as_str() {
        "normal" => Some(BlendMode::Normal),
        "multiply" => Some(BlendMode::Multiply),
        "screen" => Some(BlendMode::Screen),
        "overlay" => Some(BlendMode::Overlay),
        "darken" => Some(BlendMode::Darken),
        "lighten" => Some(BlendMode::Lighten),
        "colordodge" => Some(BlendMode::ColorDodge),
        "colorburn" => Some(BlendMode::ColorBurn),
        "hardlight" => Some(BlendMode::HardLight),
        "softlight" => Some(BlendMode::SoftLight),
        "difference" => Some(BlendMode::Difference),
        "exclusion" => Some(BlendMode::Exclusion),
        "hue" => Some(BlendMode::Hue),
        "saturation" => Some(BlendMode::Saturation),
        "color" => Some(BlendMode::Color),
        "luminosity" => Some(BlendMode::Luminosity),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_css_keywords() {
        for (kw, want) in [
            ("normal", BlendMode::Normal),
            ("multiply", BlendMode::Multiply),
            ("screen", BlendMode::Screen),
            ("overlay", BlendMode::Overlay),
            ("darken", BlendMode::Darken),
            ("lighten", BlendMode::Lighten),
            ("color-dodge", BlendMode::ColorDodge),
            ("color-burn", BlendMode::ColorBurn),
            ("hard-light", BlendMode::HardLight),
            ("soft-light", BlendMode::SoftLight),
            ("difference", BlendMode::Difference),
            ("exclusion", BlendMode::Exclusion),
            ("hue", BlendMode::Hue),
            ("saturation", BlendMode::Saturation),
            ("color", BlendMode::Color),
            ("luminosity", BlendMode::Luminosity),
        ] {
            assert_eq!(parse_blend(kw), Some(want), "keyword {kw}");
            // Case-insensitive; dashed and undashed both accepted.
            assert_eq!(parse_blend(&kw.to_uppercase()), Some(want), "upper {kw}");
            assert_eq!(
                parse_blend(&kw.replace('-', "")),
                Some(want),
                "undashed {kw}"
            );
        }
        assert_eq!(parse_blend("bogus"), None);
        assert_eq!(parse_blend(""), None);
        assert_eq!(parse_blend("normalish"), None);
    }

    #[test]
    fn wire_ids_roundtrip_and_are_append_only() {
        // The discriminant is the wire id — 0..=15 in spec order.
        for (i, want) in [
            (0.0, BlendMode::Normal),
            (1.0, BlendMode::Multiply),
            (2.0, BlendMode::Screen),
            (3.0, BlendMode::Overlay),
            (4.0, BlendMode::Darken),
            (5.0, BlendMode::Lighten),
            (6.0, BlendMode::ColorDodge),
            (7.0, BlendMode::ColorBurn),
            (8.0, BlendMode::HardLight),
            (9.0, BlendMode::SoftLight),
            (10.0, BlendMode::Difference),
            (11.0, BlendMode::Exclusion),
            (12.0, BlendMode::Hue),
            (13.0, BlendMode::Saturation),
            (14.0, BlendMode::Color),
            (15.0, BlendMode::Luminosity),
        ] {
            assert_eq!(want.as_uniform_id(), i, "id for {want:?}");
            assert_eq!(BlendMode::from_uniform_id(i), want);
        }
        // Out-of-range folds to Normal — shaders may feed garbage.
        assert_eq!(BlendMode::from_uniform_id(16.0), BlendMode::Normal);
        assert_eq!(BlendMode::from_uniform_id(-1.0), BlendMode::Normal);
        assert_eq!(BlendMode::from_uniform_id(f32::NAN), BlendMode::Normal);
    }

    #[test]
    fn separable_partition() {
        // 0..=11 separable, 12..=15 non-separable (Lum/SetLum family).
        for m in [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::HardLight,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Exclusion,
        ] {
            assert!(m.is_separable(), "{m:?}");
        }
        for m in [
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
        ] {
            assert!(!m.is_separable(), "{m:?}");
        }
        assert!(BlendMode::Normal.is_normal());
        assert!(!BlendMode::Multiply.is_normal());
    }
}
