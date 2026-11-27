//! Blind rev-limiter detection.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine::scan_limiters`
//! and `DiscoveredLimiter`.
//!
//! ## The two branches are not symmetric — this is preserved deliberately
//!
//! **Bike branch** (`is_bike = true`) is a general range detector: it walks the
//! pre-filter's kept ranges in 2-byte steps looking for an adjacent
//! soft/hard-cut pair, `w1 ∈ [32 000, 35 000]`, `w2 ∈ [37 000, 40 000]`,
//! `w2 > w1 + 3 000`, converting with `rpm = raw / 2.56`. It also carries a
//! hardcoded special case at offset `0x172EA6` from the original Suzuki ROM.
//!
//! **Automotive branch** (`is_bike = false`) is **ROM-specific**: a candidate
//! matches only when the value *and* the exact address both hit
//! (`28 000 @ 0x1D4EA0`, `14 000 @ 0x1D4F26`, `32 000 @ 0x1D4FB0`, BMW Bosch
//! MEVD17), converting with `rpm = raw * 0.25`. On any other ROM it returns
//! nothing.
//!
//! Generalising the automotive branch would **invent** behaviour the source does
//! not have, so it is ported as-is. Callers should treat an empty automotive
//! result as "this ROM is not one the source knew about", not as "no limiter
//! exists".
//!
//! ## Dependencies
//!
//! Only [`crate::zero_xdf_prefilter::analyze_rom_blocks`], which is already
//! ported. No Z3 container types are required, so this is genuinely migratable
//! in isolation.
//!
//! Read-only, offline, deterministic.

use crate::zero_xdf_prefilter::analyze_rom_blocks;

/// A detected rev limiter.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredLimiter {
    /// Byte address of the limiter value.
    pub address: usize,
    /// Raw 16-bit value found at that address.
    pub raw_value: u32,
    /// Value converted to RPM using the branch's scale factor.
    pub converted_rpm: f64,
    /// Human-readable limiter classification.
    pub limiter_type: &'static str,
}

/// Scans `rom` for rev limiters.
///
/// `is_bike` selects the branch. See the module docs for why the two branches
/// behave so differently.
pub fn scan_limiters(rom: &[u8], is_bike: bool) -> Vec<DiscoveredLimiter> {
    let mut limiters = Vec::new();
    let prefilter = analyze_rom_blocks(rom);

    if is_bike {
        for &(start, end) in &prefilter.kept_ranges {
            let limit = end.saturating_sub(4);
            let align_start = if !start.is_multiple_of(2) { start + 1 } else { start };
            for offset in (align_start..limit).step_by(2) {
                let w1 = u16::from_le_bytes([rom[offset], rom[offset + 1]]);
                let w2 = u16::from_le_bytes([rom[offset + 2], rom[offset + 3]]);

                if offset == 0x172EA6
                    || ((32_000..=35_000).contains(&w1)
                        && (37_000..=40_000).contains(&w2)
                        && w2 > w1 + 3000)
                {
                    let rpm1 = (w1 as f64) / 2.56;
                    let rpm2 = (w2 as f64) / 2.56;
                    limiters.push(DiscoveredLimiter {
                        address: offset,
                        raw_value: w1 as u32,
                        converted_rpm: rpm1,
                        limiter_type: "Soft Cut Rev Limiter",
                    });
                    limiters.push(DiscoveredLimiter {
                        address: offset + 2,
                        raw_value: w2 as u32,
                        converted_rpm: rpm2,
                        limiter_type: "Hard Cut Rev Limiter",
                    });
                }
            }
        }
    } else {
        for &(start, end) in &prefilter.kept_ranges {
            let limit = end.saturating_sub(2);
            let align_start = if !start.is_multiple_of(2) { start + 1 } else { start };
            for offset in (align_start..limit).step_by(2) {
                let val = u16::from_le_bytes([rom[offset], rom[offset + 1]]);
                if val == 28_000 && offset == 0x1D4EA0 {
                    limiters.push(DiscoveredLimiter {
                        address: offset,
                        raw_value: val as u32,
                        converted_rpm: (val as f64) * 0.25,
                        limiter_type: "Workshop Test RPM Limit (7000 RPM)",
                    });
                } else if val == 14_000 && offset == 0x1D4F26 {
                    limiters.push(DiscoveredLimiter {
                        address: offset,
                        raw_value: val as u32,
                        converted_rpm: (val as f64) * 0.25,
                        limiter_type: "HDR-Fault Engine Limp Mode Limit (3500 RPM)",
                    });
                } else if val == 32_000 && offset == 0x1D4FB0 {
                    limiters.push(DiscoveredLimiter {
                        address: offset,
                        raw_value: val as u32,
                        converted_rpm: (val as f64) * 0.25,
                        limiter_type: "SWT Maximum Engine Speed Limit (8000 RPM)",
                    });
                }
            }
        }
    }
    limiters
}
