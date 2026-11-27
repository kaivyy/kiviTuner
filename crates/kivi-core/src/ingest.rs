use crate::memory_map::MemoryMap;
use thiserror::Error;

pub const MAX_BINARY_SIZE: usize = 64 * 1024 * 1024; // 64 MB

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IngestError {
    #[error("File is empty (0 bytes)")]
    EmptyFile,
    #[error("File too large (exceeds 64 MB)")]
    FileTooLarge,
}

pub fn ingest_binary(raw_bytes: &[u8]) -> Result<MemoryMap, IngestError> {
    if raw_bytes.is_empty() {
        return Err(IngestError::EmptyFile);
    }
    if raw_bytes.len() > MAX_BINARY_SIZE {
        return Err(IngestError::FileTooLarge);
    }
    Ok(MemoryMap::new(raw_bytes.to_vec()))
}
