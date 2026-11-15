// Single responsibility: Engine-facing primitive traits — placed drawing
// items plus the sink core lowers chunks into.
//
// This is the backend boundary: core owns the stack walk, clip accumulation,
// and placement policy; engines implement pixel methods and never match on
// `SceneCommand`, never walk framing state, never decide placement. A backend
// that cannot rasterize a primitive simply doesn't override its method: the
// default is a no-op that logs once, so new core primitives can never break
// or crash an engine.
//
// Items are chunk-local: the consumer applies the chunk base
// (`translate(abs_origin) * placement`, see [`chunk_base`]) plus its own
// root device transform (tile offset, device scale) as one matrix around
// them. Transform-only motion keeps reusing lowered content because the base
// stays out of the items.
//
// `DrawRect`/`DrawShadow` and the folded opacity are gone: a node's paint is
// its material layer, so the sink's primitives are text, image and the
// layer open/close pair.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::foundation::{Color, LayerPaint, Point, ResolvedRect, Transform};
use crate::scene::chunk::SceneChunk;
use crate::scene::command::SceneCommand;
use crate::scene::image::ImageSource;
use crate::scene::stack::SceneStack;
use crate::text::layout::TextLayout;
use std::sync::Arc;

/// Which raster path a text run takes. Decided by core from the absolute
/// transform — engines just branch to their existing implementations.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextRaster {
    /// Translation-only: integer-anchored analytic blits.
    Aligned,
    /// Scale/skew/rotation present: exact fractional placement.
    Transformed,
}

/// A clip in chunk-local space with the absolute transform active when it
/// was pushed. The engine builds its native mask from both. Clips are
/// square — `radius` was purged.
#[derive(Clone, Debug)]
pub struct PlacedClip {
    pub rect: ResolvedRect,
    pub transform: Transform,
}

/// A shaped text run, fully placed. `origin` is absolute in chunk-local
/// space with the settle adjustment already baked in — the engine adds its
/// root device transform and rasterizes.
#[derive(Clone, Debug)]
pub struct PlacedText<'a> {
    pub origin: Point,
    pub layout: &'a Arc<TextLayout>,
    pub color: Color,
    pub transform: Transform,
    pub raster: TextRaster,
}

/// A raster image quad, fully placed.
#[derive(Clone, Debug)]
pub struct PlacedImage<'a> {
    pub rect: ResolvedRect,
    pub image: &'a ImageSource,
    pub transform: Transform,
}

/// An isolated layer carrying its paint bundle, fully placed.
///
/// The paint rides along as the contract for the shader rewrite: the
/// current wgpu sink reads `None`/`Some` only to decide "isolation
/// blit", and composites the layer's content unchanged — no material
/// pass runs yet. `None` and `Some` therefore render identically today.
#[derive(Clone, Debug)]
pub struct PlacedLayer<'a> {
    pub rect: ResolvedRect,
    pub paint: &'a LayerPaint,
    pub transform: Transform,
}

fn warn_ignored(what: &'static str, flag: &AtomicBool, counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
    if !flag.swap(true, Ordering::Relaxed) {
        tracing::warn!("backend ignores primitive `{what}` (further occurrences silent)");
    }
}

/// Per-primitive drop counters: how many placed items the active sink
/// declined to rasterize. Non-zero means the backend doesn't implement that
/// primitive — visible degradation, not a crash.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SinkDiagnostics {
    pub ignored_text: u64,
    pub ignored_image: u64,
}

static IGNORED_TEXT: AtomicU64 = AtomicU64::new(0);
static IGNORED_IMAGE: AtomicU64 = AtomicU64::new(0);

/// Snapshot of the cumulative ignored-primitive counters.
pub fn sink_ignored_counts() -> SinkDiagnostics {
    SinkDiagnostics {
        ignored_text: IGNORED_TEXT.load(Ordering::Relaxed),
        ignored_image: IGNORED_IMAGE.load(Ordering::Relaxed),
    }
}

/// Engine-facing raster contract: one method per primitive, all defaulting
/// to a log-once no-op so backends implement only what they can paint.
pub trait SceneSink {
    /// Pushes a clip mask built from the absolute rect and transform.
    /// Mask construction is rendering, so this (and pop) is the one
    /// framing concept engines keep.
    fn push_clip(&mut self, _clip: &PlacedClip) {}
    /// Pops the most recent clip mask.
    fn pop_clip(&mut self) {}
    /// Paints a placed text run.
    fn text(&mut self, _t: &PlacedText) {
        static WARNED: AtomicBool = AtomicBool::new(false);
        warn_ignored("text", &WARNED, &IGNORED_TEXT);
    }
    /// Paints a placed image.
    fn image(&mut self, _i: &PlacedImage) {
        static WARNED: AtomicBool = AtomicBool::new(false);
        warn_ignored("image", &WARNED, &IGNORED_IMAGE);
    }
    /// Opens an isolated layer. The default treats it as a plain clip, so
    /// engines that don't implement layers degrade to clip semantics
    /// instead of dropping content.
    fn begin_layer(&mut self, layer: &PlacedLayer) {
        self.push_clip(&PlacedClip {
            rect: layer.rect,
            transform: layer.transform,
        });
    }
    /// Closes the most recent layer (matching pop for the default clip).
    fn end_layer(&mut self) {
        self.pop_clip();
    }
}

/// Chunk base as a single transform: `translate(abs_origin) * placement`.
/// The consumer composes this with its root device transform once per chunk
/// instead of re-deriving placement per command.
pub fn chunk_base(chunk: &SceneChunk) -> Transform {
    Transform::from_translation(chunk.abs_origin.x, chunk.abs_origin.y)
        .multiply(&chunk.placement)
}

/// Lowers one chunk's command stream into sink calls — the single stack walk
/// in the codebase. `base` is the chunk base (`translate(abs_origin) *
/// placement`, see [`chunk_base`]); it is the walk's root frame, so absolute
/// transforms — including the text raster verdict — account for placement
/// scale. Transforms are absolute in chunk-local space, text carries its
/// settle rounding and raster path.
pub fn lower_chunk(
    commands: &[SceneCommand],
    base: &Transform,
    sink: &mut impl SceneSink,
) {
    let mut stack = SceneStack::new(Transform::IDENTITY);
    stack.push_transform(base);
    let checkpoint = stack.checkpoint();
    for cmd in commands {
        match cmd {
            SceneCommand::PushTransform(t) => stack.push_transform(t),
            SceneCommand::PopTransform => stack.pop_transform(),
            SceneCommand::PushOffset(p) => stack.push_offset(*p),
            SceneCommand::PopOffset => stack.pop_transform(),
            SceneCommand::PushClip { rect } => sink.push_clip(&PlacedClip {
                rect: *rect,
                transform: stack.transform(),
            }),
            SceneCommand::PopClip => sink.pop_clip(),
            SceneCommand::BeginLayer { rect, paint, .. } => {
                sink.begin_layer(&PlacedLayer {
                    rect: *rect,
                    paint,
                    transform: stack.transform(),
                })
            }
            SceneCommand::EndLayer { .. } => sink.end_layer(),
            SceneCommand::DrawText {
                origin,
                layout,
                color,
            } => {
                let t = stack.transform();
                sink.text(&PlacedText {
                    origin: *origin,
                    layout,
                    color: *color,
                    transform: t,
                    raster: if crate::scene::has_scale_or_skew(&t) {
                        TextRaster::Transformed
                    } else {
                        TextRaster::Aligned
                    },
                })
            }
            SceneCommand::DrawImage { rect, image } => sink.image(&PlacedImage {
                rect: *rect,
                image,
                transform: stack.transform(),
            }),
        }
    }
    stack.restore(checkpoint);
}
