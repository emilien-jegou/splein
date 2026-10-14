// Single responsibility: Release-glide animation (drop → settle into slot).
//
// A released row keeps the transform it had under the pointer and eases it
// to zero on an ease-out cubic, then retires. Self-retiring is the point:
// unlike the follow ticker, which lives as long as the row, this only asks
// for frames while a glide is actually in flight, so a list at rest costs
// nothing.

use riode::foundation::Transform;
use riode::prelude::*;
use riode::reactive::Ticker;

/// Seconds a released row takes to ease into its slot.
const GLIDE_SECS: f32 = 0.22;

/// Eases a release offset down to zero on an ease-out cubic, then retires.
///
/// A fresh grab disarms it by clearing `settling` (see `gesture`), and the next
/// advance then retires without touching the offset the new hold just claimed.
///
/// The glide ends when the timer is done *and* the raw FLIP is at rest — whichever comes
/// later. Ending on the timer alone reopens the FLIP gate while the spring is still
/// decaying from the drag's last reorder, and the residual snaps in as a visible jitter
/// exactly when the glide looks finished. The FLIP snaps to exactly zero on settle, so an
/// exact comparison is the right handoff test.
pub struct GlideTicker {
    pub out: Signal<Transform>,
    pub settling: Signal<bool>,
    pub flip: Signal<Transform>,
    pub from: Transform,
    pub t: f32,
}

impl Ticker for GlideTicker {
    fn advance(&mut self, dt: f32) -> bool {
        // Cancelled by a fresh grab, or already finished: retire without writing, so the
        // offset the new hold owns is never clobbered a frame late.
        if !self.settling.get() {
            return false;
        }
        self.t += dt;
        let k = (self.t / GLIDE_SECS).min(1.0);
        let ease = 1.0 - (1.0 - k).powi(3);
        let rest = 1.0 - ease;
        self.out.set(Transform::from_translation(
            self.from.tx * rest,
            self.from.ty * rest,
        ));
        if k >= 1.0 && self.flip.get() == Transform::IDENTITY {
            self.settling.set(false);
            return false;
        }
        true
    }
}

/// Sums two vertical offsets into the one transform a row is drawn with.
///
/// Both animations write a y translation and neither writes an x, so this is a sum rather than a
/// matrix product. Said out loud because a general compose would be wrong here in a way that
/// only shows up as a diagonal drift.
pub fn offset(
    flip: impl SignalRead<Transform> + 'static,
    follow: impl SignalRead<Transform> + 'static,
) -> Derived<Transform> {
    derived!(move || {
        let a = flip.get();
        let b = follow.get();
        Transform::from_translation(a.tx + b.tx, a.ty + b.ty)
    })
}
