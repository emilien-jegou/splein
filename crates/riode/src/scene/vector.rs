// Single responsibility: Backend-agnostic vector graphic document handle.

use std::sync::Arc;

/// Parsed scalable vector document representation for scene rendering.
#[derive(Clone)]
pub struct VectorGraphic {
    #[cfg(feature = "svg-runtime")]
    pub tree: Arc<resvg::usvg::Tree>,
    #[cfg(not(feature = "svg-runtime"))]
    tree_stub: (),
    pub width: f32,
    pub height: f32,
}

#[cfg(feature = "svg-runtime")]
impl VectorGraphic {
    /// Parses an SVG XML string into a vector graphic handle.
    pub fn from_str(xml: &str) -> Result<Self, String> {
        let opt = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_str(xml, &opt).map_err(|e| e.to_string())?;
        let size = tree.size();
        Ok(Self {
            tree: Arc::new(tree),
            width: size.width(),
            height: size.height(),
        })
    }

    /// Wraps a pre-parsed USVG tree handle into a vector graphic.
    pub fn from_tree(tree: Arc<resvg::usvg::Tree>) -> Self {
        let size = tree.size();
        Self {
            tree,
            width: size.width(),
            height: size.height(),
        }
    }
}

/// Stub when `svg-runtime` is off: parsing is unavailable (use
/// pre-rasterized images); dimensions default to zero.
#[cfg(not(feature = "svg-runtime"))]
impl VectorGraphic {
    pub fn from_str(_xml: &str) -> Result<Self, String> {
        Err("svg-runtime feature is disabled".to_string())
    }

    pub fn stub(width: f32, height: f32) -> Self {
        Self {
            tree_stub: (),
            width,
            height,
        }
    }
}

impl From<Arc<VectorGraphic>> for VectorGraphic {
    fn from(v: Arc<VectorGraphic>) -> Self {
        (*v).clone()
    }
}

impl From<&Arc<VectorGraphic>> for VectorGraphic {
    fn from(v: &Arc<VectorGraphic>) -> Self {
        (**v).clone()
    }
}

impl std::fmt::Debug for VectorGraphic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VectorGraphic")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}
