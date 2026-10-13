// Single responsibility: Discrete bounded slice of display list commands.

use crate::foundation::{Point, Radius, ResolvedRect, Transform};
use crate::scene::command::{LayerId, SceneCommand};
use smallvec::SmallVec;
use std::rc::Rc;

/// A clip rectangle in absolute screen space, with the radius it was declared with.
///
/// Radius survives as authored rather than being folded into the rect because the
/// renderer builds the mask path from both.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct InheritedClip {
    pub rect: ResolvedRect,
    pub radius: Radius,
}

/// Clips a chunk inherits from ancestors, already expressed in absolute screen space.
///
/// A chunk is replayed on its own, so an ancestor clip is not "still active" when the
/// chunk's commands run. These are re-pushed at the top of every chunk whose subtree
/// crosses a chunk boundary, which is what keeps a clipped parent's contents clipped.
pub type InheritedClips = SmallVec<[InheritedClip; 2]>;

/// A discrete, bounded slice of drawing commands belonging to a layout boundary or layer.
///
/// Coordinate contract (chunk-local emission):
/// - `commands` are expressed relative to the chunk origin: the root frame's
///   offset is rebased to `(0,0)`, so replaying the stream paints the chunk as
///   if its origin sat at the screen origin.
/// - `abs_origin` + `placement` locate it: consumers apply
///   `translate(abs_origin) * placement` around the stream. `placement` is the
///   stacking-context root's own effective transform, hoisted out of the stream
///   so a transform-only motion changes placement without touching content.
/// - `inherited_clips` stay ABSOLUTE: they are placement-time state applied by
///   the consumer around the placed stream, never baked into commands.
/// - `bounds` stays ABSOLUTE: spatial index for damage/viewport culling.
/// - `fresh` tells consumers whether content was recompiled this frame
///   (`true`) or replayed from cache (`false`).
#[derive(Clone, Debug, Default)]
pub struct SceneChunk {
    pub bounds: ResolvedRect,
    pub abs_origin: Point,
    /// Shared command stream: replaying a cached chunk bumps a refcount
    /// instead of deep-cloning every command each frame. Mutated only while
    /// compiling a fresh chunk (uniquely owned then); `make_mut` keeps a
    /// correct copy-on-write fallback if that ever changes.
    pub commands: Rc<Vec<SceneCommand>>,
    pub layer_id: Option<LayerId>,
    /// Ancestor clips baked into this chunk's command list, kept for cache validation.
    pub inherited_clips: InheritedClips,
    /// Stacking-root effective transform, hoisted out of the command stream.
    pub placement: Transform,
    /// Whether commands were recompiled this frame (vs replayed from cache).
    pub fresh: bool,
    /// Content hash memoized at finalization (N4): backends read this
    /// instead of re-walking the stream per frame per tile. `None` means
    /// unhashed (hand-built chunks); `push_chunk` fills it exactly once,
    /// clones carry it, and any `push` clears it.
    pub content_hash: Option<u64>,
}

impl SceneChunk {
    pub fn new(bounds: ResolvedRect, abs_origin: Point) -> Self {
        Self {
            bounds,
            abs_origin,
            commands: Rc::new(Vec::new()),
            layer_id: None,
            inherited_clips: InheritedClips::new(),
            placement: Transform::IDENTITY,
            fresh: true,
            content_hash: None,
        }
    }

    pub fn with_commands(
        bounds: ResolvedRect,
        abs_origin: Point,
        commands: Vec<SceneCommand>,
    ) -> Self {
        Self {
            bounds,
            abs_origin,
            commands: Rc::new(commands),
            layer_id: None,
            inherited_clips: InheritedClips::new(),
            placement: Transform::IDENTITY,
            fresh: true,
            content_hash: None,
        }
    }

    pub fn with_layer(bounds: ResolvedRect, abs_origin: Point, layer_id: LayerId) -> Self {        Self {
            bounds,
            abs_origin,
            commands: Rc::new(Vec::new()),
            layer_id: Some(layer_id),
            inherited_clips: InheritedClips::new(),
            placement: Transform::IDENTITY,
            fresh: true,
            content_hash: None,
        }
    }

    /// Rebases the root frame offset to chunk-local `(0,0)`.
    ///
    /// The first `PushOffset` in the stream is the stacking root's absolute
    /// origin by construction; nested offsets are already parent-relative.
    /// Called once at chunk finalization, before caching. Memoizes the
    /// content hash: the stream is complete here, so backends never re-walk
    /// it for an unchanged chunk.
    pub fn rebase_to_local(&mut self) {
        let (ox, oy) = (self.abs_origin.x, self.abs_origin.y);
        for cmd in Rc::make_mut(&mut self.commands).iter_mut() {
            if let SceneCommand::PushOffset(p) = cmd {
                p.x -= ox;
                p.y -= oy;
                break;
            }
        }
        self.content_hash = Some(super::command::hash_commands(&self.commands));
    }

    #[inline(always)]
    pub fn push(&mut self, cmd: SceneCommand) {
        Rc::make_mut(&mut self.commands).push(cmd);
        // Any mutation voids the memoized hash (recomputed at finalization).
        self.content_hash = None;
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
