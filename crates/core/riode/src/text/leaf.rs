// Single responsibility: Text as a layout leaf — measure, min-content, and paint behavior.
//
// Layout treats a text node as opaque sizing behavior behind these methods: it never
// shapes, wraps, or inspects glyphs itself. Wrapping width derivation, shaping calls,
// min-content floors, and display-list emission all live here, next to the shaping
// cache they rely on.

use std::sync::Arc;

use crate::foundation::{Constraints, Direction, IntrinsicSize, LayerPaint, Point, Size};
use crate::scene::{LayerId, SceneChunk, SceneCommand};
use crate::text::config::TextConfig;
use crate::text::layout::TextLayout;
use crate::text::selection::{selection_rects, TextSelection};
use crate::text::TextContext;

impl TextConfig {
    /// Inline width text wraps against, from the node's width intent and offered space.
    ///
    /// Moved out of layout measurement so the wrapping policy is defined wherever
    /// shaping is defined. `Fit` means CSS `fit-content` on both probes:
    /// `max(min-content, min(max-content, available))`. A bounded offer folds
    /// the text instead of reporting max-content, so a fit container is capped
    /// by its parent rather than stretched by its longest line; an unbounded
    /// offer still shapes max-content. Both probes shape the unbounded layout
    /// only when nothing bounds them, which is shared through the shaping
    /// cache — one cached shape, not two cold ones.
    pub fn wrap_width(
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

    /// Shapes this run under constraints, returning its intrinsic size and layout.
    pub fn measure(
        &self,
        ctx: &TextContext,
        constraints: Constraints,
    ) -> (IntrinsicSize, Arc<TextLayout>) {
        let layout = ctx.shape_config(self, constraints);
        (layout.total_size, layout)
    }

    /// Min-content floor: longest word horizontally, one line box vertically.
    ///
    /// Mirrors CSS flexbox's automatic minimum size. Callers clamp this by the item's
    /// own desired size so the guard never inflates a box past what was asked for.
    pub fn min_content(&self, ctx: &TextContext, axis: Direction) -> f32 {
        match axis {
            Direction::Horizontal => ctx.min_inline_width(self),
            // A run always needs one line box, never zero, so it cannot be crushed
            // out of existence.
            Direction::Vertical => self.line_height.resolve(self.size),
        }
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

    /// Emits a shaped run: selection highlight bands, then glyphs.
    ///
    /// Core contributes geometry only — one `BeginLayer`/`EndLayer` pair per
    /// overlapped line, carrying whatever material stages the caller authored
    /// on `selection_fill` (concrete fill stages live in `riode-shader`). No
    /// fill stage exists in core; without a fill the bands are simply not
    /// emitted. Decorations and the caret remain purged.
    pub fn emit(
        &self,
        layout: &Arc<TextLayout>,
        chunk: &mut SceneChunk,
        layer_id: LayerId,
        selection: Option<TextSelection>,
        _focused: bool,
    ) {
        if let (Some(sel), Some(fill)) = (selection, self.selection_fill.as_ref()) {
            // One band per line, behind the glyphs: emitted in stream order
            // before `DrawText`, each its own material layer so the engine's
            // standard fill-then-composite paints it.
            for (i, band) in selection_rects(layout, &self.content, &sel)
                .into_iter()
                .enumerate()
            {
                let id = LayerId::from_packed(layer_id.0.wrapping_sub(i as u64 + 1));
                chunk.push(SceneCommand::BeginLayer {
                    id,
                    rect: band,
                    paint: LayerPaint {
                        material: Some(fill.clone()),
                        ..LayerPaint::default()
                    },
                });
                chunk.push(SceneCommand::EndLayer { id });
            }
        }
        chunk.push(SceneCommand::DrawText {
            origin: Point::ZERO,
            layout: Arc::clone(layout),
            color: self.color,
        });
    }
}
