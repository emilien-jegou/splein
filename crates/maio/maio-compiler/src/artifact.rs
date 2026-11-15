// Single responsibility: Binary format layout, serialization, and disk I/O for baked asset artifacts.

use maio_core::{AssetId, MAIO_VERSION};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_TEXTURE_EDGE: u32 = 2048;
pub const HEADER_SIZE: usize = 64;
const MAGIC: &[u8; 4] = b"MAIO";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ArtifactKind {
    Image = 0,
    Shader = 1,
    Font = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AlphaKind {
    Premultiplied = 0,
    Straight = 1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactHeader {
    pub kind: ArtifactKind,
    pub alpha: AlphaKind,
    pub width: u32,
    pub height: u32,
    pub source_hash: [u8; 16],
    pub engine_hash: [u8; 16],
}

impl ArtifactHeader {
    pub fn size_on_disk(&self) -> u64 {
        HEADER_SIZE as u64 + (self.width as u64 * self.height as u64 * 4)
    }

    pub fn encode(&self, payload_len: u64) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[0..4].copy_from_slice(MAGIC);
        out[4..8].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        out[8] = self.kind as u8;
        out[9] = self.alpha as u8;
        // bytes 10..16 reserved
        out[16..20].copy_from_slice(&self.width.to_le_bytes());
        out[20..24].copy_from_slice(&self.height.to_le_bytes());
        out[24..40].copy_from_slice(&self.source_hash);
        out[40..56].copy_from_slice(&self.engine_hash);
        out[56..64].copy_from_slice(&payload_len.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<(Self, u64), ArtifactError> {
        if bytes.len() < HEADER_SIZE {
            return Err(ArtifactError::Corrupt("Header too short".into()));
        }
        if &bytes[0..4] != MAGIC {
            return Err(ArtifactError::Corrupt("Invalid magic bytes".into()));
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(ArtifactError::VersionMismatch { expected: FORMAT_VERSION, got: version });
        }
        let kind = match bytes[8] {
            0 => ArtifactKind::Image,
            1 => ArtifactKind::Shader,
            2 => ArtifactKind::Font,
            other => return Err(ArtifactError::Corrupt(format!("Unknown artifact kind: {other}"))),
        };
        let alpha = match bytes[9] {
            0 => AlphaKind::Premultiplied,
            _ => AlphaKind::Straight,
        };
        let width = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let height = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let mut source_hash = [0u8; 16];
        source_hash.copy_from_slice(&bytes[24..40]);
        let mut engine_hash = [0u8; 16];
        engine_hash.copy_from_slice(&bytes[40..56]);
        let payload_len = u64::from_le_bytes(bytes[56..64].try_into().unwrap());

        Ok((Self { kind, alpha, width, height, source_hash, engine_hash }, payload_len))
    }
}

pub struct Artifact {
    pub header: ArtifactHeader,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("artifact format corruption: {0}")]
    Corrupt(String),
    #[error("format version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u32, got: u32 },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn engine_hash() -> &'static [u8; 16] {
    static HASH: std::sync::OnceLock<[u8; 16]> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = [0u8; 16];
        h.copy_from_slice(&blake3::hash(MAIO_VERSION.as_bytes()).as_bytes()[..16]);
        h
    })
}

pub fn path_for(out_dir: &Path, id: &AssetId) -> PathBuf {
    out_dir.join(format!("{}.bin", id.as_hex()))
}

pub fn write(out_dir: &Path, id: &AssetId, artifact: &Artifact) -> Result<u64, ArtifactError> {
    std::fs::create_dir_all(out_dir)?;
    let path = path_for(out_dir, id);
    let header_bytes = artifact.header.encode(artifact.pixels.len() as u64);

    let mut buf = Vec::with_capacity(HEADER_SIZE + artifact.pixels.len());
    buf.extend_from_slice(&header_bytes);
    buf.extend_from_slice(&artifact.pixels);

    std::fs::write(&path, buf)?;
    Ok((HEADER_SIZE + artifact.pixels.len()) as u64)
}

pub fn load(out_dir: &Path, id: &AssetId) -> Result<Artifact, ArtifactError> {
    let path = path_for(out_dir, id);
    let bytes = std::fs::read(path)?;
    let (header, payload_len) = ArtifactHeader::decode(&bytes)?;
    let pixels = bytes[HEADER_SIZE..HEADER_SIZE + payload_len as usize].to_vec();
    Ok(Artifact { header, pixels })
}
