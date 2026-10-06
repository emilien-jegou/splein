// Single responsibility: Finding painted content that escapes where it should not stay.

use aurora::foundation::{Point, ResolvedRect};
use aurora::runtime::Engine;
use crate::testing::snapshot::{Rgba, Snapshot};
use aurora::tree::{NodeId, TreeArena};

/// A geometric observation worth reading. None of these are automatically a bug: a clipping
/// parent is *supposed* to contain content that reaches past it, which is how a reveal works.
#[derive(Clone, Debug, PartialEq)]
pub enum Escape {
    /// A node's children reach outside a parent that does not clip them.
    ///
    /// Without a clip those children paint over whatever is behind the parent, which is
    /// usually a mistake.
    ContentOutsideParent {
        child: Option<String>,
        parent: Option<String>,
        /// How far past the parent's edge the content reaches.
        overshoot: f32,
    },
    /// A node's box lies wholly outside the window.
    OutsideViewport {
        node: Option<String>,
        rect: ResolvedRect,
    },
}

impl Escape {
    /// A one-line description for a failure message.
    pub fn describe(&self) -> String {
        match self {
            Self::ContentOutsideParent {
                child,
                parent,
                overshoot,
            } => format!(
                "{} paints {:.1}px outside {}, which does not clip",
                label(child),
                overshoot,
                label(parent)
            ),
            Self::OutsideViewport { node, rect } => format!(
                "{} is at {:.0},{:.0} {:.0}x{:.0}, outside the window",
                label(node),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            ),
        }
    }
}

/// Renders a key for a message, or a placeholder when the node is unkeyed.
fn label(key: &Option<String>) -> String {
    match key {
        Some(k) => format!("{k:?}"),
        None => "an unkeyed node".to_string(),
    }
}

/// How far `inner` reaches past `outer` on any edge.
fn overshoot(inner: &ResolvedRect, outer: &ResolvedRect) -> f32 {
    let (left, right) = (outer.x - inner.x, inner.right() - outer.right());
    let (top, bottom) = (outer.y - inner.y, inner.bottom() - outer.bottom());
    [left, right, top, bottom]
        .into_iter()
        .fold(0.0f32, f32::max)
}

/// Walks the tree and reports geometry that reaches outside a container.
///
/// Purely informational. Text bounds include ascent and descent, and a clipping parent is
/// meant to overflow, so this reports observations rather than failures — see
/// [`assert_no_viewport_overflow`] for the check that is unambiguous.
pub fn report(engine: &Engine) -> Vec<Escape> {
    let viewport = ResolvedRect::new(
        0.0,
        0.0,
        engine.logical_size().0 as f32,
        engine.logical_size().1 as f32,
    );
    engine.with_arena(|arena, root| walk(arena, root, viewport, true))
}

/// Whether a node is the root, which is exempt from the viewport check by definition.
fn walk(arena: &TreeArena, id: NodeId, viewport: ResolvedRect, is_root: bool) -> Vec<Escape> {
    let mut found = Vec::new();
    let node = arena.get(id);
    let key = node.key.as_ref().map(|k| k.0.clone());
    let box_abs = arena.node_absolute_rect(id);

    if !is_root && box_abs.width > 0.0 && !box_abs.overlaps(&viewport) {
        found.push(Escape::OutsideViewport {
            node: key.clone(),
            rect: box_abs,
        });
    }

    let children = arena.children(id).to_vec();
    for child_id in &children {
        found.extend(walk(arena, *child_id, viewport, false));
    }

    if children.is_empty() || node.style.clip {
        // A clipping parent containing overflowing content is the reveal mechanism, not a bug.
        return found;
    }

    let (mut content, mut worst) = (ResolvedRect::ZERO, (0.0f32, None::<String>));
    for child_id in &children {
        if matches!(arena.get(*child_id).kind, aurora::tree::NodeKind::Text(_)) {
            // A text box includes ascent and descent, so its bounds legitimately exceed the
            // ink. Including it here would report every label as an overflow.
            continue;
        }
        let abs = arena.node_absolute_rect(*child_id);
        content = if content == ResolvedRect::ZERO {
            abs
        } else {
            content.union(&abs)
        };
        let past = overshoot(&abs, &box_abs);
        if past > worst.0 {
            worst = (past, arena.get(*child_id).key.as_ref().map(|k| k.0.clone()));
        }
    }

    if content != ResolvedRect::ZERO && worst.0 > 1.0 {
        found.push(Escape::ContentOutsideParent {
            child: worst.1,
            parent: key,
            overshoot: worst.0,
        });
    }
    found
}

/// Panics if any pixel outside the frame's inset margin differs from the page background.
///
/// This is the check worth asserting on. Geometry alone cannot distinguish a clipped child
/// from a visible one, but a painted pixel can: anything in the margin band that is not the
/// page colour was drawn outside the content box.
pub fn assert_no_viewport_overflow(
    frame: &Snapshot,
    background: Rgba,
    margin: u32,
) -> Result<(), String> {
    let w = frame.width();
    let h = frame.height();
    let mut offenders: Vec<String> = Vec::new();

    let mut probe = |x: u32, y: u32| {
        let p = frame.color(x, y);
        if p.near(background, 2) {
            return;
        }
        if offenders.len() < 8 {
            offenders.push(format!("({x},{y}) is {:?}, not the page colour", p));
        }
    };

    for x in 0..w {
        if x < margin || x >= w.saturating_sub(margin) {
            for y in 0..h {
                probe(x, y);
            }
        }
    }
    for y in 0..h {
        if y < margin || y >= h.saturating_sub(margin) {
            for x in margin..w.saturating_sub(margin) {
                probe(x, y);
            }
        }
    }

    if offenders.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "painted outside the {margin}px page margin:\n    {}",
            offenders.join("\n    ")
        ))
    }
}

/// Whether a point lies inside the window.
pub fn in_viewport(size: (u32, u32), p: Point) -> bool {
    p.x >= 0.0 && p.y >= 0.0 && p.x < size.0 as f32 && p.y < size.1 as f32
}

/// Panics if any two of the given boxes intersect.
///
/// A much sharper invariant than "nothing overflows": a stack of cards must occupy distinct
/// bands of the screen, so any overlap between them is unambiguously wrong. Naming the boxes
/// makes the failure readable without needing a picture.
pub fn assert_disjoint(named: &[(&str, ResolvedRect)]) {
    for i in 0..named.len() {
        for j in (i + 1)..named.len() {
            let (an, a) = named[i];
            let (bn, b) = named[j];
            if !a.intersects(&b) {
                continue;
            }
            // Touching edges are not overlapping; only real shared area is.
            let overlap_w = a.right().min(b.right()) - a.x.max(b.x);
            let overlap_h = a.bottom().min(b.bottom()) - a.y.max(b.y);
            if overlap_w <= 0.5 || overlap_h <= 0.5 {
                continue;
            }
            panic!(
                "{an} and {bn} overlap by {overlap_w:.1}x{overlap_h:.1}px:\n  {an} {a:?}\n  {bn} {b:?}"
            );
        }
    }
}
