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

impl VectorGraphic {
    /// Creates an empty/stub graphic (used when parsing fails or svg-runtime is disabled).
    pub fn stub(width: f32, height: f32) -> Self {
        #[cfg(feature = "svg-runtime")]
        {
            let opt = resvg::usvg::Options::default();
            let tree = resvg::usvg::Tree::from_str("<svg xmlns='http://www.w3.org/2000/svg'/>", &opt)
                .expect("valid empty svg");
            Self {
                tree: Arc::new(tree),
                width,
                height,
            }
        }
        #[cfg(not(feature = "svg-runtime"))]
        {
            Self {
                tree_stub: (),
                width,
                height,
            }
        }
    }
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

#[cfg(not(feature = "svg-runtime"))]
impl VectorGraphic {
    pub fn from_str(_xml: &str) -> Result<Self, String> {
        Err("svg-runtime feature is disabled".to_string())
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
