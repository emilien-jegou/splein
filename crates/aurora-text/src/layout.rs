// In crates/aurora/src/text/layout.rs:
// Replace TextLayout implementation:

use aurora_foundation::IntrinsicSize;
use crate::glyph::ShapedLine;
use std::any::Any;
use std::ops::Deref;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<ShapedLine>,
    pub total_size: IntrinsicSize,
}

impl TextLayout {
    pub fn new(lines: Vec<ShapedLine>, total_size: IntrinsicSize) -> Self {
        Self { lines, total_size }
    }

    /// Recovers the concrete layout behind a display list view handle.
    ///
    /// Only `aurora-text` constructs `DrawText`, so a mismatch is an internal
    /// invariant violation. The `Arc` round-trips the SAME allocation, which is
    /// what keeps the backends' `Arc::as_ptr` cache keys stable.
    pub fn view_arc(
        view: &Arc<dyn aurora_scene::TextLayoutView>,
    ) -> Arc<TextLayout> {
        // Turbofish first: inference would otherwise unify Arc::clone's T with the
        // annotated result. Then upcast to Any for the downcast below.
        let owned = Arc::<dyn aurora_scene::TextLayoutView>::clone(view);
        let any: Arc<dyn Any + Send + Sync> = owned;
        any.downcast::<TextLayout>()
            .ok()
            .expect("DrawText layout is always aurora_text::TextLayout")
    }

    /// Borrows the concrete layout behind a display list view handle.
    pub fn view_ref(view: &Arc<dyn aurora_scene::TextLayoutView>) -> &TextLayout {
        (view.as_ref() as &dyn Any)
            .downcast_ref::<TextLayout>()
            .expect("DrawText layout is always aurora_text::TextLayout")
    }
}

impl aurora_scene::TextLayoutView for TextLayout {}

impl Deref for TextLayout {
    type Target = [ShapedLine];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.lines
    }
}
