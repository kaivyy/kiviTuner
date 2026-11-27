//! IEEE-754 floating-point map decoding and encoding.
//!
//! Ported from `ecu_engine_rust::math::float_map` (`FloatMapEngine`).
//! Targets modern ECUs using single- or double-precision calibration maps
//! (Bosch MD1/MG1, Continental SID, Siemens Simos 18).
//!
//! ## Deviation from the source
//!
//! `write_float_matrix` mutates the caller's buffer. This crate is read-only by
//! convention, so it is exposed as [`write_float_matrix_into`], returning a new
//! buffer. Byte layout is identical.
//!
//! ## Preserved source behaviour
//!
//! * Matrices are row-major and contiguous from `start_offset`.
//! * `f32` conversion rounds; `f64` round-trips bit-exactly.
//! * An empty matrix is a no-op on write.
//! * Both directions reject a matrix that would exceed the buffer.

/// Floating-point layout of a calibration map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatMapFormat {
    /// 32-bit, little endian (Intel / ARM / TriCore).
    Float32LittleEndian,
    /// 32-bit, big endian (Motorola / PowerPC MPC5xx).
    Float32BigEndian,
    /// 64-bit, little endian.
    Float64LittleEndian,
    /// 64-bit, big endian.
    Float64BigEndian,
}

impl FloatMapFormat {
    /// Bytes per element for this format.
    pub fn bytes_per_element(self) -> usize {
        match self {
            FloatMapFormat::Float32LittleEndian | FloatMapFormat::Float32BigEndian => 4,
            FloatMapFormat::Float64LittleEndian | FloatMapFormat::Float64BigEndian => 8,
        }
    }
}

/// Reads a row-major matrix of floating-point values from ROM bytes.
pub fn read_float_matrix(
    rom: &[u8],
    start_offset: usize,
    rows: usize,
    cols: usize,
    format: FloatMapFormat,
) -> Result<Vec<Vec<f64>>, &'static str> {
    let bytes_per_element = format.bytes_per_element();

    let total_bytes = rows
        .checked_mul(cols)
        .and_then(|c| c.checked_mul(bytes_per_element))
        .ok_or("Float matrix size overflows usize")?;
    if start_offset
        .checked_add(total_bytes)
        .is_none_or(|end| end > rom.len())
    {
        return Err("Float matrix exceeds ROM buffer bounds");
    }

    let mut matrix = Vec::with_capacity(rows);
    let mut curr_offset = start_offset;

    for _ in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for _ in 0..cols {
            let val = match format {
                FloatMapFormat::Float32LittleEndian => {
                    let b = [
                        rom[curr_offset],
                        rom[curr_offset + 1],
                        rom[curr_offset + 2],
                        rom[curr_offset + 3],
                    ];
                    f32::from_le_bytes(b) as f64
                }
                FloatMapFormat::Float32BigEndian => {
                    let b = [
                        rom[curr_offset],
                        rom[curr_offset + 1],
                        rom[curr_offset + 2],
                        rom[curr_offset + 3],
                    ];
                    f32::from_be_bytes(b) as f64
                }
                FloatMapFormat::Float64LittleEndian => {
                    let b = [
                        rom[curr_offset],
                        rom[curr_offset + 1],
                        rom[curr_offset + 2],
                        rom[curr_offset + 3],
                        rom[curr_offset + 4],
                        rom[curr_offset + 5],
                        rom[curr_offset + 6],
                        rom[curr_offset + 7],
                    ];
                    f64::from_le_bytes(b)
                }
                FloatMapFormat::Float64BigEndian => {
                    let b = [
                        rom[curr_offset],
                        rom[curr_offset + 1],
                        rom[curr_offset + 2],
                        rom[curr_offset + 3],
                        rom[curr_offset + 4],
                        rom[curr_offset + 5],
                        rom[curr_offset + 6],
                        rom[curr_offset + 7],
                    ];
                    f64::from_be_bytes(b)
                }
            };
            row.push(val);
            curr_offset += bytes_per_element;
        }
        matrix.push(row);
    }

    Ok(matrix)
}

/// Writes a row-major matrix into a copy of `rom` and returns it.
///
/// Pure: the supplied buffer is never mutated.
pub fn write_float_matrix_into(
    rom: &[u8],
    start_offset: usize,
    matrix: &[Vec<f64>],
    format: FloatMapFormat,
) -> Result<Vec<u8>, &'static str> {
    let rows = matrix.len();
    let mut out = rom.to_vec();
    if rows == 0 {
        return Ok(out);
    }

    let cols = matrix[0].len();
    if matrix.iter().any(|row| row.len() != cols) {
        return Err("Float matrix rows must have uniform length");
    }
    let bytes_per_element = format.bytes_per_element();

    let total_bytes = rows
        .checked_mul(cols)
        .and_then(|c| c.checked_mul(bytes_per_element))
        .ok_or("Float matrix size overflows usize")?;
    if start_offset
        .checked_add(total_bytes)
        .is_none_or(|end| end > out.len())
    {
        return Err("Float matrix exceeds ROM buffer bounds");
    }

    let mut curr_offset = start_offset;
    for row in matrix {
        for &val in row {
            match format {
                FloatMapFormat::Float32LittleEndian => {
                    out[curr_offset..curr_offset + 4]
                        .copy_from_slice(&(val as f32).to_le_bytes());
                }
                FloatMapFormat::Float32BigEndian => {
                    out[curr_offset..curr_offset + 4]
                        .copy_from_slice(&(val as f32).to_be_bytes());
                }
                FloatMapFormat::Float64LittleEndian => {
                    out[curr_offset..curr_offset + 8].copy_from_slice(&val.to_le_bytes());
                }
                FloatMapFormat::Float64BigEndian => {
                    out[curr_offset..curr_offset + 8].copy_from_slice(&val.to_be_bytes());
                }
            }
            curr_offset += bytes_per_element;
        }
    }

    Ok(out)
}
