// Single responsibility: Playback lifecycle of a motion controller.

/// Playback lifecycle of a motion controller.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Playback {
    /// Registered but not advancing.
    Idle,
    /// Advancing on every frame.
    Playing,
    /// Holding its current value while retaining progress.
    Paused,
    /// Settled, cancelled, or orphaned by an unmount.
    Finished,
}
