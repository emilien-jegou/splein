// Single responsibility: Engine-facing primitive traits — placed drawing
// items plus the sink core lowers chunks into.
//
// This is the backend boundary: core owns the stack walk, clip accumulation,
// opacity folding, and placement policy; engines implement pixel methods and
// never match on `SceneCommand`, never walk framing state, never decide
// placement. A backend that cannot rasterize a primitive simply doesn't
// override its method: the default is a no-op that logs once, so new core
// primitives can never break or crash an engine.
//
// Items are chunk-local: the consumer applies the chunk base
// (`translate(abs_origin) * placement`, see [`chunk_base`]) plus its own
// root device transform (tile offset, device scale) as one matrix around
// them. Transform-only motion keeps reusing lowered content because the base
// stays out of the items.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::foundation::{
    Appearance, Color, LightEnvironment, Point, Radius, ResolvedRect, Shadow, Transform,
};
use crate::scene::chunk::SceneChunk;
use crate::scene::command::SceneCommand;
use crate::scene::image::ImageSource;
use crate::scene::stack::SceneStack;
use crate::scene::vector::VectorGraphic;
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
/// was pushed. The engine builds its native mask from both.
#[derive(Clone, Debug)]
pub struct PlacedClip {
    pub rect: ResolvedRect,
    pub radius: Radius,
    pub transform: Transform,
}

/// A filled/stroked rectangle, fully placed.
#[derive(Clone, Debug)]
pub struct PlacedRect<'a> {
    pub rect: ResolvedRect,
    pub appearance: &'a Appearance,
    pub transform: Transform,
    pub opacity: f32,
}

/// A shaped text run, fully placed. `origin` is absolute in chunk-local
/// space with the settle adjustment already baked in — the engine adds its
/// root device transform and rasterizes.
#[derive(Clone, Debug)]
pub struct PlacedText<'a> {
    pub origin: Point,
    pub layout: &'a Arc<TextLayout>,
    pub color: Color,
    pub opacity: f32,
    pub transform: Transform,
    pub raster: TextRaster,
}

/// A raster image quad, fully placed.
#[derive(Clone, Debug)]
pub struct PlacedImage<'a> {
    pub rect: ResolvedRect,
    pub image: &'a ImageSource,
    pub opacity: f32,
    pub transform: Transform,
}

/// A vector graphic, fully placed.
#[derive(Clone, Debug)]
pub struct PlacedSvg<'a> {
    pub rect: ResolvedRect,
    pub graphic: &'a VectorGraphic,
    pub opacity: f32,
    pub transform: Transform,
}

/// A drop shadow, fully placed.
#[derive(Clone, Debug)]
pub struct PlacedShadow<'a> {
    pub rect: ResolvedRect,
    pub radius: Radius,
    pub shadow: &'a Shadow,
    pub light: &'a LightEnvironment,
    pub opacity: f32,
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
    pub ignored_rect: u64,
    pub ignored_text: u64,
    pub ignored_image: u64,
    pub ignored_svg: u64,
    pub ignored_shadow: u64,
}

static IGNORED_RECT: AtomicU64 = AtomicU64::new(0);
static IGNORED_TEXT: AtomicU64 = AtomicU64::new(0);
static IGNORED_IMAGE: AtomicU64 = AtomicU64::new(0);
static IGNORED_SVG: AtomicU64 = AtomicU64::new(0);
static IGNORED_SHADOW: AtomicU64 = AtomicU64::new(0);

/// Snapshot of the cumulative ignored-primitive counters.
pub fn sink_ignored_counts() -> SinkDiagnostics {
    SinkDiagnostics {
        ignored_rect: IGNORED_RECT.load(Ordering::Relaxed),
        ignored_text: IGNORED_TEXT.load(Ordering::Relaxed),
        ignored_image: IGNORED_IMAGE.load(Ordering::Relaxed),
        ignored_svg: IGNORED_SVG.load(Ordering::Relaxed),
        ignored_shadow: IGNORED_SHADOW.load(Ordering::Relaxed),
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
    /// Paints a placed rectangle.
    fn rect(&mut self, _r: &PlacedRect) {
        static WARNED: AtomicBool = AtomicBool::new(false);
        warn_ignored("rect", &WARNED, &IGNORED_RECT);
    }
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
    /// Paints a placed vector graphic.
    fn svg(&mut self, _s: &PlacedSvg) {
        static WARNED: AtomicBool = AtomicBool::new(false);
        warn_ignored("svg", &WARNED, &IGNORED_SVG);
    }
    /// Paints a placed shadow.
    fn shadow(&mut self, _s: &PlacedShadow) {
        static WARNED: AtomicBool = AtomicBool::new(false);
        warn_ignored("shadow", &WARNED, &IGNORED_SHADOW);
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
/// scale. Opacity is folded per item, transforms are absolute in chunk-local
/// space, text carries its settle rounding and raster path.
pub fn lower_chunk(
    commands: &[SceneCommand],
    base: &Transform,
    sink: &mut impl SceneSink,
) {
    let mut stack = SceneStack::new(Transform::IDENTITY, 1.0);
    stack.push_transform(base);
    let checkpoint = stack.checkpoint();
    for cmd in commands {
        match cmd {
            SceneCommand::PushTransform(t) => stack.push_transform(t),
            SceneCommand::PopTransform => stack.pop_transform(),
            SceneCommand::PushOffset(p) => stack.push_offset(*p),
            SceneCommand::PopOffset => stack.pop_transform(),
            SceneCommand::PushOpacity(o) => stack.push_opacity(*o),
            SceneCommand::PopOpacity => stack.pop_opacity(),
            SceneCommand::PushClip { rect, radius } => sink.push_clip(&PlacedClip {
                rect: *rect,
                radius: *radius,
                transform: stack.transform(),
            }),
            SceneCommand::PopClip => sink.pop_clip(),
            SceneCommand::BeginLayer { rect, .. } => sink.push_clip(&PlacedClip {
                rect: *rect,
                radius: Radius::ZERO,
                transform: stack.transform(),
            }),
            SceneCommand::EndLayer { .. } => sink.pop_clip(),
            SceneCommand::DrawRect { rect, appearance } => sink.rect(&PlacedRect {
                rect: *rect,
                appearance,
                transform: stack.transform(),
                opacity: stack.opacity(),
            }),
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
                    opacity: stack.opacity(),
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
                opacity: stack.opacity(),
                transform: stack.transform(),
            }),
            SceneCommand::DrawSvg { rect, graphic } => sink.svg(&PlacedSvg {
                rect: *rect,
                graphic,
                opacity: stack.opacity(),
                transform: stack.transform(),
            }),
            SceneCommand::DrawShadow {
                rect,
                radius,
                shadow,
                light,
            } => sink.shadow(&PlacedShadow {
                rect: *rect,
                radius: *radius,
                shadow,
                light,
                opacity: stack.opacity(),
                transform: stack.transform(),
            }),
        }
    }
    stack.restore(checkpoint);
}
