// Single responsibility: Text as a layout leaf — measure, min-content, and paint behavior.
//
// Layout treats a text node as opaque sizing behavior behind these methods: it never
// shapes, wraps, or inspects glyphs itself. Wrapping width derivation, shaping calls,
// min-content floors, and display-list emission all live here, next to the shaping
// cache they rely on.

use std::sync::Arc;

use aurora_foundation::{Appearance, Constraints, Direction, Fill, IntrinsicSize, Point, Size};
use crate::scene::{SceneChunk, SceneCommand};
use crate::text::config::TextConfig;
use crate::text::decoration::decoration_rules;
use crate::text::layout::TextLayout;
use crate::text::selection::{caret_rect, selection_rects, TextSelection};
use crate::text::TextContext;

impl TextConfig {
    /// Inline width text wraps against, from the node's width intent and offered space.
    ///
    /// Moved out of layout measurement so the wrapping policy (tight widths bind,
    /// `Fit` is unbounded, `Fill`/`Percent` clamp to what is available) is defined
    /// wherever shaping is defined.
    pub fn wrap_width(style_width: Size, constraints: Constraints) -> f32 {
        // When the width is already resolved (tight), reuse it directly instead of
        // re-applying the sizing intent, which would double-count Fill/Percent and
        // ignore a wrapped cross size.
        if constraints.is_tight_width() {
            return constraints.max_width.max(0.0);
        }
        if matches!(style_width, Size::Fit) {
            return f32::INFINITY;
        }
        let available = constraints.max_width;
        match style_width {
            Size::Fit => f32::INFINITY,
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

    /// Emits a shaped run, its selection highlight, decorations, and caret.
    ///
    /// The highlight is pushed *before* the glyphs so translucent selection
    /// tints the background rather than washing the text out; decorations stay
    /// on top as before. A collapsed selection paints a 1px caret *after* the
    /// glyphs when `focused` — the caller gates that on focus and the blink
    /// phase, so blur and blink-off hide it while keeping the range.
    pub fn emit(
        &self,
        layout: &Arc<TextLayout>,
        chunk: &mut SceneChunk,
        selection: Option<TextSelection>,
        focused: bool,
    ) {
        // Alignment is baked into the shaped layout, so it is shared without mutation.
        if let Some(sel) = selection {
            if !sel.is_collapsed() {
                let rects = selection_rects(layout, &self.content, &sel);
                tracing::debug!(
                    range = ?sel.normalized(),
                    rects = rects.len(),
                    lines = layout.lines.len(),
                    content_len = self.content.len(),
                    "painting highlight"
                );
                for rect in rects {
                    chunk.push(SceneCommand::DrawRect {
                        rect,
                        appearance: Appearance::EMPTY
                            .with_fill(Fill::Solid(self.selection_color)),
                    });
                }
            }
        }
        let decorations: Vec<aurora_foundation::ResolvedRect> = if self.decoration.is_empty() {
            Vec::new()
        } else {
            layout
                .lines
                .iter()
                .flat_map(|line| decoration_rules(line, self.size, self.decoration))
                .collect()
        };
        chunk.push(SceneCommand::DrawText {
            origin: Point::ZERO,
            layout: Arc::clone(layout),
            color: self.color,
        });
        for rect in decorations {
            chunk.push(SceneCommand::DrawRect {
                rect,
                appearance: Appearance::EMPTY.with_fill(Fill::Solid(self.color)),
            });
        }
        // A focused caret paints last, over the glyphs. Unfocused collapsed
        // selections stay invisible, like a blurred browser field.
        if focused {
            if let Some(sel) = selection {
                if sel.is_collapsed() {
                    match caret_rect(layout, &self.content, sel.focus) {
                        Some(rect) => {
                            tracing::debug!(offset = sel.focus, "painting caret");
                            chunk.push(SceneCommand::DrawRect {
                                rect,
                                appearance: Appearance::EMPTY
                                    .with_fill(Fill::Solid(self.color)),
                            });
                        }
                        None => {
                            tracing::debug!(
                                offset = sel.focus,
                                lines = layout.lines.len(),
                                "caret rect missing"
                            );
                        }
                    }
                }
            }
        }
    }
}
