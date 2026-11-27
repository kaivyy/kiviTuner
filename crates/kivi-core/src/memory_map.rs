use crate::hash::{compute_hashes, BinaryHashes};
use crate::types::{DType, Endian, TypeError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum MemoryError {
    #[error("Address 0x{addr:X} out of bounds (size 0x{size:X})")]
    OutOfBounds { addr: usize, size: usize },
    #[error("Type conversion error: {0}")]
    Type(#[from] TypeError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMap {
    data: Vec<u8>,
    hashes: BinaryHashes,
    /// Lowest logical address represented by this image, when known.
    ///
    /// * `Some(0)` — the image is known to start at address zero.
    /// * `Some(n)` — the image is known to start at address `n`.
    /// * `None` — the address is **unknown**.
    ///
    /// `None` must never be silently converted to `Some(0)`. A raw `.bin` has
    /// no inherent absolute address, so it carries `None` until a caller states
    /// otherwise. Serialized data written before this field existed
    /// deserializes as `None` via `serde(default)`.
    #[serde(default)]
    base_address: Option<u32>,
}

impl MemoryMap {
    pub fn new(data: Vec<u8>) -> Self {
        let hashes = compute_hashes(&data);
        Self {
            data,
            hashes,
            base_address: None,
        }
    }

    /// Builds a map that carries an explicitly supplied base address.
    ///
    /// Passing `None` is equivalent to [`MemoryMap::new`]; passing `Some(0)`
    /// records a *known* zero address, which is not the same as unknown.
    pub fn with_base(data: Vec<u8>, base_address: Option<u32>) -> Self {
        let hashes = compute_hashes(&data);
        Self {
            data,
            hashes,
            base_address,
        }
    }

    /// The base address, or `None` when it is unknown.
    pub fn base_address(&self) -> Option<u32> {
        self.base_address
    }

    /// Sets or clears the base address without touching the data or hashes.
    ///
    /// Used when the user states an address for an already-loaded image.
    pub fn set_base_address(&mut self, base_address: Option<u32>) {
        self.base_address = base_address;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn hashes(&self) -> &BinaryHashes {
        &self.hashes
    }

    pub fn read_value(
        &self,
        addr: usize,
        dtype: DType,
        endian: Endian,
    ) -> Result<f64, MemoryError> {
        let size = dtype.byte_size();
        if addr
            .checked_add(size)
            .is_none_or(|end| end > self.data.len())
        {
            return Err(MemoryError::OutOfBounds {
                addr,
                size: self.data.len(),
            });
        }
        dtype
            .read_f64(&self.data[addr..addr + size], endian)
            .map_err(Into::into)
    }

    pub fn recompute_hashes(&mut self) -> &BinaryHashes {
        self.hashes = compute_hashes(&self.data);
        &self.hashes
    }

    pub fn write_value_deferred_hash(
        &mut self,
        addr: usize,
        val: f64,
        dtype: DType,
        endian: Endian,
    ) -> Result<(usize, Vec<u8>, Vec<u8>), MemoryError> {
        let size = dtype.byte_size();
        if addr
            .checked_add(size)
            .is_none_or(|end| end > self.data.len())
        {
            return Err(MemoryError::OutOfBounds {
                addr,
                size: self.data.len(),
            });
        }
        let old_bytes = self.data[addr..addr + size].to_vec();
        dtype.write_f64(val, &mut self.data[addr..addr + size], endian)?;
        let new_bytes = self.data[addr..addr + size].to_vec();
        Ok((addr, old_bytes, new_bytes))
    }

    pub fn write_slice_deferred_hash(
        &mut self,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), MemoryError> {
        if bytes.is_empty() {
            if offset > self.data.len() {
                return Err(MemoryError::OutOfBounds {
                    addr: offset,
                    size: self.data.len(),
                });
            }
            return Ok((Vec::new(), Vec::new()));
        }
        if offset
            .checked_add(bytes.len())
            .is_none_or(|end| end > self.data.len())
        {
            return Err(MemoryError::OutOfBounds {
                addr: offset,
                size: self.data.len(),
            });
        }
        let end = offset + bytes.len();
        let old_bytes = self.data[offset..end].to_vec();
        self.data[offset..end].copy_from_slice(bytes);
        Ok((old_bytes, bytes.to_vec()))
    }

    pub fn write_value(
        &mut self,
        addr: usize,
        val: f64,
        dtype: DType,
        endian: Endian,
    ) -> Result<(), MemoryError> {
        self.write_value_deferred_hash(addr, val, dtype, endian)?;
        self.recompute_hashes();
        Ok(())
    }

    pub fn search_pattern(
        &self,
        pattern: &[u8],
        start_offset: usize,
        max_hits: usize,
    ) -> Vec<usize> {
        if pattern.is_empty()
            || max_hits == 0
            || start_offset
                .checked_add(pattern.len())
                .is_none_or(|end| end > self.data.len())
        {
            return Vec::new();
        }
        let mut hits = Vec::new();
        let mut pos = start_offset;
        while let Some(rel) = self.data[pos..].windows(pattern.len()).position(|w| w == pattern) {
            let abs = pos + rel;
            hits.push(abs);
            if hits.len() >= max_hits {
                break;
            }
            pos = abs + 1;
        }
        hits
    }

    pub fn get_window(&self, offset: usize, length: usize) -> &[u8] {
        if offset >= self.data.len() {
            &[]
        } else {
            let end = offset.saturating_add(length).min(self.data.len());
            &self.data[offset..end]
        }
    }
}
