// Single responsibility: Directional flex container constructors for rows and columns.

use crate::dsl::group::{group, GroupDef};
use crate::foundation::Direction;

/// Constructs a horizontal flex row container.
pub fn row() -> GroupDef {
    group().direction(Direction::Horizontal)
}

/// Constructs a vertical flex column container.
pub fn column() -> GroupDef {
    group().direction(Direction::Vertical)
}
