use kivi_core::types::{DType, Endian};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DiffKind {
    Scalar,
    Axis1D {
        length: usize,
    },
    Table2D {
        suggested_rows: usize,
        suggested_cols: usize,
    },
    ChecksumOrRawBlock,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffRegion {
    pub id: String,
    pub start_addr: usize,
    pub end_addr: usize,
    pub byte_len: usize,
    pub changed_byte_count: usize,
    pub max_abs_delta: f64,
    pub mean_delta: f64,
    pub inferred_dtype: DType,
    pub inferred_endian: Endian,
    pub classification: DiffKind,
}
