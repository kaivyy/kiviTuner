//! Ported from `ecu_engine_rust::recognition::denso`.
//!
//! Pure logic, no I/O. Verified by unit tests where applicable; not validated
//! against physical hardware.

/// Denso / Toyota Interleaved Map Specification (`kennfelder_manuell_finden_toyo.htm`).
///
/// Format: The Y-axis breakpoint precedes each row of table data:
/// [Y_0] [Z_00, Z_01, ... Z_0(W-1)]
/// [Y_1] [Z_10, Z_11, ... Z_1(W-1)]
/// ...
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DensoInterleavedMap {
    pub start_address: usize,
    pub rows: usize,
    pub cols: usize,
    pub is_16bit: bool,
    pub y_axis_values: Vec<f64>,
    pub matrix: Vec<Vec<f64>>,
}

pub struct DensoMapScanner;

impl DensoMapScanner {
    /// Detect interleaved Denso vertical maps in raw ROM buffer.
    pub fn scan_interleaved_maps(
        rom: &[u8],
        candidate_cols: &[usize],
        min_rows: usize,
        max_rows: usize,
    ) -> Vec<DensoInterleavedMap> {
        let mut results = Vec::new();
        let rom_len = rom.len();

        for &cols in candidate_cols {
            // For 16-bit word maps: each row has 1 word (Y-axis) + cols words (data) = (cols + 1) * 2 bytes
            let row_bytes_16 = (cols + 1) * 2;

            for rows in min_rows..=max_rows {
                let total_bytes = rows * row_bytes_16;
                let mut offset = 0;

                while offset + total_bytes <= rom_len {
                    // Extract candidate Y-axis values from row headers
                    let mut y_vals = Vec::with_capacity(rows);
                    let mut monotonic_y = true;

                    for r in 0..rows {
                        let y_offset = offset + (r * row_bytes_16);
                        let y_raw = u16::from_le_bytes([rom[y_offset], rom[y_offset + 1]]) as f64;
                        if r > 0 && y_raw <= y_vals[r - 1] {
                            monotonic_y = false;
                            break;
                        }
                        y_vals.push(y_raw);
                    }

                    // Check if Y axis looks like a reasonable RPM or Load axis
                    if monotonic_y && y_vals.len() == rows {
                        let y_min = y_vals[0];
                        let y_max = y_vals[rows - 1];
                        let is_rpm = y_min >= 500.0 && y_max <= 18000.0;
                        let is_load = y_min <= 60.0 && y_max <= 2048.0;

                        if is_rpm || is_load {
                            // Extract data matrix (skipping 2 bytes at start of each row)
                            let mut matrix = Vec::with_capacity(rows);
                            for r in 0..rows {
                                let row_start = offset + (r * row_bytes_16) + 2;
                                let mut row_data = Vec::with_capacity(cols);
                                for c in 0..cols {
                                    let cell_off = row_start + (c * 2);
                                    let val = u16::from_le_bytes([rom[cell_off], rom[cell_off + 1]]) as f64;
                                    row_data.push(val);
                                }
                                matrix.push(row_data);
                            }

                            // Smoothness gate. A rising breakpoint column occurs
                            // by chance in unrelated bytes — on a real 256 KiB
                            // Denso ROM the axis-only test returned 1 105
                            // "maps", nearly all of them noise. A calibration
                            // surface moves a small fraction of its own spread
                            // per step, so mean|Δ| / std_dev stays low.
                            //
                            // The constant matches the 2-D and curve scanners so
                            // all three detectors agree on what "smooth" means.
                            if !is_smooth_surface(&matrix) {
                                offset += 16;
                                continue;
                            }

                            results.push(DensoInterleavedMap {
                                start_address: offset,
                                rows,
                                cols,
                                is_16bit: true,
                                y_axis_values: y_vals,
                                matrix,
                            });
                        }
                    }

                    offset += 16;
                }
            }
        }

        // Suppress overlapping detections. The scan steps in 16-byte increments
        // and tries every shape, so one real map is found at many overlapping
        // offsets. Measured on a real 256 KiB Denso ROM: 922 detections across
        // only 200 unique start addresses, with 370 adjacent overlapping pairs.
        //
        // Sort by area descending so a long map is accepted before its own
        // prefixes, then drop anything overlapping an already-accepted span.
        results.sort_by(|a, b| {
            let area_a = a.rows * a.cols;
            let area_b = b.rows * b.cols;
            area_b.cmp(&area_a).then(a.start_address.cmp(&b.start_address))
        });

        let mut accepted: Vec<DensoInterleavedMap> = Vec::with_capacity(results.len());
        for cand in results {
            let cand_start = cand.start_address;
            let cand_end = cand_start + cand.rows * (cand.cols + 1) * 2;
            let overlaps = accepted.iter().any(|acc| {
                let acc_start = acc.start_address;
                let acc_end = acc_start + acc.rows * (acc.cols + 1) * 2;
                cand_start < acc_end && acc_start < cand_end
            });
            if !overlaps {
                accepted.push(cand);
            }
        }

        accepted.sort_by_key(|m| m.start_address);
        accepted
    }
}

/// True when the matrix is smooth enough to be a calibration surface.
///
/// `mean|Δ| / std_dev < 0.48`, the same gate the 2-D and curve scanners use.
/// Returns `false` for a constant block, which has no gradient to calibrate.
fn is_smooth_surface(matrix: &[Vec<f64>]) -> bool {
    let flat: Vec<f64> = matrix.iter().flatten().copied().collect();
    if flat.len() < 4 {
        return false;
    }
    let n = flat.len() as f64;
    let mean = flat.iter().sum::<f64>() / n;
    let var = flat.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    let std_dev = var.sqrt();
    if std_dev < 1e-9 {
        return false;
    }
    // Row-major deltas plus row-wrap deltas so a step between the last cell of
    // one row and the first of the next is not silently skipped.
    let deltas = flat.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>();
    let mean_delta = deltas / (n - 1.0);
    (mean_delta / std_dev) < 0.48
}

/// Bosch Motronic Standard Map Header Descriptor.
///
/// Typical descriptors:
/// - 0xE5A0: 16-bit data, 16-bit axes
/// - 0x3B80: 8-bit data, 16-bit axes
#[derive(Debug, Clone)]
pub struct BoschMapDescriptor {
    pub offset: usize,
    pub descriptor_word: u16,
    pub rows: usize,
    pub cols: usize,
    pub is_16bit_data: bool,
    pub x_axis_addr: usize,
    pub y_axis_addr: usize,
    pub data_addr: usize,
}

pub struct BoschHeaderScanner;

impl BoschHeaderScanner {
    /// Scan for Bosch Motronic structured table headers.
    pub fn scan_bosch_headers(rom: &[u8], is_little_endian: bool) -> Vec<BoschMapDescriptor> {
        let mut descriptors = Vec::new();
        if rom.len() < 16 {
            return descriptors;
        }

        for offset in 0..rom.len() - 16 {
            let id = if is_little_endian {
                u16::from_le_bytes([rom[offset], rom[offset + 1]])
            } else {
                u16::from_be_bytes([rom[offset], rom[offset + 1]])
            };

            // Standard Bosch descriptors
            if id == 0xE5A0 || id == 0x3B80 || id == 0x3B81 {
                let cols = rom[offset + 2] as usize;
                let rows = rom[offset + 3] as usize;

                if (4..=32).contains(&cols) && (4..=32).contains(&rows) {
                    let is_16bit_data = id == 0xE5A0;
                    let x_axis_addr = offset + 4;
                    let y_axis_addr = x_axis_addr + (cols * 2);
                    let data_addr = y_axis_addr + (rows * 2);

                    let data_size = rows * cols * (if is_16bit_data { 2 } else { 1 });
                    if data_addr + data_size <= rom.len() {
                        descriptors.push(BoschMapDescriptor {
                            offset,
                            descriptor_word: id,
                            rows,
                            cols,
                            is_16bit_data,
                            x_axis_addr,
                            y_axis_addr,
                            data_addr,
                        });
                    }
                }
            }
        }

        descriptors
    }
}
