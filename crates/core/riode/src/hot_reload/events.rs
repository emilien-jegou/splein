// Responsibility: Defines declarative events and state transitions emitted by the dev supervisor.

/// Observable lifecycle events emitted during supervisor polling.
#[derive(Debug, PartialEq, Eq)]
pub enum SupervisorEvent {
    /// A fresh dynamic library is verified and ready for symbol extraction.
    LibraryReady,
    /// Compilation settled without producing changes.
    BuildSettled,
    /// Compilation is actively running.
    Building,
    /// Compilation terminated with an error.
    BuildFailed { cause: String, log: Vec<String> },
    /// No file modifications or builds are pending.
    Idle,
}

/// Operational state of the hot dev supervisor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SupervisorState {
    #[default]
    Watching,
    Stale,
    Building,
    Failed,
}
