// Single responsibility: Liquid-Glass motion — touch deformation, springs,
// and the press geometry that rides a shape into the uniform pack.
//
// Source: `adaptive_liquid_glass/lib/src/core/glass_motion_constants.dart`
// and `adaptive_liquid_glass/lib/src/group/glass_press_geometry.dart`.

use crate::anim::Spring;
use crate::anim::SpringState;
use crate::foundation::{Point, ResolvedRect};

/// How long pressed glass stays lit after the finger lifts, then goes
/// dark (measured on iOS 26.4: 20 frames after the release starts).
/// Source: `kPressLightHold` in `glass_press_controller.dart`.
pub const PRESS_LIGHT_HOLD_SECS: f32 = 0.33;

/// Share of the press light lost over [`PRESS_LIGHT_HOLD_SECS`].
pub const PRESS_LIGHT_HOLD_FADE: f32 = 0.2;

/// Converts SwiftUI's `.spring(response:dampingFraction:)` to riode
/// springs (mass 1): stiffness = (2π/response)², damping = 4π·ζ/response.
/// Source: `swiftUISpring` in the reference.
pub(crate) fn swiftui_spring(response: f32, damping_fraction: f32) -> Spring {
    Spring::new(
        (2.0 * std::f32::consts::PI / response).powi(2),
        4.0 * std::f32::consts::PI * damping_fraction / response,
        1.0,
    )
}

/// Motion constants for `.interactive()` and `glassId` morphs. Transcribed
/// from `GlassMotionConstants` (defaults included).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlassMotionConstants {
    /// Area (pt²) a pressed shape gains at full press: its uniform scale is
    /// `√(1 + pressGrowthArea / (w·h))`, capped at [`Self::press_scale_max`],
    /// so small shapes grow more than large ones.
    pub press_growth_area: f32,
    /// Largest uniform press scale.
    pub press_scale_max: f32,
    /// Extra scale along the touch direction at full press.
    pub press_stretch: f32,
    /// Radius of the touch glow (logical px).
    pub glow_radius: f32,
    /// SwiftUI spring response for the press-in.
    pub press_response: f32,
    /// SwiftUI damping fraction for the press-in.
    pub press_damping: f32,
    /// SwiftUI spring response for the release.
    pub release_response: f32,
    /// SwiftUI damping fraction for the release.
    pub release_damping: f32,
    /// SwiftUI spring response for morphs.
    pub morph_response: f32,
    /// SwiftUI damping fraction for morphs.
    pub morph_damping: f32,
    /// Largest extra scale along an axis when the finger is dragged past
    /// the pressed shape's edge on that axis: the shape rubber-bands toward
    /// the finger, its opposite edge anchored. Estimated by eye from iOS 26
    /// buttons, not measured.
    pub drag_stretch: f32,
    /// Drag distance (pt) past the edge at which the stretch reaches
    /// `1 − 1/e` (63 %) of [`Self::drag_stretch`]; further drags add less
    /// and less. Estimated, not measured.
    pub drag_stretch_distance: f32,
}

impl GlassMotionConstants {
    /// The shipped defaults (both from `GlassMotionConstants`' constructor
    /// defaults and the reference's `GlassConstants.standard().motion`).
    pub const fn standard() -> Self {
        Self {
            press_growth_area: 1800.0,
            press_scale_max: 1.36,
            press_stretch: 0.006,
            glow_radius: 40.0,
            press_response: 0.28,
            press_damping: 0.63,
            release_response: 0.38,
            release_damping: 0.55,
            morph_response: 0.5,
            morph_damping: 0.7,
            drag_stretch: 0.08,
            drag_stretch_distance: 60.0,
        }
    }

    /// Uniform scale a shape of `width × height` (logical px) gains at full
    /// press: `min(pressScaleMax, √(1 + pressGrowthArea / area))`.
    ///
    /// Mirrors `pressScaleFor`; a zero or negative area returns
    /// `pressScaleMax` like the reference.
    pub fn press_scale_for(&self, width: f32, height: f32) -> f32 {
        let area = width * height;
        if area <= 0.0 {
            return self.press_scale_max;
        }
        (1.0 + self.press_growth_area / area).sqrt().min(self.press_scale_max)
    }
}

/// Press deformation of one member, produced by the press controller.
/// Source: `GlassPressGeometry`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GlassPressGeometry {
    /// Horizontal scale about the shape centre.
    pub scale_x: f32,
    /// Vertical scale about the shape centre.
    pub scale_y: f32,
    /// Offset of the shape centre, logical px.
    pub translation: Point,
    /// Touch glow strength, 0..1.
    pub glow: f32,
    /// SwiftUI's press light, 0..1: on at the touch, off a moment after the
    /// release (`GlassPressController::light`). The glass shader draws it
    /// as `uExtra[N].w` (`light`).
    pub light: f32,
    /// Touch point in the member's local coordinates, logical px.
    pub touch: Option<Point>,
}

impl GlassPressGeometry {
    /// No deformation.
    pub const IDENTITY: Self = Self {
        scale_x: 1.0,
        scale_y: 1.0,
        translation: Point::ZERO,
        glow: 0.0,
        light: 0.0,
        touch: None,
    };

    /// Corner radii scale with the smaller axis scale. Applied to the
    /// shape's radius *before* the uniform pack, alongside
    /// [`Self::apply`].
    pub fn radius_scale(&self) -> f32 {
        self.scale_x.min(self.scale_y)
    }

    /// `[r]` deformed by this geometry: centre shifted by `translation`,
    /// size scaled about the centre. Mirrors `GlassPressGeometry.apply`
    /// (member-local logical px in and out).
    pub fn apply(&self, r: ResolvedRect) -> ResolvedRect {
        let cx = r.x + r.width / 2.0 + self.translation.x;
        let cy = r.y + r.height / 2.0 + self.translation.y;
        let w = (r.width * self.scale_x).max(0.0);
        let h = (r.height * self.scale_y).max(0.0);
        ResolvedRect::new(cx - w / 2.0, cy - h / 2.0, w, h)
    }
}

/// Press state of one interactive glass node, driven by the pointer
/// router (down/move/up) and integrated once per frame by the damage
/// pass — the same fixed-quantum pattern as morph tracks.
///
/// Mirrors `GlassPressController`: a spring from 0 (rest) to 1 (pressed)
/// with press-in and release tunings, plus the hold-then-fade press
/// light. One active press at a time (primary pointer); a new press
/// elsewhere ends the previous one.
#[derive(Clone, Debug)]
pub struct GlassPress {
    amount: SpringState,
    touch: Option<Point>,
    /// Touch position at the last damage advance: fast finger travel
    /// changes the deformation (directional stretch, drag elongation),
    /// so damage covers the last advance's touch as well as this one.
    last_touch: Option<Point>,
    size: (f32, f32),
    down: bool,
    hold: f32,
    reduce_motion: bool,
}

impl GlassPress {
    /// Starts a press at box-local `touch` on a `width × height` box.
    pub fn down(touch: Point, width: f32, height: f32, motion: &GlassMotionConstants) -> Self {
        let mut amount = SpringState::new(
            swiftui_spring(motion.press_response, motion.press_damping),
            0.0,
            1.0,
        );
        amount.velocity = 0.0;
        Self {
            amount,
            touch: Some(touch),
            last_touch: None,
            size: (width, height),
            down: true,
            hold: 0.0,
            reduce_motion: false,
        }
    }

    /// Retargets an existing press (fresh down on the same node).
    pub fn retarget_down(&mut self, touch: Point, width: f32, height: f32, m: &GlassMotionConstants) {
        self.amount.spring = swiftui_spring(m.press_response, m.press_damping);
        self.amount.retarget(1.0);
        self.touch = Some(touch);
        self.size = (width, height);
        self.down = true;
        self.hold = 0.0;
    }

    /// Moves the touch point while pressed.
    pub fn move_to(&mut self, touch: Point) {
        if self.down {
            self.touch = Some(touch);
        }
    }

    /// Releases: the spring heads home on the release tuning and the
    /// light holds briefly before fading.
    pub fn up(&mut self, m: &GlassMotionConstants) {
        if self.down {
            self.down = false;
            self.hold = 0.0;
        }
        self.amount.spring = swiftui_spring(m.release_response, m.release_damping);
        self.amount.retarget(0.0);
    }

    /// Advances one fixed quantum (the animation clock's step).
    pub fn advance(&mut self) {
        self.last_touch = self.touch;
        self.amount.advance(crate::anim::clock::FIXED_STEP_SECS);
        if !self.down {
            self.hold += crate::anim::clock::FIXED_STEP_SECS;
        }
    }

    /// Whether frames are still needed: finger down, spring gliding, or
    /// light hold fading.
    pub fn is_active(&self) -> bool {
        self.down || !self.amount.is_settled() || self.hold < PRESS_LIGHT_HOLD_SECS
    }

    /// SwiftUI's press light, 0..1: full from the touch, held past the
    /// release (fading by [`PRESS_LIGHT_HOLD_FADE`]), then off. Unlike
    /// `amount` it has no spring: it switches on in the frame of the
    /// touch.
    pub fn light(&self) -> f32 {
        if self.down {
            return 1.0;
        }
        if self.hold >= PRESS_LIGHT_HOLD_SECS {
            return 0.0;
        }
        1.0 - PRESS_LIGHT_HOLD_FADE * (self.hold / PRESS_LIGHT_HOLD_SECS)
    }

    /// Current deformation; [`GlassPressGeometry::IDENTITY`] at rest.
    pub fn geometry(&self, m: &GlassMotionConstants) -> GlassPressGeometry {
        self.geometry_for(m, self.touch)
    }

    /// Deformation for an explicit touch point (damage covers both the
    /// last advance's touch and the current one while the finger runs).
    pub fn geometry_for(
        &self,
        m: &GlassMotionConstants,
        touch: Option<Point>,
    ) -> GlassPressGeometry {
        let a = self.amount.position;
        let light = self.light();
        if a.abs() < 1e-4 && self.amount.is_settled() && light == 0.0 {
            return GlassPressGeometry::IDENTITY;
        }
        let glow = a.clamp(0.0, 1.0);
        let (w, h) = self.size;
        if self.reduce_motion || w <= 0.0 || h <= 0.0 {
            return GlassPressGeometry {
                glow,
                light,
                touch,
                ..GlassPressGeometry::IDENTITY
            };
        }
        let t = touch.unwrap_or(Point::new(w / 2.0, h / 2.0));
        let dx = ((t.x - w / 2.0) / (w / 2.0)).clamp(-1.0, 1.0);
        let dy = ((t.y - h / 2.0) / (h / 2.0)).clamp(-1.0, 1.0);
        let s = 1.0 + (m.press_scale_for(w, h) - 1.0) * a;
        // Dragged past an edge: extra elongation toward the finger,
        // anchored at the opposite edge. Zero while the finger is inside.
        let ex = drag_stretch(t.x - w / 2.0, w, m) * a;
        let ey = drag_stretch(t.y - h / 2.0, h, m) * a;
        GlassPressGeometry {
            scale_x: s + m.press_stretch * a * dx.abs() + ex.abs(),
            scale_y: s + m.press_stretch * a * dy.abs() + ey.abs(),
            translation: Point::new(
                dx * m.press_stretch * a * w / 4.0 + ex * w / 2.0,
                dy * m.press_stretch * a * h / 4.0 + ey * h / 2.0,
            ),
            glow,
            light,
            touch,
        }
    }

    /// Current touch in pressed-box-local px (for damage bookkeeping).
    pub fn touch(&self) -> Option<Point> {
        self.touch
    }

    /// Touch position at the last damage advance (covers fast travel).
    pub fn last_touch(&self) -> Option<Point> {
        self.last_touch
    }
}

/// Signed extra scale for a touch `offset` from the centre along an axis
/// of `extent`: zero inside, rising toward `dragStretch` with diminishing
/// returns past the edge. Mirrors the controller's `_dragStretch`.
fn drag_stretch(offset: f32, extent: f32, m: &GlassMotionConstants) -> f32 {
    let past = offset.abs() - extent / 2.0;
    if past <= 0.0 || m.drag_stretch_distance <= 0.0 {
        return 0.0;
    }
    let e = m.drag_stretch * (1.0 - (-past / m.drag_stretch_distance).exp());
    offset.signum() * e
}
