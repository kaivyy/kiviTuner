use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TypeError {
    #[error("Buffer too small: expected {expected} bytes, got {got}")]
    BufferTooSmall { expected: usize, got: usize },
    #[error("Cannot encode non-finite value {0}")]
    NonFiniteValue(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Endian {
    Big,
    Little,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DType {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
}

impl DType {
    pub fn byte_size(self) -> usize {
        match self {
            DType::U8 | DType::I8 => 1,
            DType::U16 | DType::I16 => 2,
            DType::U32 | DType::I32 | DType::F32 => 4,
        }
    }

    pub fn read_f64(self, buf: &[u8], endian: Endian) -> Result<f64, TypeError> {
        let size = self.byte_size();
        if buf.len() < size {
            return Err(TypeError::BufferTooSmall {
                expected: size,
                got: buf.len(),
            });
        }
        match (self, endian) {
            (DType::U8, _) => Ok(buf[0] as f64),
            (DType::I8, _) => Ok((buf[0] as i8) as f64),
            (DType::U16, Endian::Big) => Ok(u16::from_be_bytes([buf[0], buf[1]]) as f64),
            (DType::U16, Endian::Little) => Ok(u16::from_le_bytes([buf[0], buf[1]]) as f64),
            (DType::I16, Endian::Big) => Ok(i16::from_be_bytes([buf[0], buf[1]]) as f64),
            (DType::I16, Endian::Little) => Ok(i16::from_le_bytes([buf[0], buf[1]]) as f64),
            (DType::U32, Endian::Big) => {
                Ok(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64)
            }
            (DType::U32, Endian::Little) => {
                Ok(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64)
            }
            (DType::I32, Endian::Big) => {
                Ok(i32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64)
            }
            (DType::I32, Endian::Little) => {
                Ok(i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64)
            }
            (DType::F32, Endian::Big) => {
                Ok(f32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64)
            }
            (DType::F32, Endian::Little) => {
                Ok(f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as f64)
            }
        }
    }

    pub fn write_f64(self, val: f64, buf: &mut [u8], endian: Endian) -> Result<(), TypeError> {
        if !val.is_finite() {
            return Err(TypeError::NonFiniteValue(val));
        }
        let size = self.byte_size();
        if buf.len() < size {
            return Err(TypeError::BufferTooSmall {
                expected: size,
                got: buf.len(),
            });
        }
        match (self, endian) {
            (DType::U8, _) => buf[0] = val.round().clamp(0.0, 255.0) as u8,
            (DType::I8, _) => buf[0] = (val.round().clamp(-128.0, 127.0) as i8) as u8,
            (DType::U16, Endian::Big) => {
                buf[..2].copy_from_slice(&(val.round().clamp(0.0, 65535.0) as u16).to_be_bytes())
            }
            (DType::U16, Endian::Little) => {
                buf[..2].copy_from_slice(&(val.round().clamp(0.0, 65535.0) as u16).to_le_bytes())
            }
            (DType::I16, Endian::Big) => {
                buf[..2].copy_from_slice(&(val.round().clamp(-32768.0, 32767.0) as i16).to_be_bytes())
            }
            (DType::I16, Endian::Little) => {
                buf[..2].copy_from_slice(&(val.round().clamp(-32768.0, 32767.0) as i16).to_le_bytes())
            }
            (DType::U32, Endian::Big) => buf[..4]
                .copy_from_slice(&(val.round().clamp(0.0, u32::MAX as f64) as u32).to_be_bytes()),
            (DType::U32, Endian::Little) => buf[..4]
                .copy_from_slice(&(val.round().clamp(0.0, u32::MAX as f64) as u32).to_le_bytes()),
            (DType::I32, Endian::Big) => buf[..4].copy_from_slice(
                &(val.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32).to_be_bytes(),
            ),
            (DType::I32, Endian::Little) => buf[..4].copy_from_slice(
                &(val.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32).to_le_bytes(),
            ),
            (DType::F32, Endian::Big) => buf[..4].copy_from_slice(&(val as f32).to_be_bytes()),
            (DType::F32, Endian::Little) => buf[..4].copy_from_slice(&(val as f32).to_le_bytes()),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemRange {
    pub start: u32,
    pub length: u32,
}

impl MemRange {
    pub fn new(start: u32, length: u32) -> Self {
        Self { start, length }
    }

    pub fn end(&self) -> u32 {
        self.start.saturating_add(self.length)
    }

    pub fn contains(&self, address: u32) -> bool {
        address >= self.start && address < self.end()
    }
}
