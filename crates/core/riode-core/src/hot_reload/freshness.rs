// Responsibility: Performs filesystem mtime scans and computes pure source freshness decisions.

use std::path::Path;
use std::time::SystemTime;

/// Decision outcome for source freshness checks.
#[derive(Debug, PartialEq, Eq)]
pub enum FreshDecision {
    Build,
    Quiet,
}

/// Evaluates whether source mtime warrants triggering a build.
pub fn decide_freshness(
    max_src: SystemTime,
    last_trigger: Option<SystemTime>,
    loaded: Option<SystemTime>,
) -> (FreshDecision, Option<SystemTime>) {
    if last_trigger.is_some_and(|t| max_src <= t) {
        return (FreshDecision::Quiet, last_trigger);
    }
    let baseline = Some(max_src);
    let stale = loaded.is_some_and(|l| max_src > l);
    let decision = if stale {
        FreshDecision::Build
    } else {
        FreshDecision::Quiet
    };
    (decision, baseline)
}

/// Finds the newest modification time under a directory recursively.
pub fn max_mtime_under(dir: &Path) -> Option<SystemTime> {
    let mut max = None;
    walk(dir, &mut max);
    max
}

fn walk(dir: &Path, max: &mut Option<SystemTime>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, max);
        } else if let Ok(mtime) = std::fs::metadata(&path).and_then(|m| m.modified()) {
            *max = Some(max.map_or(mtime, |t: SystemTime| t.max(mtime)));
        }
    }
}
