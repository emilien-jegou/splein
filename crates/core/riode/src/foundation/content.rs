// Responsibility: How a node's interior reacts while its render box changes size.
// Responsibility: Media content fitting and scaling modes (object-fit / BoxFit alternative).

/// Who owns a node's render box while it is away from its flow box.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum RenderDriver {
    /// The render box springs toward the flow box.
    #[default]
    Auto,
    /// A pointer or gesture writes the render box directly, ignoring the flow box.
    ///
    /// This is what makes drag simple: the held node stops following layout, siblings
    /// keep springing to their own flow boxes, and release hands the pointer's velocity
    /// to the spring. No dragged flag, no stride compensation.
    Pointer,
}


#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    /// Scale proportionally to fill container; anchors top-left and crops bottom or right on overflow (Default).
    #[default]
    Cover,
    /// Scale proportionally to fill container; centered crop.
    CenterCrop,
    /// Scale proportionally to fit entirely inside container without cropping.
    Contain,
    /// Stretch to fill container ignoring aspect ratio.
    Fill,
    /// Native 1:1 pixel scale, cropped by container.
    None,
}

impl From<&str> for ImageFit {
    fn from(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "cover" | "crop" => ImageFit::Cover,
            "center-crop" | "center" => ImageFit::CenterCrop,
            "contain" => ImageFit::Contain,
            "fill" | "stretch" => ImageFit::Fill,
            "none" | "native" => ImageFit::None,
            _ => ImageFit::Cover,
        }
    }
}

impl From<String> for ImageFit {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
