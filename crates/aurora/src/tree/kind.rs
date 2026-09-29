// Single responsibility: Node kind taxonomy and primitive trait contract.

use crate::foundation::{Constraints, IntrinsicSize, ResolvedRect};
use crate::scene::context::PaintContext;
pub use crate::text::TextConfig;

/// Trait implemented by user-defined custom layout primitives.
pub trait Primitive {
    /// Computes the natural intrinsic bounds under constraints.
    fn measure(&self, constraints: Constraints) -> IntrinsicSize;
    /// Records display list drawing commands into the paint context.
    fn paint(&self, ctx: &mut PaintContext, bounds: ResolvedRect);
}

/// Node taxonomy distinguishing structural containers, text runs, and primitives.
#[derive(Default)]
pub enum NodeKind {
    /// Flex group container with child layout specifications.
    #[default]
    Group,
    /// Styled typography node.
    Text(TextConfig),
    /// User-defined layout and drawing primitive.
    Custom(Box<dyn Primitive>),
}

