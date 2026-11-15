// Single responsibility: Text as a retained layout leaf — the `Primitive` implementation for text.
//
// The engine treats a text node as opaque sizing and painting behavior behind the `Primitive`
// contract: it never shapes, wraps, or inspects glyphs itself. Wrapping width derivation,
// shaping calls, min-content floors, and display-list emission all live here, next to the
// shaping cache they rely on.

use std::cell::RefCell;
use std::sync::Arc;

use crate::foundation::{Constraints, Direction, IntrinsicSize, LayerPaint, Point, ResolvedRect, Size};
use crate::scene::context::PaintContext;
use crate::scene::{LayerId, SceneCommand};
use crate::text::config::TextConfig;
use crate::text::layout::TextLayout;
use crate::text::selection::{selection_rects, TextSelection};
use crate::text::TextContext;
use crate::tree::binding::NodeBindings;
use crate::tree::flags::DirtyFlags;
use crate::tree::kind::{LeafChange, MeasureCtx, Primitive};
use crate::tree::state::NodeState;
use crate::tree::style::NodeStyle;

/// Shaping-adjacent helpers on the declarative config: pure functions of the
/// config plus a shaping context, shared by the leaf's measure/refit paths.
impl TextConfig {
    /// Shapes this run under constraints, returning its intrinsic size and layout.
    pub fn measure(&self, ctx: &TextContext, constraints: Constraints) -> (IntrinsicSize, Arc<TextLayout>) {
        let layout = ctx.shape_config(self, constraints);
        (layout.total_size, layout)
    }

    /// Whether a cached layout outgrows the inline size the node finally received.
    pub fn needs_refit(cached: Option<&TextLayout>, width: f32) -> bool {
        match cached {
            Some(layout) => layout.total_size.width > width + 0.01,
            None => true,
        }
    }

    /// Re-shapes this run at an exact inline width.
    pub fn refit(&self, ctx: &TextContext, width: f32) -> Arc<TextLayout> {
        ctx.shape_config(self, Constraints::tight_width(width))
    }

    /// Layout to paint with: the cached run, or a fallback shaped at the used width.
    pub fn resolve_paint_layout(
        &self,
        ctx: &TextContext,
        cached: Option<Arc<TextLayout>>,
        fit_width: bool,
        resolved_width: f32,
    ) -> Arc<TextLayout> {
        cached.unwrap_or_else(|| {
            let max_w = if fit_width {
                f32::INFINITY
            } else {
                resolved_width.max(1.0)
            };
            ctx.shape_config(self, Constraints::loose(max_w, f32::INFINITY))
        })
    }
}

/// The retained text leaf: a declarative [`TextConfig`] plus interior mutability
/// for the reactive content the scheduler pushes in between frames.
pub struct TextLeaf {
    config: RefCell<TextConfig>,
}

impl TextLeaf {
    pub fn new(config: TextConfig) -> Self {
        Self { config: RefCell::new(config) }
    }

    /// Inline width text wraps against, from the node's width intent and offered space.
    ///
    /// `Fit` means CSS `fit-content` on both probes:
    /// `max(min-content, min(max-content, available))`. A bounded offer folds
    /// the text instead of reporting max-content, so a fit container is capped
    /// by its parent rather than stretched by its longest line; an unbounded
    /// offer still shapes max-content.
    fn wrap_width(
        config: &TextConfig,
        style_width: Size,
        ctx: &TextContext,
        constraints: Constraints,
        for_height: bool,
    ) -> f32 {
        // When the width is already resolved (tight), reuse it directly instead of
        // re-applying the sizing intent, which would double-count Fill/Percent and
        // ignore a wrapped cross size.
        if constraints.is_tight_width() {
            return constraints.max_width.max(0.0);
        }
        // Refusing to wrap means unbounded by definition; the parent caps the
        // box and paint clips. Balance searches its own width inside shaping:
        // item sizing takes the full offer, height sizing takes the offer
        // directly so the search runs inside the final box.
        let available = constraints.max_width;
        if config.wrap.is_nowrap() {
            return f32::INFINITY;
        }
        if config.wrap.is_balance() {
            if for_height && available.is_finite() {
                return available.max(0.0);
            }
            return f32::INFINITY;
        }
        match style_width {
            Size::Fold => {
                // Fold never stretches a parent: intrinsic is min-content on
                // both probes, and the fold itself happens at the assigned
                // box (forced refit). Unbounded offers degrade to Fit only
                // downstream, where min(max-content, infinity) is max-content.
                ctx.min_inline_width(config).max(0.0)
            }
            Size::Fit => {
                if !available.is_finite() {
                    return f32::INFINITY;
                }
                let min = ctx.min_inline_width(config);
                let max = ctx.max_inline_width(config);
                min.max(0.0).max(max.min(available.max(0.0)))
            }
            Size::Fill => {
                if available.is_finite() {
                    available.max(0.0)
                } else {
                    f32::INFINITY
                }
            }
            Size::Fixed(value) => {
                if available.is_finite() {
                    value.min(available).max(0.0)
                } else {
                    value.max(0.0)
                }
            }
            Size::Percent(ratio) => {
                if available.is_finite() {
                    (available * ratio).max(0.0)
                } else {
                    f32::INFINITY
                }
            }
        }
    }
}

impl Primitive for TextLeaf {
    fn measure(&self, style: &NodeStyle, state: &mut NodeState, ctx: &MeasureCtx) -> IntrinsicSize {
        let config = self.config.borrow();
        let wrap_w = Self::wrap_width(
            &config,
            style.width,
            ctx.text,
            ctx.constraints,
            ctx.for_height,
        );
        let effective = Constraints {
            min_width: 0.0,
            max_width: wrap_w,
            min_height: 0.0,
            max_height: f32::INFINITY,
        };
        let layout = ctx.text.shape_config(&config, effective);
        let size = layout.total_size;
        state.text.cached_text_layout = Some(layout);
        size
    }

    fn min_content(&self, _style: &NodeStyle, axis: Direction, text: &TextContext) -> f32 {
        let config = self.config.borrow();
        match axis {
            Direction::Horizontal => text.min_inline_width(&config),
            // A run always needs one line box, never zero, so it cannot be crushed
            // out of existence.
            Direction::Vertical => config.line_height.resolve(config.size),
        }
    }

    fn refit_to_width(&self, style: &NodeStyle, state: &mut NodeState, width: f32, text: &TextContext) {
        let config = self.config.borrow();
        let force = style.width.is_fold();
        let cached = state.text.cached_text_layout.as_deref();
        if !force && !TextConfig::needs_refit(cached, width) {
            return;
        }
        let layout = config.refit(text, width);
        state.text.cached_text_layout = Some(layout);
    }

    fn paint(&self, style: &NodeStyle, state: &mut NodeState, ctx: &mut PaintContext, bounds: ResolvedRect) {
        let config = self.config.borrow();
        // Layout to paint with: the cached run, or a fallback shaped at the used width.
        let layout = config.resolve_paint_layout(
            ctx.text(),
            state.text.cached_text_layout.clone(),
            style.width.is_fit(),
            (bounds.width - style.padding.horizontal()).max(0.0),
        );

        let pad = style.padding;
        let run_y = ctx.abs_origin_y() + pad.top;
        let adj = state.text.placement.observe(run_y, crate::text::baseline_snap_y());
        ctx.mark_has_text();
        ctx.and_text_settled(state.text.placement.is_settled());
        if pad.left != 0.0 || pad.top != 0.0 {
            ctx.command(SceneCommand::PushOffset(Point::new(pad.left, pad.top)));
        }
        if adj != 0.0 {
            ctx.command(SceneCommand::PushOffset(Point::new(0.0, adj)));
        }
        Self::emit(
            &config,
            &layout,
            ctx,
            ctx.layer_id(),
            state.text.text_selection,
            state.input.text_focused && ctx.caret_visible(),
        );
        if adj != 0.0 {
            ctx.command(SceneCommand::PopOffset);
        }
        if pad.left != 0.0 || pad.top != 0.0 {
            ctx.command(SceneCommand::PopOffset);
        }
    }

    fn update_bindings(&self, bindings: &NodeBindings) -> bool {
        let mut config = self.config.borrow_mut();
        bindings.update_text(&mut config.content)
    }

    fn selectable(&self) -> bool {
        self.config.borrow().selectable
    }

    fn content(&self) -> Option<String> {
        Some(self.config.borrow().content.clone())
    }

    fn selection_text(&self, selection: TextSelection) -> Option<String> {
        let config = self.config.borrow();
        let (s, e) = selection.normalized();
        Some(config.content.get(s..e).unwrap_or("").to_string())
    }

    fn reconcile(&self, next: &dyn Primitive) -> Option<LeafChange> {
        let next = next.as_any().downcast_ref::<TextLeaf>()?;
        let old = self.config.borrow();
        let fresh = next.config.borrow();
        let changed = *old != *fresh;
        let clear_selection = old.content != fresh.content || !fresh.selectable;
        let dirty = if changed { DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT } else { DirtyFlags::NONE };
        Some(LeafChange { dirty, clear_selection })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl TextLeaf {
    /// Emits a shaped run: selection highlight bands, then glyphs.
    ///
    /// Core contributes geometry only — one `BeginLayer`/`EndLayer` pair per
    /// overlapped line, carrying whatever material stages the caller authored
    /// on `selection_fill` (concrete fill stages live in `riode-shader`). No
    /// fill stage exists in core; without a fill the bands are simply not
    /// emitted. Decorations and the caret remain purged.
    fn emit(
        config: &TextConfig,
        layout: &Arc<TextLayout>,
        chunk_ctx: &mut PaintContext,
        layer_id: LayerId,
        selection: Option<TextSelection>,
        _focused: bool,
    ) {
        if let (Some(sel), Some(fill)) = (selection, config.selection_fill.as_ref()) {
            // One band per line, behind the glyphs: emitted in stream order
            // before `DrawText`, each its own material layer so the engine's
            // standard fill-then-composite paints it.
            for (i, band) in selection_rects(layout, &config.content, &sel)
                .into_iter()
                .enumerate()
            {
                let id = LayerId::from_packed(layer_id.0.wrapping_sub(i as u64 + 1));
                chunk_ctx.command(SceneCommand::BeginLayer {
                    id,
                    rect: band,
                    paint: LayerPaint {
                        material: Some(fill.clone()),
                        ..LayerPaint::default()
                    },
                });
                chunk_ctx.command(SceneCommand::EndLayer { id });
            }
        }
        chunk_ctx.command(SceneCommand::DrawText {
            origin: Point::ZERO,
            layout: Arc::clone(layout),
            color: config.color,
        });
    }
}
