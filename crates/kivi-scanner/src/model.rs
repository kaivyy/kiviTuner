use kivi_core::types::{DType, Endian};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PITCHES: &[usize] = &[8, 10, 12, 15, 16, 19, 20, 24, 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScannerSensitivity {
    Strict,
    #[default]
    Balanced,
    Aggressive,
}

impl ScannerSensitivity {
    pub fn min_confidence(&self) -> u8 {
        match self {
            ScannerSensitivity::Strict => 85,
            ScannerSensitivity::Balanced => 70,
            ScannerSensitivity::Aggressive => 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisCandidate {
    pub start_addr: usize,
    pub length: usize,
    pub dtype: DType,
    pub endian: Endian,
    pub min_val: f64,
    pub max_val: f64,
    pub step_variance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawTableCandidate {
    pub start_addr: usize,
    pub end_addr: usize,
    pub rows: usize,
    pub cols: usize,
    pub dtype: DType,
    pub endian: Endian,
    pub avg_similarity: f64,
    pub min_val: f64,
    pub max_val: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedMapCandidate {
    pub id: String,
    pub name_hint: String,
    pub category_hint: String,
    pub start_addr: usize,
    pub end_addr: usize,
    pub rows: usize,
    pub cols: usize,
    pub dtype: DType,
    pub endian: Endian,
    pub confidence: u8,
    pub x_axis_addr: Option<usize>,
    pub y_axis_addr: Option<usize>,
}
