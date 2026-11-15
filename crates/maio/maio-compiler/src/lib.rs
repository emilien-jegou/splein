// Responsibility: Generic parallel batch execution of .maiodb tasks via registered task processors.

mod artifact;
mod report;

pub use artifact::{AlphaKind, Artifact, ArtifactError, ArtifactHeader, ArtifactKind, FORMAT_VERSION, MAX_TEXTURE_EDGE};
pub use report::{CompileReport, Compiled, Failure};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use blake3::Hasher;
use maio::{ArtifactMeta, Maiodb};
use maio_core::{AssetId, AssetSource, AssetTask};
use rayon::prelude::*;
use thiserror::Error;

/// Output produced by a domain processor.
#[derive(Debug, Clone)]
pub struct ProcessedOutput {
    pub width: u32,
    pub height: u32,
    pub payload: Vec<u8>,
}

pub trait TaskProcessor: Send + Sync {
    fn process(&self, task: &AssetTask, source_bytes: &[u8]) -> Result<ProcessedOutput, ProcessorError>;
}

#[derive(Debug, Error)]
pub enum ProcessorError {
    #[error("task execution failed: {0}")]
    Execution(String),
    #[error("unsupported task format: {0}")]
    Format(String),
}

#[derive(Default, Clone)]
pub struct ProcessorRegistry {
    processors: Arc<HashMap<String, Arc<dyn TaskProcessor>>>,
}

impl ProcessorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, kind: impl Into<String>, processor: impl TaskProcessor + 'static) {
        let map = Arc::make_mut(&mut self.processors);
        map.insert(kind.into(), Arc::new(processor));
    }

    pub fn get(&self, kind: &str) -> Option<Arc<dyn TaskProcessor>> {
        self.processors.get(kind).cloned()
    }
}

#[derive(Clone)]
pub struct Compiler {
    out_dir: PathBuf,
    jobs: Option<usize>,
    registry: ProcessorRegistry,
}

impl Compiler {
    pub fn new(out_dir: PathBuf, registry: ProcessorRegistry) -> Self {
        Self { out_dir, jobs: None, registry }
    }

    pub fn set_jobs(&mut self, jobs: usize) {
        self.jobs = Some(jobs);
    }

    pub fn out_dir(&self) -> &Path {
        &self.out_dir
    }

    pub fn compile(&self, tasks: &[(AssetId, AssetTask)]) -> CompileReport {
        let results: Vec<Result<Compiled, (String, String)>> = if let Some(jobs) = self.jobs {
            rayon::ThreadPoolBuilder::new()
                .num_threads(jobs)
                .build()
                .unwrap_or_else(|e| panic!("failed to build rayon pool: {e}"))
                .install(|| self.par_run(tasks))
        } else {
            self.par_run(tasks)
        };

        let mut report = CompileReport::default();
        for ((id, _task), result) in tasks.iter().zip(results) {
            match result {
                Ok(compiled) => report.compiled.push(compiled),
                Err((kind, error)) => report.failed.push(Failure { id: *id, kind: Box::leak(kind.into_boxed_str()), error }),
            }
        }
        report
    }

    fn par_run(&self, tasks: &[(AssetId, AssetTask)]) -> Vec<Result<Compiled, (String, String)>> {
        tasks
            .par_iter()
            .map(|(id, task)| self.process_task(id, task).map_err(|e| (task.kind.clone(), e.to_string())))
            .collect()
    }

    #[tracing::instrument(skip_all, fields(asset_id = %id, kind = %task.kind))]
    fn process_task(&self, id: &AssetId, task: &AssetTask) -> Result<Compiled, CompilerError> {
        let started = Instant::now();
        let source_bytes = match &task.source {
            AssetSource::Path { path, .. } => std::fs::read(path)
                .map_err(|e| CompilerError::Source(format!("{}: {e}", path.display())))?,
            AssetSource::Inline(bytes) => bytes.clone(),
        };

        let mut hasher = Hasher::new();
        hasher.update(&source_bytes);
        let mut source_hash = [0u8; 16];
        source_hash.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
        let engine_hash = *artifact::engine_hash();

        if let Ok(existing) = artifact::load(&self.out_dir, id) {
            if existing.header.source_hash == source_hash && existing.header.engine_hash == engine_hash {
                return Ok(Compiled {
                    id: *id,
                    kind: Box::leak(task.kind.clone().into_boxed_str()),
                    size: existing.header.size_on_disk(),
                    path: artifact::path_for(&self.out_dir, id),
                    elapsed: started.elapsed(),
                    source_hash,
                });
            }
        }

        let processor = self.registry.get(&task.kind)
            .ok_or_else(|| CompilerError::Unsupported(task.kind.clone()))?;

        let output = processor.process(task, &source_bytes)?;
        let artifact = Artifact {
            header: ArtifactHeader {
                kind: ArtifactKind::Image,
                alpha: AlphaKind::Premultiplied,
                width: output.width,
                height: output.height,
                source_hash,
                engine_hash,
            },
            pixels: output.payload,
        };

        let size = artifact::write(&self.out_dir, id, &artifact)?;
        Ok(Compiled {
            id: *id,
            kind: Box::leak(task.kind.clone().into_boxed_str()),
            path: artifact::path_for(&self.out_dir, id),
            size,
            elapsed: started.elapsed(),
            source_hash,
        })
    }
}

#[derive(Debug, Error)]
pub enum CompilerError {
    #[error("source error: {0}")]
    Source(String),
    #[error(transparent)]
    Processor(#[from] ProcessorError),
    #[error("unsupported task kind: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn compile_and_commit(
    db: &Maiodb,
    compiler: &Compiler,
    tasks: &[(AssetId, AssetTask)],
) -> maio::Result<CompileReport> {
    let report = compiler.compile(tasks);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    for fail in &report.failed {
        tracing::error!(id = %fail.id.short_hex(), error = %fail.error, "task compilation failed");
    }

    for compiled in &report.compiled {
        db.complete(
            compiled.id,
            &ArtifactMeta {
                source_hash: compiled.source_hash,
                size: compiled.size,
                completed_at_ms: now,
            },
        )?;
    }
    Ok(report)
}

