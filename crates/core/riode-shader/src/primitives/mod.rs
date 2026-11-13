// Single responsibility: Flat registry and re-exports of visual primitives.

pub mod blend;
pub mod blur;
pub mod fill;
pub mod glass;
pub mod shadow;
pub mod shapes;
pub mod stroke;

pub use blend::*;
pub use blur::*;
pub use fill::*;
pub use glass::*;
pub use shadow::*;
pub use shapes::*;
pub use stroke::*;
