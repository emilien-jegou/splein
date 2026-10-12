// Font identifiers and font data management for the Parley text pipeline.

use std::sync::Arc;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

/// Typographic style selecting normal, italic, or oblique faces.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    /// Upright face.
    #[default]
    Normal,
    /// Italic face.
    Italic,
    /// Slanted oblique face.
    Oblique,
}

pub struct FontData {
    pub id: FontId,
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
}

impl FontData {
    pub fn new(id: u32, name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            id: FontId(id),
            name: name.into(),
            bytes: Arc::new(bytes),
        }
    }
}

/// A single variable-font axis setting: 4-byte OpenType tag plus value.
///
/// Tags are raw 4-byte arrays (`*b"wght"`, `*b"opsz"`, `*b"wdth"`); use
/// [`FontVariation::named`] for the common `&str` form.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FontVariation {
    /// Raw 4-byte OpenType axis tag.
    pub tag: [u8; 4],
    /// Axis value in font design units.
    pub value: f32,
}

impl FontVariation {
    /// Builds a setting from a 4-character axis name (`"wght"`, `"opsz"`, ...).
    ///
    /// Returns `None` when the name is not exactly 4 ASCII characters.
    pub fn named(tag: &str, value: f32) -> Option<Self> {
        let bytes = tag.as_bytes();
        if bytes.len() == 4 && bytes.is_ascii() {
            Some(Self {
                tag: [bytes[0], bytes[1], bytes[2], bytes[3]],
                value,
            })
        } else {
            None
        }
    }

    /// Converts to the Parley setting consumed by the shaper.
    #[cfg(feature = "text-runtime")]
    pub(crate) fn to_parley(self) -> parley::FontVariation {
        parley::FontVariation::new(parley::setting::Tag::from_bytes(self.tag), self.value)
    }
}
