// Single responsibility: Direct scroll axis flags and GroupElement wire-up.

use crate::dsl::group::GroupElement;
use crate::foundation::Direction;
use super::DEFAULT_OVERSCROLL;

/// Scroll channel enabled directly on `<g>`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
    Both,
}

/// Finalizes scroll viewport settings onto the group builder at Element build time.
pub fn finalize_scroll(mut def: GroupElement) -> GroupElement {
    let Some(axis) = def.scroll else {
        return def;
    };

    def.clip = true;
    def.scrollable = true;
    if def.overscroll == 0.0 {
        def.overscroll = DEFAULT_OVERSCROLL;
    }

    match axis {
        ScrollAxis::Vertical => {
            def.layout.direction = Direction::Vertical;
        }
        ScrollAxis::Horizontal => {
            def.layout.direction = Direction::Horizontal;
        }
        ScrollAxis::Both => {}
    }

    def
}
