// Single responsibility: Backend-agnostic display list drawing commands.

use crate::foundation::{Color, LayerPaint, Point, ResolvedRect, Transform};
use crate::scene::image::ImageSource;
use crate::text::layout::TextLayout;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Stable identifier for cached layer-backed display list subtrees.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

impl LayerId {
    /// Wraps a packed generational node identity as a layer cache key.
    #[inline(always)]
    pub const fn from_packed(packed: u64) -> Self {
        Self(packed)
    }
}

/// Abstract display list drawing and state transformation commands.
///
/// The purge removed `DrawRect`, `DrawShadow`, `PushOpacity`/`PopOpacity`
/// and every `radius` field: a node paints exactly one thing, its
/// [`Material`] layer, plus text/image/svg content. Clips are square.
#[derive(Clone, Debug)]
pub enum SceneCommand {
    PushTransform(Transform),
    PopTransform,
    PushOffset(Point),
    PopOffset,
    PushClip {
        rect: ResolvedRect,
    },
    PopClip,
    BeginLayer {
        id: LayerId,
        rect: ResolvedRect,
        paint: LayerPaint,
    },
    EndLayer {
        id: LayerId,
    },
    DrawImage {
        rect: ResolvedRect,
        image: ImageSource,
    },
    DrawText {
        origin: Point,
        layout: Arc<TextLayout>,
        color: Color,
    },
}

impl SceneCommand {
    /// Whether this command paints pixels, as opposed to chunk framing.
    ///
    /// Push/Pop state commands are per-chunk overhead, not work: a culled
    /// subtree must move this count by zero, so "commands emitted" counts
    /// only drawing commands.
    pub fn is_drawing(&self) -> bool {
        matches!(
            self,
            SceneCommand::DrawImage { .. } | SceneCommand::DrawText { .. }
        )
    }
}

/// Content hash of a chunk-local command stream, for cross-frame encode reuse.
///
/// Placement-independent by construction: the root offset is rebased to
/// `(0,0)` and the root transform hoisted into `SceneChunk::placement`, so
/// equal hashes mean equal paint content wherever the chunk sits. Shared
/// payloads (`TextLayout`, image bytes, SVG trees) hash by allocation
/// identity — they are immutable behind `Arc`, so a changed payload is always
/// a new allocation.
pub fn hash_commands(commands: &[SceneCommand]) -> u64 {
    let mut h = rustc_hash::FxHasher::default();
    commands.len().hash(&mut h);
    for cmd in commands {
        match cmd {
            SceneCommand::PushTransform(t) => {
                0u8.hash(&mut h);
                transform_bits(t).hash(&mut h);
            }
            SceneCommand::PopTransform => 1u8.hash(&mut h),
            SceneCommand::PushOffset(p) => {
                2u8.hash(&mut h);
                point_bits(p).hash(&mut h);
            }
            SceneCommand::PopOffset => 3u8.hash(&mut h),
            SceneCommand::PushClip { rect } => {
                4u8.hash(&mut h);
                rect_bits(rect).hash(&mut h);
            }
            SceneCommand::PopClip => 5u8.hash(&mut h),
            SceneCommand::BeginLayer {
                id,
                rect,
                paint,
            } => {
                8u8.hash(&mut h);
                id.0.hash(&mut h);
                rect_bits(rect).hash(&mut h);
                hash_chain(paint.shape.as_ref().map(|c| &c.stages), &mut h);
                hash_chain(paint.material.as_ref().map(|c| &c.stages), &mut h);
                hash_chain(paint.overlay.as_ref().map(|c| &c.stages), &mut h);
                hash_chain(paint.effect.as_ref().map(|c| &c.stages), &mut h);
            }
            SceneCommand::EndLayer { id } => {
                9u8.hash(&mut h);
                id.0.hash(&mut h);
            }
            SceneCommand::DrawImage { rect, image } => {
                11u8.hash(&mut h);
                rect_bits(rect).hash(&mut h);
                image_bits(image).hash(&mut h);
            }
            SceneCommand::DrawText {
                origin,
                layout,
                color,
            } => {
                14u8.hash(&mut h);
                point_bits(origin).hash(&mut h);
                (Arc::as_ptr(layout) as usize).hash(&mut h);
                color_bits(color).hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Bit-stable identity of a placement transform, for fragment cache keys.
pub fn transform_bits(t: &Transform) -> [u32; 6] {
    [
        t.a.to_bits(),
        t.b.to_bits(),
        t.c.to_bits(),
        t.d.to_bits(),
        t.tx.to_bits(),
        t.ty.to_bits(),
    ]
}

/// Presence + allocation identity of one stage chain, for cache keys.
fn hash_chain<T>(chain: Option<&Arc<Vec<T>>>, h: &mut impl Hasher) {
    match chain {
        None => 0u8.hash(h),
        Some(c) => {
            1u8.hash(h);
            Arc::as_ptr(c).hash(h);
        }
    }
}

fn point_bits(p: &Point) -> [u32; 2] {
    [p.x.to_bits(), p.y.to_bits()]
}

fn rect_bits(r: &ResolvedRect) -> [u32; 4] {
    [
        r.x.to_bits(),
        r.y.to_bits(),
        r.width.to_bits(),
        r.height.to_bits(),
    ]
}

fn color_bits(c: &Color) -> [u32; 4] {
    [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]
}

fn image_bits(img: &ImageSource) -> [usize; 3] {
    [
        Arc::as_ptr(&img.data) as usize,
        img.width as usize,
        img.height as usize,
    ]
}

