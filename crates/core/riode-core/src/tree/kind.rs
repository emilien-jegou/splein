// Single responsibility: Node kind taxonomy and the polymorphic leaf contract.

use std::any::Any;
use std::rc::Rc;

use crate::foundation::{Constraints, Direction, IntrinsicSize, ResolvedRect};
use crate::scene::context::PaintContext;

use crate::text::selection::TextSelection;
use crate::text::TextContext;
use crate::tree::binding::NodeBindings;
use crate::tree::flags::DirtyFlags;
use crate::tree::state::NodeState;
use crate::tree::style::NodeStyle;


/// Inputs to the measure phase: the offered constraints, which dimension the
/// intrinsic is probing, and the shared text shaping context.
pub struct MeasureCtx<'a> {
    pub constraints: Constraints,
    pub for_height: bool,
    pub text: &'a TextContext,
}

/// What a same-type re-reconcile absorbed into a retained leaf.
///
/// `dirty` are the flags the engine must mark on the node; `clear_selection`
/// retires any interactive selection the old content carried.
#[derive(Clone, Copy, Default)]
pub struct LeafChange {
    pub dirty: DirtyFlags,
    pub clear_selection: bool,
}

/// The open leaf contract. One method per engine phase a leaf can participate
/// in — measure, layout refit, paint, reactive update, input selection,
/// reconcile. Defaults make every capability opt-in, so a minimal third-party
/// primitive implements `measure` and `paint` and nothing else.
pub trait Primitive {
    /// Measure phase: intrinsic size under the offered constraints.
    ///
    /// The leaf receives the node's declared style (a leaf's wrap policy keys
    /// off its width intent) and its per-node state (a text leaf caches its
    /// shaped layout there). Implementations must stay consistent for a given
    /// `(style, ctx)` pair: results are memoized by the caller.
    fn measure(
        &self,
        style: &NodeStyle,
        state: &mut NodeState,
        ctx: &MeasureCtx,
    ) -> IntrinsicSize;

    /// Measure phase: minimum content size along one axis, mirroring CSS
    /// flexbox's automatic minimum size. Defaults to the unconstrained measure.
    fn min_content(&self, style: &NodeStyle, axis: Direction, text: &TextContext) -> f32 {
        let size = self.measure(
            style,
            &mut NodeState::default(),
            &MeasureCtx { constraints: Constraints::UNCONSTRAINED, for_height: false, text },
        );
        if axis == Direction::Horizontal { size.width } else { size.height }
    }

    /// Layout phase: re-shape at the exact inline size the parent assigned.
    /// Only leaves whose content re-flows to width (text) do anything; the
    /// default is a no-op.
    fn refit_to_width(
        &self,
        style: &NodeStyle,
        state: &mut NodeState,
        width: f32,
        text: &TextContext,
    ) {
        let _ = (style, state, width, text);
    }

    /// Paint phase: emit display list commands for this leaf's content box.
    ///
    /// `bounds` is the local content rect (`(0,0,width,height)` of the resolved
    /// box). State gives access to the leaf's sidecar (selection, settle
    /// placement, cached layout); ctx carries the chunk and frame inputs.
    fn paint(
        &self,
        style: &NodeStyle,
        state: &mut NodeState,
        ctx: &mut PaintContext,
        bounds: ResolvedRect,
    );

    /// Update phase: absorb reactive bindings into the retained leaf.
    /// Returns whether anything changed (the caller clears selection and
    /// inserts paint dirty on `true`).
    fn update_bindings(&self, bindings: &NodeBindings) -> bool {
        let _ = bindings;
        false
    }

    /// Input phase: whether this leaf hosts interactive text selection.
    fn selectable(&self) -> bool {
        false
    }

    /// Input phase: the substring this leaf presents for a selection range.
    fn content(&self) -> Option<String> {
        None
    }

    /// Input phase: the selected substring for a normalized range, if this
    /// leaf carries selectable text.
    fn selection_text(&self, selection: TextSelection) -> Option<String> {
        let _ = selection;
        None
    }

    /// Reconcile phase: absorb a same-type replacement, returning the change
    /// it represents. `None` means "not the same type — replace the instance".
    /// Same-type detection is `as_any`-based and lives here, never in the engine.
    fn reconcile(&self, next: &dyn Primitive) -> Option<LeafChange> {
        let _ = next;
        None
    }

    /// Erased self view for same-type reconciliation.
    fn as_any(&self) -> &dyn Any;
}

#[derive(Default)]
pub enum NodeKind {
    #[default]
    Group,
    Leaf(Rc<dyn Primitive>),
}

impl NodeKind {
    #[inline(always)]
    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group)
    }

    #[inline(always)]
    pub fn is_leaf(&self) -> bool {
        matches!(self, Self::Leaf(_))
    }

    #[inline(always)]
    pub fn primitive(&self) -> Option<&dyn Primitive> {
        match self {
            Self::Leaf(p) => Some(&**p),
            Self::Group => None,
        }
    }

    #[inline(always)]
    pub fn primitive_rc(&self) -> Option<Rc<dyn Primitive>> {
        match self {
            Self::Leaf(p) => Some(Rc::clone(p)),
            Self::Group => None,
        }
    }

    pub fn is_selectable(&self) -> bool {
        self.primitive().is_some_and(|p| p.selectable())
    }

    pub fn text_content(&self) -> Option<String> {
        self.primitive().and_then(|p| p.content())
    }

    pub fn selection_text(&self, selection: TextSelection) -> Option<String> {
        self.primitive().and_then(|p| p.selection_text(selection))
    }
}
