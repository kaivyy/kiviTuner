use kivi_core::types::{DType, Endian};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scaling {
    pub equation: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "default_decimals")]
    pub decimals: u8,
}

fn default_decimals() -> u8 {
    2
}

impl Default for Scaling {
    fn default() -> Self {
        Self {
            equation: "X".to_string(),
            unit: String::new(),
            decimals: 2,
        }
    }
}

impl Scaling {
    pub fn raw(dtype: DType) -> Self {
        let decimals = match dtype {
            DType::F32 => 2,
            _ => 0,
        };
        Self {
            equation: "X".to_string(),
            unit: String::new(),
            decimals,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisDef {
    pub address: usize,
    pub length: usize,
    pub dtype: DType,
    pub endian: Endian,
    #[serde(default)]
    pub scaling: Scaling,
    #[serde(default)]
    pub values: Vec<f64>,
}

impl Default for AxisDef {
    fn default() -> Self {
        Self {
            address: 0,
            length: 0,
            dtype: DType::U8,
            endian: Endian::Big,
            scaling: Scaling::default(),
            values: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub address: usize,
    pub rows: usize,
    pub cols: usize,
    pub dtype: DType,
    pub endian: Endian,
    #[serde(default)]
    pub column_major: bool,
    /// Row-to-row distance in bits. `0` means packed (`cols * dtype` bits).
    ///
    /// This is XDF's `mmedmajorstridebits`. A non-zero value that differs
    /// from the packed size means the table is padded; reading it contiguously
    /// returns cells from the wrong row.
    #[serde(default)]
    pub major_stride_bits: i32,
    /// Column-to-column distance in bits. `0` means packed.
    #[serde(default)]
    pub minor_stride_bits: i32,
    #[serde(default)]
    pub scaling: Scaling,
    #[serde(default)]
    pub x_axis: Option<AxisDef>,
    #[serde(default)]
    pub y_axis: Option<AxisDef>,
}

fn default_category() -> String {
    "Other".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstantDef {
    pub id: String,
    pub name: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub address: usize,
    pub dtype: DType,
    pub endian: Endian,
    #[serde(default)]
    pub scaling: Scaling,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlagDef {
    pub id: String,
    pub name: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub address: usize,
    pub dtype: DType,
    pub mask: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchEntryDef {
    pub name: String,
    pub address: usize,
    pub patch_data: Vec<u8>,
    pub base_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchDef {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub entries: Vec<PatchEntryDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XdfChecksumDef {
    pub id: String,
    pub name: String,
    pub start_address: usize,
    pub end_address: usize,
    pub store_address: usize,
    pub data_bits: u8,
    pub sum_bits: u8,
    pub little_endian: bool,
    pub calculation_method: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Definitions {
    #[serde(default)]
    pub maps: Vec<MapDef>,
    #[serde(default)]
    pub constants: Vec<ConstantDef>,
    #[serde(default)]
    pub flags: Vec<FlagDef>,
    #[serde(default)]
    pub patches: Vec<PatchDef>,
    #[serde(default)]
    pub checksums: Vec<XdfChecksumDef>,
}
