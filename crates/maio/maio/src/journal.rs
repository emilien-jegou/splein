// Responsibility: Journal record types and framed append/replay binary codec.

use serde::{Deserialize, Serialize};
use std::io::{Seek, SeekFrom, Write};

use maio_core::{AssetId, AssetTask};

use crate::{MaioError, Result, TornTail};

/// Metadata recorded in the journal when the compiler finishes a task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactMeta {
    /// BLAKE3-16 of the source bytes when the artifact was baked.
    pub source_hash: [u8; 16],
    /// Size of the baked artifact file in bytes.
    pub size: u64,
    /// Wall-clock completion time, unix milliseconds.
    pub completed_at_ms: u64,
}

/// One journal record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Record {
    /// A task was registered for future compilation.
    Register {
        id: AssetId,
        task: AssetTask,
    },
    /// A task was compiled and its artifact written.
    Complete {
        id: AssetId,
        meta: ArtifactMeta,
    },
}

const FRAME_HEADER: usize = 8;
const MAX_FRAME: usize = 64 * 1024 * 1024;

/// Append one record to the journal (file positioned anywhere, lock held).
pub(crate) fn append(file: &mut std::fs::File, record: &Record, sync: bool) -> Result<u64> {
    let payload = postcard::to_stdvec(record)?;
    let mut frame = Vec::with_capacity(FRAME_HEADER + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&crc32fast::hash(&payload).to_le_bytes());
    frame.extend_from_slice(&payload);

    let offset = file.seek(SeekFrom::End(0))
        .map_err(|e| MaioError::Io { context: "journal seek end", source: e })?;
    file.write_all(&frame)
        .map_err(|e| MaioError::Io { context: "journal append", source: e })?;
    if sync {
        file.sync_data()
            .map_err(|e| MaioError::Io { context: "journal sync", source: e })?;
    }
    Ok(offset)
}

/// Replay outcome: reconstructed records plus where the valid stream ended.
#[derive(Debug, Default)]
pub(crate) struct Replayed {
    pub records: Vec<Record>,
    pub len: u64,
    pub torn: Option<TornTail>,
}

/// Read every valid frame from `path`, stopping at the first torn/corrupt one.
pub(crate) fn replay(path: &std::path::Path) -> Result<Replayed> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Replayed::default()),
        Err(e) => return Err(MaioError::Io { context: "journal read", source: e }),
    };

    let mut out = Replayed::default();
    let mut pos: usize = 0;

    while pos + FRAME_HEADER <= bytes.len() {
        let len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
        let frame_end = pos + FRAME_HEADER + len;
        if len > MAX_FRAME || frame_end > bytes.len() || crc32fast::hash(&bytes[pos + FRAME_HEADER..frame_end]) != crc {
            out.torn = Some(TornTail { offset: pos as u64 });
            break;
        }
        match postcard::from_bytes::<Record>(&bytes[pos + FRAME_HEADER..frame_end]) {
            Ok(record) => out.records.push(record),
            Err(_) => {
                out.torn = Some(TornTail { offset: pos as u64 });
                break;
            }
        }
        pos = frame_end;
    }
    if out.torn.is_none() && pos != bytes.len() {
        out.torn = Some(TornTail { offset: pos as u64 });
    }
    out.len = out.torn.map_or(bytes.len() as u64, |t| t.offset);
    Ok(out)
}

/// Truncate the journal back to the last good frame (torn-tail recovery).
pub(crate) fn truncate(path: &std::path::Path, good_len: u64) -> Result<()> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| MaioError::Io { context: "journal open for truncate", source: e })?;
    file.set_len(good_len)
        .map_err(|e| MaioError::Io { context: "journal truncate", source: e })?;
    file.sync_data()
        .map_err(|e| MaioError::Io { context: "journal sync after truncate", source: e })
}
