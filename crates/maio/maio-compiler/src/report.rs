// Responsibility: Data models for compilation outcomes, failure tracking, and summary reporting.

use maio_core::AssetId;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Default)]
pub struct CompileReport {
    pub compiled: Vec<Compiled>,
    pub fresh: Vec<AssetId>,
    pub failed: Vec<Failure>,
}

impl CompileReport {
    pub fn is_ok(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} compiled, {} fresh, {} failed",
            self.compiled.len(),
            self.fresh.len(),
            self.failed.len()
        )
    }
}

#[derive(Debug)]
pub struct Compiled {
    pub id: AssetId,
    pub kind: &'static str,
    pub path: PathBuf,
    pub size: u64,
    pub elapsed: Duration,
    pub source_hash: [u8; 16],
}

#[derive(Debug)]
pub struct Failure {
    pub id: AssetId,
    pub kind: &'static str,
    pub error: String,
}
