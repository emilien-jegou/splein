// Responsibility: Wraps standard OS child processes into the BuildProcess abstraction.

use super::{BuildProcess, BuildStatus};
use std::process::Child;
use tracing::instrument;

/// Standard OS process implementation of a build handle.
pub struct SubprocessHandle {
    child: Child,
}

impl SubprocessHandle {
    pub fn new(child: Child) -> Self {
        Self { child }
    }
}

impl BuildProcess for SubprocessHandle {
    #[instrument(skip_all, fields(pid = self.child.id()))]
    fn poll_status(&mut self) -> std::io::Result<BuildStatus> {
        match self.child.try_wait()? {
            None => Ok(BuildStatus::Running),
            Some(status) if status.success() => Ok(BuildStatus::Success),
            Some(status) => Ok(BuildStatus::Failed {
                code: status.code(),
            }),
        }
    }

    #[instrument(skip_all, fields(pid = self.child.id()))]
    fn cancel(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }
}
