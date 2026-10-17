//! Backend-agnostic display list: what to paint, where, in what order.
//!
//! The compile pass ([`crate::pipeline`]) translates the retained tree into a [`Scene`]
//! — a flat list of [`SceneChunk`]s, each a bounded slice of [`SceneCommand`]s with a
//! placement transform and clips. Backends (`riode-tiny-skia`, `riode-wgpu`) replay
//! these chunks without knowing what a node is. That ignorance is the backend boundary:
//! everything tree-shaped stays on this side of it.
//!
//! # The command vocabulary
//!
//! Two kinds, and the distinction matters for diagnostics: `Draw*`
//! ([`SceneCommand::DrawRect`], `DrawText`, `DrawImage`, `DrawSvg`, `DrawShadow`) paint
//! pixels; `Push*`/`Pop*`/`BeginLayer`/`EndLayer` are chunk framing (offsets, clips,
//! opacity, isolation). `commands_emitted` counts drawing commands only — framing is
//! overhead, not work.
//!
//! # Showcase
//!
//! Custom [`Primitive`](crate::tree::Primitive)s paint by emitting commands, not pixels:
//!
//! ```ignore
//! use riode::prelude::*;
//! use riode::scene::{PaintContext, SceneCommand};
//! use riode::foundation::{Point, ResolvedRect};
//!
//! struct Crosshair;
//! impl Primitive for Crosshair {
//!     fn measure(&self, _: Constraints) -> IntrinsicSize {
//!         IntrinsicSize { width: 20.0, height: 20.0 }
//!     }
//!     fn paint(&self, ctx: &mut PaintContext, bounds: ResolvedRect) {
//!         // Placement, clipping, and culling are the chunk's problem;
//!         // here you just describe marks in local coordinates.
//!         ctx.draw_rect(bounds, Appearance::EMPTY.with_fill(Fill::solid(Color::RED)));
//!     }
//! }
//! ```
//!
//! [`PaintContext`] is also the scope for `with_transform` / `with_clip` nesting, and
//! [`VectorGraphic::from_str`] / [`ImageSource::from_rgba8`] are the asset handles the
//! `svg()` / `image()` builders carry.
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`scene`] | [`Scene`]: the per-frame chunk list (cleared + rebuilt each compile) |
//! | [`chunk`] | [`SceneChunk`]: bounds, placement, clips, retained-cache identity |
//! | [`command`] | [`SceneCommand`], [`command::LayerId`], `hash_commands` (allocation-identity content hashing for chunk reuse) |
//! | [`context`] | [`PaintContext`]: the emit API custom primitives see |
//! | [`image`] / [`vector`] | [`ImageSource`], [`VectorGraphic`]: retained asset handles |
//!
//! # Pitfalls
//!
//! - Chunks are chunk-*local*: streams are rebased to `(0,0)` with placement hoisted
//!   into `abs_origin` + `placement`. Reading a `DrawRect`'s rect as screen coordinates
//!   is wrong — apply placement first (which is exactly what moved-chunk reuse relies on).
//! - `hash_commands` hashes shared payloads (`TextLayout`, image bytes, SVG trees) by
//!   *allocation identity*: same pixels, new allocation → new hash → re-encode. If your
//!   cache hit rate looks wrong, look for per-frame reallocations first.

pub mod chunk;
pub mod command;
pub mod context;
pub mod image;
pub mod scene;
pub mod settle;
pub mod sink;
pub mod snap;
pub mod stack;
pub mod vector;

pub use chunk::SceneChunk;
pub use command::{LayerId, SceneCommand};
pub use context::PaintContext;
pub use image::ImageSource;
pub use scene::Scene;
pub use sink::{
    chunk_base, lower_chunk, sink_ignored_counts, PlacedClip, PlacedImage, PlacedRect,
    PlacedShadow, PlacedSvg, PlacedText, SceneSink, SinkDiagnostics, TextRaster,
};
pub use settle::RectSettle;
pub use snap::{
    hairline_adjust, has_scale_or_skew, integerize_damage, quantize_nths, quantize_quarter,
    quantize_y, raster_cache_size, settle_round, snap_rect_to_device, snap_transform_to_device,
};
pub use snap::GLYPH_QUANTA_Y;
pub use stack::SceneStack;
pub use vector::VectorGraphic;
