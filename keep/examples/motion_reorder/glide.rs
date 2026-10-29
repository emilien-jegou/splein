// Single responsibility: Summing row offsets into the one transform a row draws with.
//
// (The release-glide ticker that used to live here is gone: releases fold
// into the flip spring itself, so there is no second motion to drive.)

use riode::foundation::Transform;
use riode::prelude::*;

/// Sums two vertical offsets into the one transform a row is drawn with.
///
/// Both animations write a y translation and neither writes an x, so this is a sum rather than a
/// matrix product. Said out loud because a general compose would be wrong here in a way that
/// only shows up as a diagonal drift.
pub fn offset(
    flip: impl SignalRead<Transform> + 'static,
    follow: impl SignalRead<Transform> + 'static,
) -> Computed<Transform> {
    computed!(move || {
        let a = flip.get();
        let b = follow.get();
        Transform::from_translation(a.tx + b.tx, a.ty + b.ty)
    })
}
