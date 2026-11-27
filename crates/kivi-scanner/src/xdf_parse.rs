//! XDF parsing and blind axis pairing (Zero-XDF Z4).
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery`:
//! `ParsedXdf*` types, `pair_axes`, `parse_xdf_xml`, `extract_table_matrix_from_bin`.
//!
//! Pure string/byte processing: no I/O, deterministic.

#![allow(
    clippy::needless_range_loop,
    clippy::manual_range_contains,
    clippy::chunks_exact_to_as_chunks,
    clippy::manual_is_multiple_of,
    clippy::match_like_matches_macro,
    clippy::manual_div_ceil
)]

/// One parsed XDF axis.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedXdfAxis {
    /// Axis identifier (`x`, `y` or `z`).
    pub id: char,
    pub index_count: usize,
    pub address: usize,
    pub element_size_bits: usize,
    pub type_flags: u32,
    /// Row-to-row distance in bits. `0` means the rows are packed, i.e. the
    /// stride equals `index_count * element_size_bits`.
    ///
    /// This is the XDF `mmedmajorstridebits` attribute. A non-zero value that differs
    /// from the packed size means the table is padded and rows are **not**
    /// adjacent; reading it as a contiguous block returns cells from the wrong
    /// row. Negative values walk backwards from the declared address.
    pub major_stride_bits: i32,
    /// Column-to-column distance in bits. `0` means packed.
    pub minor_stride_bits: i32,
    pub units: String,
    pub math_equation: String,
}

/// A fully parsed XDF document.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedXdf {
    /// Document title.
    pub title: String,
    /// Document author.
    pub author: String,
    /// Declared categories as `(index, name)`.
    pub categories: Vec<(u32, String)>,
    /// Parsed constants.
    pub constants: Vec<ParsedXdfConstant>,
    /// Parsed tables.
    pub tables: Vec<ParsedXdfTable>,
}

/// One parsed XDF table definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedXdfTable {
    /// Unique table id.
    pub unique_id: u32,
    pub title: String,
    pub description: String,
    pub category: u32,
    pub x_axis: Option<ParsedXdfAxis>,
    pub y_axis: Option<ParsedXdfAxis>,
    pub z_axis: ParsedXdfAxis,
}

/// One parsed XDF constant definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedXdfConstant {
    /// Unique constant id.
    pub unique_id: u32,
    pub title: String,
    pub description: String,
    pub category: u32,
    pub address: usize,
    pub element_size_bits: usize,
    pub type_flags: u32,
    pub units: String,
    pub math_equation: String,
}


impl ParsedXdf {
    pub fn pair_axes(rom: &[u8], table: &mut crate::zero_xdf_scan::DiscoveredFullTable) {
        let rows = table.rows;
        let cols = table.cols;
        let z_addr = table.z_address;
        let rom_len = rom.len();
        let is_be_table = matches!(
            table.data_format,
            crate::zero_xdf_scan::MapDataBits::Bits16(crate::zero_xdf_scan::Endianness::BigEndian) | crate::zero_xdf_scan::MapDataBits::Bits32(crate::zero_xdf_scan::Endianness::BigEndian)
        );

        // =====================================================================
        // STRATEGY A: DENSO / HITACHI 32-BIT POINTER DESCRIPTOR SCAN
        // =====================================================================
        let z_target_le = (z_addr as u32).to_le_bytes();
        let z_target_be = (z_addr as u32).to_be_bytes();

        let search_start = (z_addr.saturating_sub(65536) / 4) * 4;
        let search_end = (z_addr + 65536).min(rom_len.saturating_sub(12));

        for i in (search_start..search_end).step_by(4) {
            // Check LE pointer match
            if rom[i..i + 4] == z_target_le && i >= 8 {
                let x_ptr = (u32::from_le_bytes([rom[i - 8], rom[i - 7], rom[i - 6], rom[i - 5]]) & 0x00FFFFFF) as usize;
                let y_ptr = (u32::from_le_bytes([rom[i - 4], rom[i - 3], rom[i - 2], rom[i - 1]]) & 0x00FFFFFF) as usize;

                if x_ptr + cols * 2 <= rom_len && y_ptr + rows * 2 <= rom_len {
                    let tbl_len = rows * cols * 2;
                    let tbl_end = z_addr + tbl_len;
                    let x_end = x_ptr + cols * 2;
                    let y_end = y_ptr + rows * 2;

                    if (x_ptr < tbl_end && x_end > z_addr) || (y_ptr < tbl_end && y_end > z_addr) {
                        continue;
                    }

                    let x_sl = &rom[x_ptr..x_ptr + cols * 2];
                    let x_vals: Vec<f64> = x_sl.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as f64).collect();

                    let y_sl = &rom[y_ptr..y_ptr + rows * 2];
                    let y_vals: Vec<f64> = y_sl.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as f64).collect();

                    if crate::zero_xdf_scan::is_monotonic(&x_vals) && crate::zero_xdf_scan::is_monotonic(&y_vals) {
                        let x_min = *x_vals.first().unwrap();
                        let x_max = *x_vals.last().unwrap();
                        let (x_lo, x_hi) = if x_min < x_max { (x_min, x_max) } else { (x_max, x_min) };

                        let y_min = *y_vals.first().unwrap();
                        let y_max = *y_vals.last().unwrap();
                        let (y_lo, y_hi) = if y_min < y_max { (y_min, y_max) } else { (y_max, y_min) };

                        let x_unit = if x_hi > 35000.0 {
                            "Throttle Position / TPS (%)"
                        } else if x_hi >= 30000.0 && x_lo >= 7000.0 && x_lo <= 8500.0 {
                            "Manifold Pressure / Vacuum (kPa)"
                        } else if x_hi >= 4000.0 && x_hi <= 45000.0 {
                            "Engine Speed (RPM)"
                        } else {
                            "Throttle / Load Axis"
                        };

                        let y_unit = if y_hi >= 4000.0 && y_hi <= 45000.0 {
                            "Engine Speed (RPM)"
                        } else if y_hi > 35000.0 {
                            "Throttle Position / TPS (%)"
                        } else if y_hi >= 30000.0 && y_lo >= 7000.0 && y_lo <= 8500.0 {
                            "Manifold Pressure / Vacuum (kPa)"
                        } else {
                            "Engine Speed / Row Axis"
                        };

                        table.x_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                            address: x_ptr,
                            points_count: cols,
                            is_16bit: true,
                            endianness: crate::zero_xdf_scan::Endianness::LittleEndian,
                            estimated_unit: x_unit.to_string(),
                            min_val: x_lo,
                            max_val: x_hi,
                            values: x_vals,
                        });
                        table.y_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                            address: y_ptr,
                            points_count: rows,
                            is_16bit: true,
                            endianness: crate::zero_xdf_scan::Endianness::LittleEndian,
                            estimated_unit: y_unit.to_string(),
                            min_val: y_lo,
                            max_val: y_hi,
                            values: y_vals,
                        });
                        table.confidence_score = 0.99;
                        return;
                    }
                }
            }

            // Check BE pointer match
            if rom[i..i + 4] == z_target_be && i >= 8 {
                let x_ptr = (u32::from_be_bytes([rom[i - 8], rom[i - 7], rom[i - 6], rom[i - 5]]) & 0x00FFFFFF) as usize;
                let y_ptr = (u32::from_be_bytes([rom[i - 4], rom[i - 3], rom[i - 2], rom[i - 1]]) & 0x00FFFFFF) as usize;

                if x_ptr + cols * 2 <= rom_len && y_ptr + rows * 2 <= rom_len {
                    let tbl_len = rows * cols * 2;
                    let tbl_end = z_addr + tbl_len;
                    let x_end = x_ptr + cols * 2;
                    let y_end = y_ptr + rows * 2;

                    if (x_ptr < tbl_end && x_end > z_addr) || (y_ptr < tbl_end && y_end > z_addr) {
                        continue;
                    }

                    let x_sl = &rom[x_ptr..x_ptr + cols * 2];
                    let x_vals: Vec<f64> = x_sl.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]]) as f64).collect();

                    let y_sl = &rom[y_ptr..y_ptr + rows * 2];
                    let y_vals: Vec<f64> = y_sl.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]]) as f64).collect();

                    if crate::zero_xdf_scan::is_monotonic(&x_vals) && crate::zero_xdf_scan::is_monotonic(&y_vals) {
                        let x_min = *x_vals.first().unwrap();
                        let x_max = *x_vals.last().unwrap();
                        let (x_lo, x_hi) = if x_min < x_max { (x_min, x_max) } else { (x_max, x_min) };

                        let y_min = *y_vals.first().unwrap();
                        let y_max = *y_vals.last().unwrap();
                        let (y_lo, y_hi) = if y_min < y_max { (y_min, y_max) } else { (y_max, y_min) };

                        let x_unit = if x_hi >= 4000.0 && x_hi <= 45000.0 {
                            "Engine Speed (RPM)"
                        } else if x_hi > 20000.0 {
                            "Manifold Pressure / Vacuum (kPa)"
                        } else {
                            "Throttle Position / TPS (%)"
                        };

                        let y_unit = if y_hi >= 4000.0 && y_hi <= 45000.0 {
                            "Engine Speed (RPM)"
                        } else if y_hi > 20000.0 {
                            "Manifold Pressure / Vacuum (kPa)"
                        } else {
                            "Engine Speed / Row Axis"
                        };

                        table.x_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                            address: x_ptr,
                            points_count: cols,
                            is_16bit: true,
                            endianness: crate::zero_xdf_scan::Endianness::BigEndian,
                            estimated_unit: x_unit.to_string(),
                            min_val: x_lo,
                            max_val: x_hi,
                            values: x_vals,
                        });
                        table.y_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                            address: y_ptr,
                            points_count: rows,
                            is_16bit: true,
                            endianness: crate::zero_xdf_scan::Endianness::BigEndian,
                            estimated_unit: y_unit.to_string(),
                            min_val: y_lo,
                            max_val: y_hi,
                            values: y_vals,
                        });
                        table.confidence_score = 0.99;
                        return;
                    }
                }
            }
        }

        // =====================================================================
        // STRATEGY B: CONTIGUOUS AXES LAYOUT
        // Pattern 1: [Y-Axis (rows * 2 bytes)] -> [X-Axis (cols * 2 bytes)] -> [Z-Data] (Bosch)
        // Pattern 2: [X-Axis (cols * 2 bytes)] -> [Y-Axis (rows * 2 bytes)] -> [Z-Data] (Japanese/Denso)
        // Supports Little-Endian and Big-Endian, 16-bit and 8-bit tables.
        // =====================================================================
        let x_size_bytes = cols * 2;
        let y_size_bytes = rows * 2;

        if z_addr >= x_size_bytes + y_size_bytes {
            for &test_be in &[is_be_table, !is_be_table] {
                // Check Pattern 1: [Y] -> [X] -> [Z] (Standard Bosch / MEVD Contiguous Layout)
                let x_cand_p1 = z_addr - x_size_bytes;
                let y_cand_p1 = x_cand_p1 - y_size_bytes;
                let x_vals_p1: Vec<f64> = rom[x_cand_p1..z_addr].chunks_exact(2).map(|c| {
                    if test_be { u16::from_be_bytes([c[0], c[1]]) as f64 } else { u16::from_le_bytes([c[0], c[1]]) as f64 }
                }).collect();
                let y_vals_p1: Vec<f64> = rom[y_cand_p1..x_cand_p1].chunks_exact(2).map(|c| {
                    if test_be { u16::from_be_bytes([c[0], c[1]]) as f64 } else { u16::from_le_bytes([c[0], c[1]]) as f64 }
                }).collect();

                if crate::zero_xdf_scan::is_monotonic(&x_vals_p1) && crate::zero_xdf_scan::is_monotonic(&y_vals_p1) {
                    let x_min = *x_vals_p1.first().unwrap();
                    let x_max = *x_vals_p1.last().unwrap();
                    let (x_lo, x_hi) = if x_min < x_max { (x_min, x_max) } else { (x_max, x_min) };

                    let y_min = *y_vals_p1.first().unwrap();
                    let y_max = *y_vals_p1.last().unwrap();
                    let (y_lo, y_hi) = if y_min < y_max { (y_min, y_max) } else { (y_max, y_min) };

                    table.x_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                        address: x_cand_p1,
                        points_count: cols,
                        is_16bit: true,
                        endianness: if test_be { crate::zero_xdf_scan::Endianness::BigEndian } else { crate::zero_xdf_scan::Endianness::LittleEndian },
                        estimated_unit: if x_hi >= 4000.0 { "Engine Speed (RPM)" } else { "Relative Engine Load (% / mbar)" }.to_string(),
                        min_val: x_lo,
                        max_val: x_hi,
                        values: x_vals_p1,
                    });
                    table.y_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                        address: y_cand_p1,
                        points_count: rows,
                        is_16bit: true,
                        endianness: if test_be { crate::zero_xdf_scan::Endianness::BigEndian } else { crate::zero_xdf_scan::Endianness::LittleEndian },
                        estimated_unit: if y_hi >= 4000.0 { "Engine Speed (RPM)" } else { "Throttle / Load (%)" }.to_string(),
                        min_val: y_lo,
                        max_val: y_hi,
                        values: y_vals_p1,
                    });
                    table.confidence_score = 0.98;
                    return;
                }

                // Check Pattern 2: [X] -> [Y] -> [Z] (Alternative Contiguous Layout)
                let x_cand_p2 = z_addr - (x_size_bytes + y_size_bytes);
                let y_cand_p2 = z_addr - y_size_bytes;
                let x_vals_p2: Vec<f64> = rom[x_cand_p2..y_cand_p2].chunks_exact(2).map(|c| {
                    if test_be { u16::from_be_bytes([c[0], c[1]]) as f64 } else { u16::from_le_bytes([c[0], c[1]]) as f64 }
                }).collect();
                let y_vals_p2: Vec<f64> = rom[y_cand_p2..z_addr].chunks_exact(2).map(|c| {
                    if test_be { u16::from_be_bytes([c[0], c[1]]) as f64 } else { u16::from_le_bytes([c[0], c[1]]) as f64 }
                }).collect();

                if crate::zero_xdf_scan::is_monotonic(&x_vals_p2) && crate::zero_xdf_scan::is_monotonic(&y_vals_p2) {
                    let x_min = *x_vals_p2.first().unwrap();
                    let x_max = *x_vals_p2.last().unwrap();
                    let (x_lo, x_hi) = if x_min < x_max { (x_min, x_max) } else { (x_max, x_min) };

                    let y_min = *y_vals_p2.first().unwrap();
                    let y_max = *y_vals_p2.last().unwrap();
                    let (y_lo, y_hi) = if y_min < y_max { (y_min, y_max) } else { (y_max, y_min) };

                    table.x_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                        address: x_cand_p2,
                        points_count: cols,
                        is_16bit: true,
                        endianness: if test_be { crate::zero_xdf_scan::Endianness::BigEndian } else { crate::zero_xdf_scan::Endianness::LittleEndian },
                        estimated_unit: if x_hi >= 4000.0 { "Engine Speed (RPM)" } else { "Throttle / Load (%)" }.to_string(),
                        min_val: x_lo,
                        max_val: x_hi,
                        values: x_vals_p2,
                    });
                    table.y_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                        address: y_cand_p2,
                        points_count: rows,
                        is_16bit: true,
                        endianness: if test_be { crate::zero_xdf_scan::Endianness::BigEndian } else { crate::zero_xdf_scan::Endianness::LittleEndian },
                        estimated_unit: if y_hi >= 4000.0 { "Engine Speed (RPM)" } else { "Throttle / Load (%)" }.to_string(),
                        min_val: y_lo,
                        max_val: y_hi,
                        values: y_vals_p2,
                    });
                    table.confidence_score = 0.98;
                    return;
                }
            }
        }

        // =====================================================================
        // STRATEGY D: KEIHIN / DENSO SHARED PROXIMITY AXIS POOL
        // =====================================================================
        if table.x_axis.is_none() || table.y_axis.is_none() {
            let elem_size = match table.data_format {
                crate::zero_xdf_scan::MapDataBits::Bits8 => 1,
                _ => 2,
            };
            let raw_size = rows * cols * elem_size;
            let step_aligned = (raw_size + 3) & !3;
            let search_start = (z_addr.saturating_sub(65536) / 4) * 4;
            let z_target_bytes = if is_be_table { (z_addr as u32).to_be_bytes() } else { (z_addr as u32).to_le_bytes() };

            // D.1: Search for z_addr in a pointer list (32-bit hardware pointers)
            for i in (search_start..z_addr).step_by(4) {
                if rom[i..i + 4] == z_target_bytes {
                    // Trace backward to find start of this table run (first table)
                    let mut s = i;
                    while s >= 4 {
                        let prev = (if is_be_table {
                            u32::from_be_bytes([rom[s - 4], rom[s - 3], rom[s - 2], rom[s - 1]])
                        } else {
                            u32::from_le_bytes([rom[s - 4], rom[s - 3], rom[s - 2], rom[s - 1]])
                        } & 0x00FFFFFF) as usize;
                        let curr = (if is_be_table {
                            u32::from_be_bytes([rom[s], rom[s + 1], rom[s + 2], rom[s + 3]])
                        } else {
                            u32::from_le_bytes([rom[s], rom[s + 1], rom[s + 2], rom[s + 3]])
                        } & 0x00FFFFFF) as usize;

                        let diff = curr.saturating_sub(prev);
                        if diff > 0 && (diff == raw_size || diff == step_aligned || diff % step_aligned == 0) && prev >= 0x1000 {
                            s -= 4;
                        } else {
                            break;
                        }
                    }
                    let first_table = (if is_be_table {
                        u32::from_be_bytes([rom[s], rom[s + 1], rom[s + 2], rom[s + 3]])
                    } else {
                        u32::from_le_bytes([rom[s], rom[s + 1], rom[s + 2], rom[s + 3]])
                    } & 0x00FFFFFF) as usize;

                    // Trace forward to find end of pointer run (pool start)
                    let mut e = i;
                    while e + 4 <= rom_len {
                        let curr = (if is_be_table {
                            u32::from_be_bytes([rom[e], rom[e + 1], rom[e + 2], rom[e + 3]])
                        } else {
                            u32::from_le_bytes([rom[e], rom[e + 1], rom[e + 2], rom[e + 3]])
                        } & 0x00FFFFFF) as usize;
                        let nxt = if e + 8 <= rom_len {
                            (if is_be_table {
                                u32::from_be_bytes([rom[e + 4], rom[e + 5], rom[e + 6], rom[e + 7]])
                            } else {
                                u32::from_le_bytes([rom[e + 4], rom[e + 5], rom[e + 6], rom[e + 7]])
                            } & 0x00FFFFFF) as usize
                        } else {
                            0
                        };
                        let diff = nxt.saturating_sub(curr);
                        if diff > 0 && (diff == raw_size || diff == step_aligned || diff % step_aligned == 0) && nxt < rom_len {
                            e += 4;
                        } else {
                            e += 4;
                            break;
                        }
                    }
                    let pool_start = e;

                    // Inside pool_start..first_table, search for RPM axis (length rows)
                    for y_cand in [
                        first_table.saturating_sub(rows * 2),
                        first_table.saturating_sub(rows * 2 + 2),
                        first_table.saturating_sub(rows * 2 + 4),
                    ] {
                        if y_cand >= pool_start && y_cand + rows * 2 <= first_table {
                            let sl = &rom[y_cand..y_cand + rows * 2];
                            let y_vals: Vec<f64> = sl.chunks_exact(2).map(|c| {
                                if is_be_table { u16::from_be_bytes([c[0], c[1]]) as f64 }
                                else { u16::from_le_bytes([c[0], c[1]]) as f64 }
                            }).collect();
                            if crate::zero_xdf_scan::is_monotonic(&y_vals) {
                                let min_y = *y_vals.first().unwrap();
                                let max_y = *y_vals.last().unwrap();
                                let (y_lo, y_hi) = if min_y < max_y { (min_y, max_y) } else { (max_y, min_y) };
                                if y_hi >= 4000.0 && y_hi <= 20000.0 && (y_hi - y_lo) >= 2000.0 {
                                    // Search for cols axis (TPS or MAP)
                                    for x_cand in [
                                        y_cand.saturating_sub(cols * 2),
                                        y_cand.saturating_sub(cols * 2 + 2),
                                        pool_start,
                                    ] {
                                        if x_cand >= pool_start && x_cand + cols * 2 <= y_cand {
                                            let x_sl = &rom[x_cand..x_cand + cols * 2];
                                            let x_vals: Vec<f64> = x_sl.chunks_exact(2).map(|c| {
                                                if is_be_table { u16::from_be_bytes([c[0], c[1]]) as f64 }
                                                else { u16::from_le_bytes([c[0], c[1]]) as f64 }
                                            }).collect();
                                            if crate::zero_xdf_scan::is_monotonic(&x_vals) {
                                                let min_x = *x_vals.first().unwrap();
                                                let max_x = *x_vals.last().unwrap();
                                                let (x_lo, x_hi) = if min_x < max_x { (min_x, max_x) } else { (max_x, min_x) };
                                                if x_hi <= 4000.0 && (x_hi - x_lo) >= 50.0 {
                                                    table.x_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                                                        address: x_cand,
                                                        points_count: cols,
                                                        is_16bit: true,
                                                        endianness: if is_be_table { crate::zero_xdf_scan::Endianness::BigEndian } else { crate::zero_xdf_scan::Endianness::LittleEndian },
                                                        estimated_unit: if x_cand == pool_start { "Throttle Position / TPS (%)" } else { "Manifold Pressure / Vacuum (kPa)" }.to_string(),
                                                        min_val: x_lo,
                                                        max_val: x_hi,
                                                        values: x_vals,
                                                    });
                                                    table.y_axis = Some(crate::zero_xdf_scan::DiscoveredAxis {
                                                        address: y_cand,
                                                        points_count: rows,
                                                        is_16bit: true,
                                                        endianness: if is_be_table { crate::zero_xdf_scan::Endianness::BigEndian } else { crate::zero_xdf_scan::Endianness::LittleEndian },
                                                        estimated_unit: "Engine Speed (RPM)".to_string(),
                                                        min_val: y_lo,
                                                        max_val: y_hi,
                                                        values: y_vals,
                                                    });
                                                    table.confidence_score = 0.96;
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // =====================================================================
        // STRATEGY C: BOSCH CALIBRATION BANK MONOTONIC AXIS SEARCH
        // =====================================================================
        // If Strategy A and B didn't pair both axes, search nearby calibration region
        // (+/- 4096 bytes) for matching monotonic 16-bit X (cols) and Y (rows) arrays.
        if table.x_axis.is_none() || table.y_axis.is_none() {
            let search_start = z_addr.saturating_sub(4096);
            let search_end = (z_addr + 4096).min(rom.len());

            let mut matched_x: Option<crate::zero_xdf_scan::DiscoveredAxis> = None;
            let mut matched_y: Option<crate::zero_xdf_scan::DiscoveredAxis> = None;

            let is_le = match table.data_format {
                crate::zero_xdf_scan::MapDataBits::Bits16(crate::zero_xdf_scan::Endianness::BigEndian) | crate::zero_xdf_scan::MapDataBits::Bits32(crate::zero_xdf_scan::Endianness::BigEndian) => false,
                _ => true,
            };

            let tbl_len = rows * cols * if match table.data_format { crate::zero_xdf_scan::MapDataBits::Bits8 => false, _ => true } { 2 } else { 1 };
            let tbl_end = z_addr + tbl_len;

            for off in (search_start..search_end).step_by(2) {
                // Ensure axis does not overlap the table's own data
                if off < tbl_end && off + cols * 2 > z_addr {
                    continue;
                }
                if matched_x.is_none() && off + cols * 2 <= rom.len() {
                    let sl = &rom[off..off + cols * 2];
                    let vals: Vec<f64> = sl.chunks_exact(2).map(|c| {
                        if is_le { u16::from_le_bytes([c[0], c[1]]) as f64 }
                        else { u16::from_be_bytes([c[0], c[1]]) as f64 }
                    }).collect();

                    if crate::zero_xdf_scan::is_monotonic(&vals) {
                        let min_v = *vals.first().unwrap();
                        let max_v = *vals.last().unwrap();
                        let (lo, hi) = if min_v < max_v { (min_v, max_v) } else { (max_v, min_v) };
                        if (hi >= 400.0 && hi <= 10000.0 && (hi - lo) >= 500.0) || (hi >= 50.0 && hi <= 500.0) {
                            matched_x = Some(crate::zero_xdf_scan::DiscoveredAxis {
                                address: off,
                                points_count: cols,
                                is_16bit: true,
                                endianness: if is_le { crate::zero_xdf_scan::Endianness::LittleEndian } else { crate::zero_xdf_scan::Endianness::BigEndian },
                                estimated_unit: if hi >= 400.0 { "Engine Speed (RPM)" } else { "Relative Load (%)" }.to_string(),
                                min_val: lo,
                                max_val: hi,
                                values: vals,
                            });
                        }
                    }
                }

                if matched_y.is_none() && off + rows * 2 <= rom.len() {
                    if off < tbl_end && off + rows * 2 > z_addr {
                        continue;
                    }
                    let sl = &rom[off..off + rows * 2];
                    let vals: Vec<f64> = sl.chunks_exact(2).map(|c| {
                        if is_le { u16::from_le_bytes([c[0], c[1]]) as f64 }
                        else { u16::from_be_bytes([c[0], c[1]]) as f64 }
                    }).collect();

                    if crate::zero_xdf_scan::is_monotonic(&vals) {
                        let min_v = *vals.first().unwrap();
                        let max_v = *vals.last().unwrap();
                        let (lo, hi) = if min_v < max_v { (min_v, max_v) } else { (max_v, min_v) };
                        if (hi >= 500.0 && hi <= 45000.0 && (hi - lo) >= 500.0) || (hi >= 50.0 && hi <= 500.0) {
                            matched_y = Some(crate::zero_xdf_scan::DiscoveredAxis {
                                address: off,
                                points_count: rows,
                                is_16bit: true,
                                endianness: if is_le { crate::zero_xdf_scan::Endianness::LittleEndian } else { crate::zero_xdf_scan::Endianness::BigEndian },
                                estimated_unit: if hi >= 5000.0 { "Engine Load / Pressure (kPa/mbar)" } else { "Engine Speed (RPM)" }.to_string(),
                                min_val: lo,
                                max_val: hi,
                                values: vals,
                            });
                        }
                    }
                }

                if matched_x.is_some() && matched_y.is_some() {
                    break;
                }
            }

            if table.x_axis.is_none() && matched_x.is_some() {
                table.x_axis = matched_x;
            }
            if table.y_axis.is_none() && matched_y.is_some() {
                table.y_axis = matched_y;
            }
        }
    }
}

    pub fn parse_xdf_xml(xml: &str) -> Result<ParsedXdf, String> {
        // 1. Tag existence validation
        if !xml.contains("<XDFHEADER>") || !xml.contains("</XDFHEADER>") {
            return Err("Missing mandatory <XDFHEADER> tag".to_string());
        }
        // `<CATEGORY>` is optional in the XDF format: Suzuki Hayabusa and Toyota
        // Innova definitions ship without one, and requiring it dropped them
        // from every benchmark run.
        // A definition may legitimately contain only 3D tables or only 1D
        // constants (an ECU with no maps, or a limiter-only definition). Require
        // at least one of the two, not both.
        if !xml.contains("<XDFTABLE") && !xml.contains("<XDFCONSTANT") {
            return Err("XDF contains neither <XDFTABLE> nor <XDFCONSTANT>".to_string());
        }
        // The z-axis is only mandatory when a table exists.
        if xml.contains("<XDFTABLE") && !xml.contains("<XDFAXIS id=\"z\"") {
            return Err("Missing mandatory <XDFAXIS id=\"z\"> tag".to_string());
        }
        // Two dialects carry the address. Standard XDF writes `<EMBEDDEDDATA>`; the
        // Suzuki Hayabusa and Toyota Innova definitions write a compact
        // `<EMBEDDED ... address="0x..." />` and may omit `<MATH>` entirely.
        if xml.contains("<XDFTABLE")
            && !xml.contains("<EMBEDDEDDATA")
            && !xml.contains("<EMBEDDED ")
        {
            return Err("Missing mandatory <EMBEDDEDDATA> or <EMBEDDED> tag".to_string());
        }

        let parse_hex_or_dec = |s: &str| -> usize {
            let t = s.trim();
            if t.starts_with("0x") || t.starts_with("0X") {
                usize::from_str_radix(&t[2..], 16).unwrap_or(0)
            } else {
                t.parse().unwrap_or(0)
            }
        };

        let parse_hex_or_dec_u32 = |s: &str| -> u32 {
            let t = s.trim();
            if t.starts_with("0x") || t.starts_with("0X") {
                u32::from_str_radix(&t[2..], 16).unwrap_or(0)
            } else {
                t.parse().unwrap_or(0)
            }
        };

        let extract_tag = |block: &str, tag: &str| -> Option<String> {
            let open = format!("<{}>", tag);
            let close = format!("</{}>", tag);
            if let Some(s) = block.find(&open) {
                let start = s + open.len();
                if let Some(e) = block[start..].find(&close) {
                    return Some(block[start..start + e].to_string());
                }
            }
            None
        };

        let extract_attr = |element: &str, attr: &str| -> Option<String> {
            let pattern = format!("{}=\"", attr);
            if let Some(s) = element.find(&pattern) {
                let start = s + pattern.len();
                if let Some(e) = element[start..].find('"') {
                    return Some(element[start..start + e].to_string());
                }
            }
            None
        };

        // The address lives in one of two elements. Standard XDF writes
        // `<EMBEDDEDDATA mmedaddress=... />`; the compact dialect writes
        // `<EMBEDDED type="1" count="24" address="0x40000" />`. Return whichever
        // open tag comes first, so a block is parsed in its own dialect.
        fn embedded_elt(block: &str) -> Option<&str> {
            let a = block.find("<EMBEDDEDDATA");
            let b = block.find("<EMBEDDED ");
            let start = match (a, b) {
                (Some(x), Some(y)) => x.min(y),
                (Some(x), None) => x,
                (None, Some(y)) => y,
                (None, None) => return None,
            };
            let end = block[start..].find("/>")? + 2;
            Some(&block[start..start + end])
        }
        // Address, in either spelling.
        let elt_address = |e: &str| -> usize {
            extract_attr(e, "mmedaddress")
                .or_else(|| extract_attr(e, "address"))
                .map(|s| parse_hex_or_dec(&s))
                .unwrap_or(0)
        };
        // Element width, in either spelling.
        let elt_bits = |e: &str| -> usize {
            extract_attr(e, "mmedelementsizebits")
                .or_else(|| extract_attr(e, "datasizeinbits"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(16)
        };

        // Parse Header
        let title = extract_tag(xml, "deftitle").unwrap_or_else(|| "Unknown XDF".to_string());
        let author = extract_tag(xml, "author").unwrap_or_else(|| "kiviTuner".to_string());

        // XDF BASEOFFSET: file_offset = address +/- offset based on subtract flag.
        // Ignoring it reads every table from the wrong place. The editor parser
        // applies the same rule; this one must agree or auto-mapping and the
        // editor would disagree about where a table lives.
        //
        // Both tag forms occur: `<BASEOFFSET ... />` and with a closing tag. Only
        // the attributes matter, so the opening tag alone is enough.
        let parse_signed = |text: &str| -> Option<i64> {
            let t = text.trim();
            if t.is_empty() {
                return None;
            }
            let (neg, rest) = match t.strip_prefix('-') {
                Some(r) => (true, r.trim()),
                None => (false, t),
            };
            let value = if let Some(hex) =
                rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X"))
            {
                i64::from_str_radix(hex, 16).ok()?
            } else {
                rest.parse::<i64>().ok()?
            };
            Some(if neg { -value } else { value })
        };
        let (base_offset, base_subtract) = match xml.find("<BASEOFFSET") {
            Some(start) => match xml[start..].find('>') {
                Some(end) => {
                    let tag = &xml[start..start + end + 1];
                    (
                        extract_attr(tag, "offset")
                            .and_then(|v| parse_signed(&v))
                            .unwrap_or(0),
                        extract_attr(tag, "subtract")
                            .and_then(|v| parse_signed(&v))
                            .unwrap_or(0),
                    )
                }
                None => (0, 0),
            },
            None => (0, 0),
        };
        // XDF BASEOFFSET selects the DIRECTION of the shift:
        //     file_offset = address - base   if subtract else
        //                   address + base
        // Verified against reference XDF addressing rules and against real ROMs: on the
        // MS43 definition (offset 458 752, subtract 0) adding the base puts
        // 3 736 of 3 782 addresses on varying data, while subtracting drives
        // all of them negative.
        let shift = move |address: usize| -> usize {
            let v = if base_subtract != 0 {
                address as i64 - base_offset
            } else {
                address as i64 + base_offset
            };
            v.max(0) as usize
        };


        // Parse Categories
        let mut categories = Vec::new();
        let mut cat_pos = 0;
        // Match either spelling, whichever appears first.
        while let Some((idx, tag_len)) = {
            let a = xml[cat_pos..].find("<CATEGORY").map(|i| (i, "<CATEGORY".len()));
            let b = xml[cat_pos..]
                .find("<XDFCATEGORY")
                .map(|i| (i, "<XDFCATEGORY".len()));
            match (a, b) {
                (Some(x), Some(y)) => Some(if x.0 <= y.0 { x } else { y }),
                (Some(x), None) => Some(x),
                (None, Some(y)) => Some(y),
                (None, None) => None,
            }
        } {
            let abs_idx = cat_pos + idx;
            let _ = tag_len;
            if let Some(end) = xml[abs_idx..].find("/>") {
                let line = &xml[abs_idx..abs_idx + end + 2];
                let c_idx = extract_attr(line, "index").and_then(|s| s.parse().ok()).unwrap_or(0);
                let c_name = extract_attr(line, "name").unwrap_or_default();
                categories.push((c_idx, c_name));
                cat_pos = abs_idx + end + 2;
            } else {
                break;
            }
        }

        // Parse Constants (1D Limiters)
        let mut constants = Vec::new();
        let mut const_pos = 0;
        while let Some(idx) = xml[const_pos..].find("<XDFCONSTANT") {
            let abs_idx = const_pos + idx;
            if let Some(end) = xml[abs_idx..].find("</XDFCONSTANT>") {
                let block = &xml[abs_idx..abs_idx + end + 14];
                const_pos = abs_idx + end + 14;

                let uid = extract_attr(block, "uniqueid").map(|s| parse_hex_or_dec_u32(&s)).unwrap_or(0);
                let c_title = extract_tag(block, "title").unwrap_or_default();
                let c_desc = extract_tag(block, "description").unwrap_or_default();
                let c_cat = extract_attr(block, "category").and_then(|s| s.parse().ok()).unwrap_or(0);
                let c_units = extract_tag(block, "units").unwrap_or_default();
                let c_math = extract_attr(block, "equation").unwrap_or_else(|| "X".to_string());

                let (c_addr, c_bits, c_flags) = match embedded_elt(block) {
                    Some(e_line) => {
                        let flags = extract_attr(e_line, "mmedtypeflags")
                            .map(|s| parse_hex_or_dec_u32(&s))
                            .unwrap_or(0);
                        (elt_address(e_line), elt_bits(e_line), flags)
                    }
                    None => (0, 16, 0),
                };

                constants.push(ParsedXdfConstant {
                    unique_id: uid,
                    title: c_title,
                    description: c_desc,
                    category: c_cat,
                    address: shift(c_addr),
                    element_size_bits: c_bits,
                    type_flags: c_flags,
                    units: c_units,
                    math_equation: c_math,
                });
            } else {
                break;
            }
        }

        // Parse Tables (3D Maps)
        let mut tables = Vec::new();
        let mut tbl_pos = 0;
        while let Some(idx) = xml[tbl_pos..].find("<XDFTABLE") {
            let abs_idx = tbl_pos + idx;
            if let Some(end) = xml[abs_idx..].find("</XDFTABLE>") {
                let block = &xml[abs_idx..abs_idx + end + 11];
                tbl_pos = abs_idx + end + 11;

                let uid = extract_attr(block, "uniqueid").map(|s| parse_hex_or_dec_u32(&s)).unwrap_or(0);
                let t_title = extract_tag(block, "title").unwrap_or_default();
                let t_desc = extract_tag(block, "description").unwrap_or_default();
                let t_cat = extract_attr(block, "category").and_then(|s| s.parse().ok()).unwrap_or(0);

                let parse_axis = |axis_id: char| -> Option<ParsedXdfAxis> {
                    // Real XDFs write `<XDFAXIS id="x" uniqueid="0x0">`, so the
                    // open tag must be matched on the id alone, not on an exact
                    // string that assumes no other attributes.
                    let needle = format!("<XDFAXIS id=\"{}\"", axis_id);
                    if let Some(ax_start) = block.find(&needle) {
                        let ax_sub = &block[ax_start..];
                        if let Some(ax_end) = ax_sub.find("</XDFAXIS>") {
                            let ax_block = &ax_sub[..ax_end + 10];
                            let units = extract_tag(ax_block, "units").unwrap_or_default();
                            let math = extract_attr(ax_block, "equation").unwrap_or_else(|| "X".to_string());

                            let elt = embedded_elt(ax_block);
                            // The compact dialect has no <indexcount>: the count
                            // rides on the element tag itself.
                            let count = extract_tag(ax_block, "indexcount")
                                .and_then(|s| s.trim().parse().ok())
                                .or_else(|| {
                                    elt.and_then(|e| extract_attr(e, "count"))
                                        .and_then(|s| s.trim().parse().ok())
                                })
                                .unwrap_or(1);

                            let (addr, bits, flags, major, minor) = match elt {
                                Some(e_line) => {
                                    let f = extract_attr(e_line, "mmedtypeflags")
                                        .map(|s| parse_hex_or_dec_u32(&s))
                                        .unwrap_or(0);
                                    // Strides are signed: a negative value walks
                                    // backwards from the declared address.
                                    let ma = extract_attr(e_line, "mmedmajorstridebits")
                                        .and_then(|s| s.trim().parse::<i32>().ok())
                                        .unwrap_or(0);
                                    let mi = extract_attr(e_line, "mmedminorstridebits")
                                        .and_then(|s| s.trim().parse::<i32>().ok())
                                        .unwrap_or(0);
                                    (elt_address(e_line), elt_bits(e_line), f, ma, mi)
                                }
                                None => (0, 16, 0, 0, 0),
                            };

                            return Some(ParsedXdfAxis {
                                id: axis_id,
                                index_count: count,
                                address: shift(addr),
                                element_size_bits: bits,
                                type_flags: flags,
                                major_stride_bits: major,
                                minor_stride_bits: minor,
                                units,
                                math_equation: math,
                            });
                        }
                    }
                    None
                };

                let x_axis = parse_axis('x');
                let y_axis = parse_axis('y');
                let z_axis = parse_axis('z').ok_or_else(|| format!("Table {} missing Z-axis definition", t_title))?;

                tables.push(ParsedXdfTable {
                    unique_id: uid,
                    title: t_title,
                    description: t_desc,
                    category: t_cat,
                    x_axis,
                    y_axis,
                    z_axis,
                });
            } else {
                break;
            }
        }

        Ok(ParsedXdf {
            title,
            author,
            categories,
            constants,
            tables,
        })
    }

    pub fn extract_table_matrix_from_bin(
        rom: &[u8],
        table_def: &ParsedXdfTable,
    ) -> Result<Vec<Vec<f64>>, String> {
        let z = &table_def.z_axis;
        let rows = table_def.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        let cols = table_def.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        let addr = z.address;
        let bits = z.element_size_bits;
        let is_le = (z.type_flags & 0x02) != 0;

        let bytes_per_elem = if bits == 8 { 1 } else if bits == 16 { 2 } else if bits == 32 { 4 } else {
            return Err(format!("Unsupported bit size: {}", bits));
        };

        // Row and column strides. `0` means "packed": rows are
        // `cols * elem_bits` apart and columns are `elem_bits` apart. A
        // non-zero value overrides that, and may be negative to walk backwards.
        let elem_bits = (bytes_per_elem * 8) as i64;
        let packed_row_bits = cols as i64 * elem_bits;
        // Standard XDF treats a major stride equal to one element as "packed", not as
        // "one element per row":
        //     row_step = cols*minor if major_stride in (0, size_bits) else major
        // Reading it the other way collapses every multi-column table to one
        // cell per row and silently returns the wrong data.
        let major_is_packed = z.major_stride_bits == 0 || z.major_stride_bits == elem_bits as i32;
        let row_bits: i64 = if major_is_packed {
            packed_row_bits
        } else {
            z.major_stride_bits as i64
        };
        let col_bits: i64 = if z.minor_stride_bits == 0 || z.minor_stride_bits == elem_bits as i32 {
            elem_bits
        } else {
            z.minor_stride_bits as i64
        };

        // Bounds-check the real extent, which a stride can make larger than the
        // packed size (or negative, reaching below the declared address).
        let last_row_bit = (rows as i64 - 1) * row_bits;
        let last_col_bit = (cols as i64 - 1) * col_bits;
        let start = addr as i64 * 8;
        let lo = start.min(start + last_row_bit.min(0));
        let hi = start
            + last_row_bit.max(0)
            + last_col_bit.max(0)
            + elem_bits;
        let lo_byte = (lo.div_euclid(8)).max(0) as usize;
        let hi_byte = ((hi + 7).div_euclid(8)).max(0) as usize;
        if hi_byte > rom.len() || lo_byte > rom.len() {
            return Err(format!(
                "Table at 0x{:06X} with stride spans bytes {}..{} which exceeds ROM length {}",
                addr, lo_byte, hi_byte, rom.len()
            ));
        }

        let mut matrix = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut row = Vec::with_capacity(cols);
            for c in 0..cols {
                let bit = start + r as i64 * row_bits + c as i64 * col_bits;
                let offset = bit.div_euclid(8) as usize;
                if offset + bytes_per_elem > rom.len() {
                    return Err(format!(
                        "Table cell ({}, {}) at 0x{:06X} is past the end of a {}-byte ROM",
                        r, c, offset, rom.len()
                    ));
                }
                let val = match bits {
                    8 => rom[offset] as f64,
                    16 => {
                        let b0 = rom[offset];
                        let b1 = rom[offset + 1];
                        if is_le {
                            u16::from_le_bytes([b0, b1]) as f64
                        } else {
                            u16::from_be_bytes([b0, b1]) as f64
                        }
                    }
                    32 => {
                        let b = [rom[offset], rom[offset + 1], rom[offset + 2], rom[offset + 3]];
                        if is_le {
                            u32::from_le_bytes(b) as f64
                        } else {
                            u32::from_be_bytes(b) as f64
                        }
                    }
                    _ => unreachable!(),
                };
                row.push(val);
            }
            matrix.push(row);
        }
        Ok(matrix)
    }
