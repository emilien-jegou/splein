// Single responsibility: Backend-agnostic display list drawing commands.

use crate::foundation::{
    Appearance, BlendMode, Color, Fill, LightEnvironment, Point, Radius, ResolvedRect, Shadow,
    ShadowOffset, ShadowShape, StrokeAlign, Transform,
};
use crate::scene::image::ImageSource;
use crate::scene::vector::VectorGraphic;
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
#[derive(Clone, Debug)]
pub enum SceneCommand {
    PushTransform(Transform),
    PopTransform,
    PushOffset(Point),
    PopOffset,
    PushClip {
        rect: ResolvedRect,
        radius: Radius,
    },
    PopClip,
    PushOpacity(f32),
    PopOpacity,
    BeginLayer {
        id: LayerId,
        rect: ResolvedRect,
    },
    EndLayer {
        id: LayerId,
    },
    DrawRect {
        rect: ResolvedRect,
        appearance: Appearance,
    },
    DrawImage {
        rect: ResolvedRect,
        image: ImageSource,
    },
    DrawSvg {
        rect: ResolvedRect,
        graphic: VectorGraphic,
    },
    DrawShadow {
        rect: ResolvedRect,
        radius: Radius,
        shadow: Shadow,
        light: LightEnvironment,
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
            SceneCommand::DrawRect { .. }
                | SceneCommand::DrawImage { .. }
                | SceneCommand::DrawSvg { .. }
                | SceneCommand::DrawShadow { .. }
                | SceneCommand::DrawText { .. }
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
            SceneCommand::PushClip { rect, radius } => {
                4u8.hash(&mut h);
                rect_bits(rect).hash(&mut h);
                radius_bits(radius).hash(&mut h);
            }
            SceneCommand::PopClip => 5u8.hash(&mut h),
            SceneCommand::PushOpacity(o) => {
                6u8.hash(&mut h);
                o.to_bits().hash(&mut h);
            }
            SceneCommand::PopOpacity => 7u8.hash(&mut h),
            SceneCommand::BeginLayer { id, rect } => {
                8u8.hash(&mut h);
                id.0.hash(&mut h);
                rect_bits(rect).hash(&mut h);
            }
            SceneCommand::EndLayer { id } => {
                9u8.hash(&mut h);
                id.0.hash(&mut h);
            }
            SceneCommand::DrawRect { rect, appearance } => {
                10u8.hash(&mut h);
                rect_bits(rect).hash(&mut h);
                appearance_bits(appearance).hash(&mut h);
            }
            SceneCommand::DrawImage { rect, image } => {
                11u8.hash(&mut h);
                rect_bits(rect).hash(&mut h);
                image_bits(image).hash(&mut h);
            }
            SceneCommand::DrawSvg { rect, graphic } => {
                12u8.hash(&mut h);
                rect_bits(rect).hash(&mut h);
                svg_bits(graphic).hash(&mut h);
            }
            SceneCommand::DrawShadow {
                rect,
                radius,
                shadow,
                light,
            } => {
                13u8.hash(&mut h);
                rect_bits(rect).hash(&mut h);
                radius_bits(radius).hash(&mut h);
                shadow_bits(shadow).hash(&mut h);
                light_bits(light).hash(&mut h);
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

fn radius_bits(r: &Radius) -> Vec<u32> {
    match r {
        Radius::Scalar(v) => vec![0, v.to_bits()],
        Radius::Max => vec![1],
        Radius::Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => vec![
            2,
            top_left.to_bits(),
            top_right.to_bits(),
            bottom_right.to_bits(),
            bottom_left.to_bits(),
        ],
    }
}

fn shadow_bits(s: &Shadow) -> Vec<u32> {
    let mut v = vec![
        s.elevation.to_bits(),
        match s.offset {
            ShadowOffset::Elevation => 0,
            ShadowOffset::Explicit { .. } => 1,
        },
        match s.offset {
            ShadowOffset::Elevation => 0,
            ShadowOffset::Explicit { offset_x, .. } => offset_x.to_bits(),
        },
        match s.offset {
            ShadowOffset::Elevation => 0,
            ShadowOffset::Explicit { offset_y, .. } => offset_y.to_bits(),
        },
        match s.blend {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 1,
        },
        match s.shape {
            ShadowShape::Box => 0,
            ShadowShape::Alpha => 1,
        },
        s.elevation.to_bits(),
        s.spread.to_bits(),
        s.layers().len() as u32,
    ];
    for l in s.layers() {
        v.push(l.falloff.tag() as u32);
        v.push(l.softness.to_bits());
        v.push(l.extent.to_bits());
        v.push(l.opacity.to_bits());
        match l.offset {
            None => v.push(0),
            Some((x, y)) => {
                v.push(1);
                v.push(x.to_bits());
                v.push(y.to_bits());
            }
        }
    }
    v.into_iter().chain(color_bits(&s.color)).collect()
}

fn light_bits(l: &LightEnvironment) -> [u32; 3] {
    [
        l.azimuth.to_bits(),
        l.altitude.to_bits(),
        l.intensity.to_bits(),
    ]
}

fn fill_bits(f: &Fill) -> Vec<u32> {
    match f {
        Fill::Solid(c) => [vec![0], color_bits(c).to_vec()].concat(),
        Fill::LinearGradient(g) => {
            let mut v = vec![1, g.start.x.to_bits(), g.start.y.to_bits()];
            v.extend([g.end.x.to_bits(), g.end.y.to_bits()]);
            for s in &g.stops {
                v.extend([s.position.to_bits()]);
                v.extend(color_bits(&s.color));
            }
            v
        }
        Fill::RadialGradient(g) => {
            let mut v = vec![2, g.center.x.to_bits(), g.center.y.to_bits()];
            v.push(g.radius.to_bits());
            for s in &g.stops {
                v.extend([s.position.to_bits()]);
                v.extend(color_bits(&s.color));
            }
            v
        }
    }
}

fn appearance_bits(a: &Appearance) -> Vec<u32> {
    let mut v = vec![a.opacity.to_bits()];
    match &a.fill {
        None => v.push(0),
        Some(f) => {
            v.push(1);
            v.extend(fill_bits(f));
        }
    }
    match &a.stroke {
        None => v.push(0),
        Some(s) => {
            v.push(1);
            v.push(s.width.to_bits());
            v.push(match s.align {
                StrokeAlign::Inside => 0,
                StrokeAlign::Center => 1,
                StrokeAlign::Outside => 2,
            });
            match &s.per_edge {
                None => v.push(0),
                Some(e) => {
                    v.push(1);
                    v.extend([
                        e.top.to_bits(),
                        e.right.to_bits(),
                        e.bottom.to_bits(),
                        e.left.to_bits(),
                    ]);
                }
            }
            v.extend(fill_bits(&s.fill));
        }
    }
    v.extend(radius_bits(&a.radius));
    match &a.shadow {
        None => v.push(0),
        Some(s) => {
            v.push(1);
            v.extend(shadow_bits(s));
        }
    }
    match &a.light_override {
        None => v.push(0),
        Some(l) => {
            v.push(1);
            v.extend(light_bits(l));
        }
    }
    v
}

fn image_bits(img: &ImageSource) -> [usize; 3] {
    [
        Arc::as_ptr(&img.data) as usize,
        img.width as usize,
        img.height as usize,
    ]
}

fn svg_bits(g: &VectorGraphic) -> [u32; 3] {
    [
        #[cfg(feature = "svg-runtime")]
        {
            (Arc::as_ptr(&g.tree) as usize) as u32 ^ ((Arc::as_ptr(&g.tree) as usize) >> 32) as u32
        },
        #[cfg(not(feature = "svg-runtime"))]
        {
            0u32
        },
        g.width.to_bits(),
        g.height.to_bits(),
    ]
}
