// Single responsibility: One-way OS clipboard push for readonly selections.
//
// Copy-out only: selections are readonly, so there is no paste path to feed.
// The runner calls `sync` after each presented frame; headless engines never
// construct this, and every OS interaction is fail-soft (no display server,
// compositor denying data-control, …) — failures only skip the push.

use crate::runtime::Engine;

/// Pushes the engine's internal clipboard to the OS clipboard on change.
pub struct OsClipboardSync {
    /// Last engine text observed: bounds OS attempts to actual changes, so a
    /// denied write retries on the next new selection rather than hot-looping.
    last_seen: Option<String>,
    #[cfg(feature = "os-clipboard")]
    clipboard: Option<arboard::Clipboard>,
}

impl OsClipboardSync {
    /// Creates an idle sync with no OS connection yet (lazy on first change).
    pub fn new() -> Self {
        Self {
            last_seen: None,
            #[cfg(feature = "os-clipboard")]
            clipboard: None,
        }
    }

    /// Pushes `engine.clipboard_text()` to the OS when it changed since the
    /// last call. Never panics; failures are swallowed by design.
    pub fn sync(&mut self, engine: &Engine) {
        let current = engine.clipboard_text();
        if current == self.last_seen {
            return;
        }
        self.last_seen = current.clone();
        let Some(text) = current else { return };
        if text.is_empty() {
            return;
        }
        #[cfg(feature = "os-clipboard")]
        {
            if self.clipboard.is_none() {
                self.clipboard = arboard::Clipboard::new().ok();
            }
            if let Some(clipboard) = self.clipboard.as_mut() {
                // Errors (denied serial, lost focus, …) are transient far more
                // often than fatal, so the handle is kept and the next new
                // selection retries.
                let _ = clipboard.set_text(text);
            }
        }
    }
}

impl Default for OsClipboardSync {
    fn default() -> Self {
        Self::new()
    }
}
