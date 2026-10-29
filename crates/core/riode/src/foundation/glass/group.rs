// Single responsibility: multi-shape glass groups (Phase 5) — the member
// a group contributes to its one shared layer, and the morph tracks that
// glide members between targets on the morph spring.
//
// Reference: `adaptive_liquid_glass/lib/src/group/` (`GlassGroup` merges
// nearby `LiquidGlass` descendants into one pass; members carry `unionId`
// for merging and `glassId` for morphing). The registry/ghost half of
// the reference morph (unmount animations) is deferred: tracks cover
// live members only — a removed member's track is dropped, not ghosted.

use crate::anim::clock::FIXED_STEP_SECS;
use crate::anim::SpringState;
use crate::foundation::glass::motion::swiftui_spring;
use crate::foundation::{Backdrop, Glass, Glow, Point, Refraction, ResolvedRect, Spectral};

use std::collections::HashMap;

/// One member shape of a glass group: its rect in group-layer-local
/// logical px and its material (radius already stamped from the member's
/// declared corners, ids from `.union_id()`/`.glass_id()`, morph glide
/// and press deformation already applied — see `collect_glass_members`).
/// The backdrop rides the member too: each shape samples its own stages
/// (blur, wash, saturation, tone) even though the packed bank is
/// group-shared from the first member.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlassMember {
    pub rect: ResolvedRect,
    pub glass: Glass,
    /// The non-glass stages this shape samples (core blur, wash,
    /// saturation, tone, ambient, fill).
    pub backdrop: Backdrop,
    /// SwiftUI's press light on this shape, 0..1 (the shader's
    /// `uExtra[N].w`): nonzero only on the pressed member.
    pub light: f32,
    /// Press radius multiplier (the geometry's `radiusScale`, about the
    /// member centre): 1.0 unless pressed. Applied at emission so the
    /// declared corners survive unpressed.
    pub radius_scale: f32,
}

impl GlassMember {
    /// The member's corner radii in logical px (tl, tr, br, bl): what
    /// the shader's `uCorner[N]` vec4 carries after the callers × the
    /// press radius scale and cap at half the shorter side. At equal
    /// declared corners this is the radius `radius` used to collapse.
    pub fn corners(&self) -> [f32; 4] {
        self.glass.shape_corners(self.rect.width, self.rect.height)
    }

    /// The member's corner radius in logical px (the single radius the
    /// shader's `uInfo[N].x` carries — the smallest corner the graphic
    /// collapses to, what the compositor wire still encodes).
    pub fn radius(&self) -> f32 {
        let c = self.corners();
        c[0].min(c[1]).min(c[2]).min(c[3])
    }
}

/// What a `BeginLayer` carries for glass: either today's single material
/// (byte-identical path, pending the human's 4.4 validation) or a
/// group's member shapes sharing one snapshot and one optics pass. The
/// spectral (rainbow) response is layer-shared: it widens the refraction
/// across a spectrum and is not a per-shape material knob.
#[derive(Clone, Debug, PartialEq)]
pub enum GlassLayer {
    Single(Glass, Backdrop, Spectral, Glow, Refraction),
    Group(GlassGroup),
}

/// One glass group's shared layer: member shapes plus the group-level
/// terms every member inherits.
#[derive(Clone, Debug, PartialEq)]
pub struct GlassGroup {
    /// Member shapes in group-layer-local logical px (radius stamped,
    /// morph glide already applied — see `collect_glass_members`).
    pub members: Vec<GlassMember>,
    /// Smooth-union radius in logical px (`spacing × mergeFactor`); the
    /// shader's `smin` blends members closer than this.
    pub smoothing: f32,
    /// The group's clip in group-layer-local logical px (`None` =
    /// unbounded). Set when the group node clips.
    pub clip: Option<ResolvedRect>,
    /// The group's opacity, folded into every member shape (the
    /// reference's "group's opacity"; member-level opacity fades content
    /// only).
    pub opacity: f32,
    /// The pressed member's touch in group-layer-local logical px plus
    /// the frame glow (`uTouch`). `None` when no member is pressed.
    pub press_touch: Option<(Point, f32)>,
    /// The spectral (rainbow) response shared by the whole layer: how the
    /// refraction splits by wavelength. Monochrome (off) by default.
    pub spectral: Spectral,
    /// The emissive bottom glow shared by the whole layer. Off by default.
    pub glow: Glow,
    /// The outer refraction halo shared by the whole layer: blurred card
    /// light past the silhouette. Off by default; independent of `glow`.
    pub refraction: Refraction,
}

impl GlassLayer {
    /// Copy shifted by (dx, dy) in logical px: members, clip and press
    /// touch all move. Used to rebase a pack into a surface's own local
    /// space (island layers pack island-local so the plugin's
    /// `surfacePos + box` rule holds for every mode).
    pub fn shifted(&self, dx: f32, dy: f32) -> Self {
        match self {
            GlassLayer::Single(g, b, s, gl, r) => GlassLayer::Single(*g, *b, *s, *gl, *r),
            GlassLayer::Group(group) => GlassLayer::Group(GlassGroup {
                members: group
                    .members
                    .iter()
                    .map(|m| GlassMember {
                        rect: ResolvedRect::new(
                            m.rect.x + dx,
                            m.rect.y + dy,
                            m.rect.width,
                            m.rect.height,
                        ),
                        glass: m.glass,
                        backdrop: m.backdrop,
                        light: m.light,
                        radius_scale: m.radius_scale,
                    })
                    .collect(),
                smoothing: group.smoothing,
                clip: group.clip.map(|c| {
                    ResolvedRect::new(c.x + dx, c.y + dy, c.width, c.height)
                }),
                opacity: group.opacity,
                press_touch: group.press_touch.map(|(p, glow)| {
                    (Point::new(p.x + dx, p.y + dy), glow)
                }),
                spectral: group.spectral,
                glow: group.glow,
                refraction: group.refraction,
            }),
        }
    }
    /// Outward reach of the glass output past the content: the max over
    /// members (a group repaints when any member's backdrop moves).
    pub fn damage_pad(&self) -> f32 {
        // The refraction halo paints outside the silhouette (blurred card
        // light, theatre mode): without a pad the island clips it and
        // backdrop damage under it never repaints the glass. Fixed 120 px
        // covers typical cards; the inactive-refraction path stays
        // byte-identical to before.
        let halo_pad = match self {
            GlassLayer::Single(_, _, _, _, r) => r.is_active(),
            GlassLayer::Group(group) => group.refraction.is_active(),
        };
        let halo_pad = if halo_pad { 120.0 } else { 0.0 };
        match self {
            GlassLayer::Single(g, b, _, _, _) => g.damage_pad(b).max(halo_pad),
            GlassLayer::Group(group) => group
                .members
                .iter()
                .map(|m| m.glass.damage_pad(&m.backdrop))
                .fold(halo_pad, f32::max),
        }
    }

    /// [`Self::damage_pad`] scaled to device px.
    pub fn device_pad(&self, scale: f32) -> f32 {
        self.damage_pad() * scale
    }

    /// Union of member rects (group-layer-local logical px). Empty when
    /// the group has no drawable members.
    pub fn union_bounds(&self) -> ResolvedRect {
        match self {
            GlassLayer::Single(..) => ResolvedRect::ZERO,
            GlassLayer::Group(group) => group.members.iter().fold(ResolvedRect::ZERO, |acc, m| {
                if acc.is_empty() {
                    m.rect
                } else if m.rect.is_empty() {
                    acc
                } else {
                    acc.union(&m.rect)
                }
            }),
        }
    }
}

/// One quantum-advanced morph track: a member's current rect +
/// corner radii gliding toward their layout target on the morph spring
/// (`morphResponse`/`morphDamping` → SwiftUI stiffness/damping, mass 1).
/// Corner radii live as 4-component tracks (tl, tr, br, bl — same order
/// as [`Radius::corners`] / the shader's `uCorner`), so a pill ⇄
/// asymmetric transition glides per corner at frame rate: geometry may
/// retarget from a signal every frame, so nothing about the corner set
/// may be baked.
#[derive(Clone, Debug)]
pub struct MorphTrack {
    x: SpringState,
    y: SpringState,
    w: SpringState,
    h: SpringState,
    r: [SpringState; 4],
}

impl MorphTrack {
    /// Starts a track settled at `to` (first appearance never animates —
    /// the reference only morphs after the group's first frame).
    pub fn settled(
        rect: ResolvedRect,
        radius: [f32; 4],
        response: f32,
        damping: f32,
    ) -> Self {
        let s = swiftui_spring(response, damping);
        Self {
            x: SpringState::new(s, rect.x, rect.x),
            y: SpringState::new(s, rect.y, rect.y),
            w: SpringState::new(s, rect.width, rect.width),
            h: SpringState::new(s, rect.height, rect.height),
            r: std::array::from_fn(|i| SpringState::new(s, radius[i], radius[i])),
        }
    }

    /// Retargets in place, preserving position and velocity so an
    /// interrupted glide continues smoothly.
    pub fn retarget(&mut self, rect: ResolvedRect, radius: [f32; 4]) {
        self.x.retarget(rect.x);
        self.y.retarget(rect.y);
        self.w.retarget(rect.width);
        self.h.retarget(rect.height);
        for (r, target) in self.r.iter_mut().zip(radius) {
            r.retarget(target);
        }
    }

    /// Advances one fixed quantum (the animation clock's step — the same
    /// grid the tickers integrate on, so frozen `.`-stepping walks a
    /// morph one quantum per press).
    pub fn advance(&mut self) {
        self.x.advance(FIXED_STEP_SECS);
        self.y.advance(FIXED_STEP_SECS);
        self.w.advance(FIXED_STEP_SECS);
        self.h.advance(FIXED_STEP_SECS);
        for r in &mut self.r {
            r.advance(FIXED_STEP_SECS);
        }
    }

    pub fn is_settled(&self) -> bool {
        self.x.is_settled()
            && self.y.is_settled()
            && self.w.is_settled()
            && self.h.is_settled()
            && self.r.iter().all(SpringState::is_settled)
    }

    /// Current rect + corner radii for this frame's uniform pack.
    pub fn current(&self) -> (ResolvedRect, [f32; 4]) {
        (
            ResolvedRect::new(self.x.position, self.y.position, self.w.position, self.h.position),
            std::array::from_fn(|i| self.r[i].position),
        )
    }

    /// Union of the current and target rects: the damage must cover both
    /// (the pixels being vacated and the pixels being occupied).
    pub fn motion_bounds(&self) -> ResolvedRect {
        let (cur, _) = self.current();
        let tgt = ResolvedRect::new(
            self.x.target,
            self.y.target,
            self.w.target,
            self.h.target,
        );
        cur.union(&tgt)
    }
}

/// Per-group morph state, keyed by member `glass_id`. Lives on the group
/// node in the arena; advanced once per frame by the damage pass, read by
/// the compile pass's shape collection.
#[derive(Clone, Debug, Default)]
pub struct GlassMorphState {
    tracks: HashMap<u64, MorphTrack>,
    /// Set when the last advance left a track unsettled (drives
    /// `has_active_motion` so the loop keeps running until the glide
    /// lands).
    pub active: bool,
}

impl GlassMorphState {
    /// Matches this frame's `glass_id` members against retained tracks:
    /// new ids settle at their target, changed targets retarget in
    /// place, vanished ids drop (no ghosts in v1). Returns the member
    /// rect/corner radii to emit — the track's current glide position
    /// when it has one, the layout target otherwise.
    pub fn resolve(
        &mut self,
        members: &[(Option<u64>, ResolvedRect, [f32; 4])],
        response: f32,
        damping: f32,
    ) -> Vec<(ResolvedRect, [f32; 4])> {
        let mut seen = Vec::with_capacity(members.len());
        let mut out = Vec::with_capacity(members.len());
        for (id, target, radius) in members {
            match id {
                None => out.push((*target, *radius)),
                Some(id) => {
                    seen.push(*id);
                    let track = self.tracks.entry(*id).or_insert_with(|| {
                        MorphTrack::settled(*target, *radius, response, damping)
                    });
                    track.retarget(*target, *radius);
                    out.push(track.current());
                }
            }
        }
        self.tracks.retain(|id, _| seen.contains(id));
        // Tracks left gliding keep the frame loop alive through the
        // next damage advance (which is where `active` is normally set;
        // without this the retarget frame would go back to sleep).
        self.active = self.tracks.values().any(|t| !t.is_settled());
        out
    }

    /// Advances every track one quantum. Returns the union of motion
    /// bounds for tracks that moved (for damage), and whether any track
    /// is still unsettled (for the frame-loop keepalive).
    pub fn advance(&mut self) -> (Option<ResolvedRect>, bool) {
        let mut moved: Option<ResolvedRect> = None;
        let mut unsettled = false;
        for track in self.tracks.values_mut() {
            if track.is_settled() {
                continue;
            }
            unsettled = true;
            let before = track.motion_bounds();
            track.advance();
            let after = track.motion_bounds();
            moved = Some(match moved {
                None => before.union(&after),
                Some(m) => m.union(&before).union(&after),
            });
            unsettled = unsettled || !track.is_settled();
        }
        self.active = unsettled;
        (moved, unsettled)
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}
