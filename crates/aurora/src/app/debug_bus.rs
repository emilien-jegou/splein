// Single responsibility: One-way telemetry bridge between the app window and
// the external debug window.
//
// The app loop publishes a plain-data `Snapshot` per presented frame; the
// debug panel consumes it. Commands flow back over a channel. No reactive
// signals, layouts, or glyph data ever cross engines — the panel cannot
// corrupt the app view by construction, and the app never blocks on the
// panel (non-blocking lock + bounded lossy semantics).

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::app::debug::InspectorMode;

/// Stable description of one inspected node for the panel.
#[derive(Clone, Debug, Default)]
pub struct NodeBrief {
    /// Key path, e.g. `"cta"` or `"root#3=…"`; empty when unknown.
    pub key_path: String,
    /// Tree kind label, e.g. `"Group"` / `"Text"`.
    pub kind: String,
    /// Absolute window rect in pixels.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Computed size line, e.g. `"1544px"`.
    pub computed_w: String,
    /// Computed size line, e.g. `"8737.52px"`.
    pub computed_h: String,
    /// Solid fill as `#rrggbb`, when the node has one.
    pub fill: Option<String>,
    /// Ancestor key paths, nearest first (for the Layers tab).
    pub ancestors: Vec<String>,
}

/// Everything the debug panel may display. Plain data only.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// Monotonic version, bumped per publish; the panel rebuilds on change.
    pub version: u64,
    /// Smoothed display fps.
    pub fps: u32,
    /// Trailing-5s average fps.
    pub fps_avg5: f32,
    /// Frame timings in milliseconds.
    pub total_ms: f32,
    /// Raster milliseconds.
    pub raster_ms: f32,
    /// Present milliseconds.
    pub present_ms: f32,
    /// Backend tag, e.g. `"tiny-skia"`.
    pub backend: String,
    /// Active inspector mode.
    pub inspector: InspectorMode,
    /// Engine time scale.
    pub time_scale: f32,
    /// Presented frame counter.
    pub frame_index: u64,
    /// Current demo id + title.
    pub demo_id: String,
    /// Current demo title.
    pub demo_title: String,
    /// Node under the app pointer, if any.
    pub hovered: Option<NodeBrief>,
    /// Picked node, if any.
    pub selected: Option<NodeBrief>,
    /// Whether app keyboard focus sits in text.
    pub focus_present: bool,
    /// Whether the app engine currently wants frames (motion/caret).
    pub motion_active: bool,
    /// Whether node picking is armed (next app click selects).
    pub pick_armed: bool,
    /// Recent debugger event log (capped ring, newest last).
    pub events: Vec<String>,
    /// Path of the last frame capture, if any.
    pub capture_last: Option<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            version: 0,
            fps: 0,
            fps_avg5: 0.0,
            total_ms: 0.0,
            raster_ms: 0.0,
            present_ms: 0.0,
            backend: String::new(),
            inspector: InspectorMode::Off,
            time_scale: 1.0,
            frame_index: 0,
            demo_id: String::new(),
            demo_title: String::new(),
            hovered: None,
            selected: None,
            focus_present: false,
            motion_active: false,
            pick_armed: false,
            events: Vec::new(),
            capture_last: None,
        }
    }
}

/// Commands from the debug panel to the app loop.
#[derive(Clone, Debug)]
pub enum DebugCommand {
    /// Set the inspector overlay mode.
    SetInspector(InspectorMode),
    /// Set the engine time scale.
    SetTimeScale(f32),
    /// Advance exactly one animation quantum while frozen.
    StepOnce,
    /// Write presented pixels + scene dump (the `s` action).
    Capture,
    /// Send the next presented frame to Tracy (the `T` action).
    TracySnapshot,
    /// Arm node picking: the next app-window click selects.
    ArmPick,
    /// Clear the picked node.
    ClearSelection,
    /// Rebuild the UI from the root factory, preserving all reactive state.
    ///
    /// Queued for the next frame; a panicking rebuild keeps the previous tree.
    ReloadAll,
    /// Reload scoped to one stable component identity (`module_path!()::Name`).
    ///
    /// Unknown or currently unmounted identities report without failing.
    ReloadComponent(String),
}

/// Shared handle: latest snapshot cell + command channel.
#[derive(Clone, Debug)]
pub struct DebugBus {
    snapshot: Arc<Mutex<Snapshot>>,
    tx: Sender<DebugCommand>,
}

impl DebugBus {
    /// Creates the bus and the app-side command receiver.
    pub fn new() -> (Self, Receiver<DebugCommand>) {
        let (tx, rx) = mpsc::channel();
        (
            Self {
                snapshot: Arc::new(Mutex::new(Snapshot::default())),
                tx,
            },
            rx,
        )
    }

    /// Publishes a snapshot, bumping the version. Never blocks the app loop
    /// longer than a mutex handoff; a poisoned lock still publishes.
    pub fn publish(&self, mut snap: Snapshot) {
        let mut guard = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        snap.version = guard.version.wrapping_add(1);
        *guard = snap;
    }

    /// Reads the latest snapshot without blocking. `None` only if another
    /// thread holds the lock at this instant (transient; retry next frame).
    pub fn latest(&self) -> Option<Snapshot> {
        self.snapshot.try_lock().ok().map(|g| g.clone())
    }

    /// Sends a command to the app loop (unbounded; panel-driven rates only).
    pub fn send(&self, cmd: DebugCommand) {
        let _ = self.tx.send(cmd);
    }

    /// Appends a debugger event to the ring (capped at 24, newest last).
    pub fn push_event(&self, event: String) {
        if let Ok(mut guard) = self.snapshot.try_lock() {
            let mut events: VecDeque<String> = guard.events.drain(..).collect();
            events.push_back(event);
            while events.len() > 24 {
                events.pop_front();
            }
            guard.events = events.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_bumps_version_and_latest_returns_clone() {
        let (bus, _rx) = DebugBus::new();
        let first = bus.latest().expect("snapshot readable");
        assert_eq!(first.version, 0);
        let mut snap = Snapshot::default();
        snap.fps = 60;
        bus.publish(snap);
        let second = bus.latest().expect("snapshot readable");
        assert_eq!(second.version, 1);
        assert_eq!(second.fps, 60);
    }

    #[test]
    fn event_ring_caps_at_24_newest_last() {
        let (bus, _rx) = DebugBus::new();
        for i in 0..30 {
            bus.push_event(format!("e{i}"));
        }
        let snap = bus.latest().expect("snapshot readable");
        assert_eq!(snap.events.len(), 24);
        assert_eq!(snap.events.first().unwrap(), "e6");
        assert_eq!(snap.events.last().unwrap(), "e29");
    }

    #[test]
    fn commands_reach_the_app_receiver() {
        let (bus, rx) = DebugBus::new();
        bus.send(DebugCommand::StepOnce);
        bus.send(DebugCommand::SetTimeScale(0.5));
        bus.send(DebugCommand::ReloadAll);
        bus.send(DebugCommand::ReloadComponent("app::Card".to_string()));
        assert!(matches!(rx.recv().unwrap(), DebugCommand::StepOnce));
        assert!(matches!(rx.recv().unwrap(), DebugCommand::SetTimeScale(_)));
        assert!(matches!(rx.recv().unwrap(), DebugCommand::ReloadAll));
        assert!(matches!(
            rx.recv().unwrap(),
            DebugCommand::ReloadComponent(_)
        ));
    }
}
