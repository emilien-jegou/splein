// In crates/aurora/src/text/layout.rs:
// Replace TextLayout implementation:

use crate::foundation::IntrinsicSize;
use crate::text::glyph::ShapedLine;
use std::ops::Deref;

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<ShapedLine>,
    pub total_size: IntrinsicSize,
}

impl TextLayout {
    pub fn new(lines: Vec<ShapedLine>, total_size: IntrinsicSize) -> Self {
        Self { lines, total_size }
    }
}

impl Deref for TextLayout {
    type Target = [ShapedLine];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.lines
    }
}
