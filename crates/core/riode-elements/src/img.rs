// Single responsibility: Tag-namespace constructor for the `view!` macro's `<img>` lowering.

use crate::image::{build as image_build, ImageElement};

/// `<img>` lowers to `img::build()`; `src=` fills it through `.src(...)`.
/// The module name resolves in the type namespace, so the path survives any
/// same-name value in scope.
pub fn build() -> ImageElement {
    image_build()
}
