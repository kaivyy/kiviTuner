//! Motorcycle ECU hardware, checksum, and flash memory-write boundary database.
//!
//! Built from verified OEM Keihin/Mitsubishi/Denso motorcycle ECU protocol specs and ECM ID tables.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MotoEcuProfile {
    pub id: String,
    pub make: String,
    pub model: String,
    pub years: String,
    pub part_number: String,
    pub mcu: String,
    pub rom_size_bytes: usize,
    pub writable_start: u32,
    pub writable_end_incl: u32,
    pub protected_ranges: Vec<(u32, u32)>,
    pub checksum_offset: Option<u32>,
    pub ecm_id_hex: Option<String>,
}

/// Returns built-in Honda PGM-FI (Keihin/Shindengen) and Yamaha flash boundary profiles.
pub fn list_moto_ecu_profiles() -> Vec<MotoEcuProfile> {
    vec![
        MotoEcuProfile {
            id: "yamaha_64k_kline".into(),
            make: "Yamaha".into(),
            model: "YZF-R15 / MT / Vixion 64K (Write-Protected Boot)".into(),
            years: "2008-2016".into(),
            part_number: "YAMAHA-64K".into(),
            mcu: "Mitsubishi/Mitsu 64K".into(),
            rom_size_bytes: 64 * 1024,
            writable_start: 0x1800,
            writable_end_incl: 0x7FFF,
            protected_ranges: vec![(0x0000, 0x17FF), (0x8000, 0xFFFF)],
            checksum_offset: Some(0x7FFE),
            ecm_id_hex: None,
        },
        MotoEcuProfile {
            id: "yamaha_128k_kline".into(),
            make: "Yamaha".into(),
            model: "Aerox / NMAX / R25 128K (BBP-H594A)".into(),
            years: "2017-2024".into(),
            part_number: "BBP-H594A-01/11".into(),
            mcu: "Renesas RL78".into(),
            rom_size_bytes: 128 * 1024,
            writable_start: 0x00000,
            writable_end_incl: 0x1BFFF,
            protected_ranges: vec![(0x1C000, 0x1FFFF)],
            checksum_offset: Some(0x1BFFE),
            ecm_id_hex: None,
        },
        MotoEcuProfile {
            id: "honda_cbr250rr_k64".into(),
            make: "Honda".into(),
            model: "CBR 250 RR (MC51)".into(),
            years: "2017-2023".into(),
            part_number: "38770-K64-N01/N04/ND1".into(),
            mcu: "Keihin SH7058".into(),
            rom_size_bytes: 512 * 1024,
            writable_start: 0x08000,
            writable_end_incl: 0x7FFFF,
            protected_ranges: vec![(0x00000, 0x07FFF)],
            checksum_offset: Some(0x7FFF8),
            ecm_id_hex: None,
        },
        MotoEcuProfile {
            id: "honda_cbr1000rr_06_07".into(),
            make: "Honda".into(),
            model: "CBR 1000 RR".into(),
            years: "2006-2007".into(),
            part_number: "38770-MEL-D21".into(),
            mcu: "Keihin M32R 256K".into(),
            rom_size_bytes: 256 * 1024,
            writable_start: 0x08000,
            writable_end_incl: 0x3FFFF,
            protected_ranges: vec![(0x00000, 0x07FFF)],
            checksum_offset: Some(0x3FFF8),
            ecm_id_hex: Some("01002B0101".into()),
        },
        MotoEcuProfile {
            id: "honda_cbr1000rr_17_18".into(),
            make: "Honda".into(),
            model: "CBR 1000 RR (SC77)".into(),
            years: "2017-2018".into(),
            part_number: "38770-MKF-D43".into(),
            mcu: "Keihin 1 MiB".into(),
            rom_size_bytes: 1024 * 1024,
            writable_start: 0x10000,
            writable_end_incl: 0xFFFFF,
            protected_ranges: vec![(0x00000, 0x0FFFF)],
            checksum_offset: Some(0xFFFF8),
            ecm_id_hex: Some("0102490101".into()),
        },
        MotoEcuProfile {
            id: "honda_cbr600rr_07_12".into(),
            make: "Honda".into(),
            model: "CBR 600 RR".into(),
            years: "2007-2012".into(),
            part_number: "38770-MFJ-D04/D42".into(),
            mcu: "Keihin M32R 256K".into(),
            rom_size_bytes: 256 * 1024,
            writable_start: 0x08000,
            writable_end_incl: 0x3FFFF,
            protected_ranges: vec![(0x00000, 0x07FFF)],
            checksum_offset: Some(0x3FFF8),
            ecm_id_hex: Some("0100330103".into()),
        },
        MotoEcuProfile {
            id: "honda_cb500_13_16".into(),
            make: "Honda".into(),
            model: "CB 500 F / CBR 500 R".into(),
            years: "2013-2016".into(),
            part_number: "38770-MGZ-A03/D02".into(),
            mcu: "Keihin 256K".into(),
            rom_size_bytes: 256 * 1024,
            writable_start: 0x08000,
            writable_end_incl: 0x3FFFF,
            protected_ranges: vec![(0x00000, 0x07FFF)],
            checksum_offset: Some(0x3FFF8),
            ecm_id_hex: Some("0101250501".into()),
        },
        MotoEcuProfile {
            id: "honda_cbr250r_kyj".into(),
            make: "Honda".into(),
            model: "CBR 250 R".into(),
            years: "2011-2014".into(),
            part_number: "38770-KYJ-922".into(),
            mcu: "Keihin 56K".into(),
            rom_size_bytes: 56 * 1024,
            writable_start: 0x04000,
            writable_end_incl: 0x0DFFF,
            protected_ranges: vec![(0x00000, 0x03FFF)],
            checksum_offset: Some(0x0DFEF),
            ecm_id_hex: Some("0100B80502".into()),
        },
        MotoEcuProfile {
            id: "honda_msx125_k26".into(),
            make: "Honda".into(),
            model: "MSX 125 / Grom".into(),
            years: "2013-2019".into(),
            part_number: "38770-K26-911".into(),
            mcu: "Keihin 56K/64K".into(),
            rom_size_bytes: 64 * 1024,
            writable_start: 0x04000,
            writable_end_incl: 0x0FFFF,
            protected_ranges: vec![(0x00000, 0x03FFF)],
            checksum_offset: None,
            ecm_id_hex: Some("0101350501".into()),
        },
        MotoEcuProfile {
            id: "honda_vfr1200f".into(),
            make: "Honda".into(),
            model: "VFR 1200 F".into(),
            years: "2010-2011".into(),
            part_number: "38770-MGE-D02".into(),
            mcu: "Keihin 512K".into(),
            rom_size_bytes: 512 * 1024,
            writable_start: 0x08000,
            writable_end_incl: 0x7FFFF,
            protected_ranges: vec![(0x00000, 0x07FFF)],
            checksum_offset: Some(0x7FFF8),
            ecm_id_hex: Some("0100AA0102".into()),
        },
    ]
}

pub fn find_moto_ecu_profile(query: &str) -> Option<MotoEcuProfile> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return None;
    }
    list_moto_ecu_profiles().into_iter().find(|p| {
        p.id.to_ascii_lowercase() == q
            || p.part_number.to_ascii_lowercase().contains(&q)
            || p.model.to_ascii_lowercase().contains(&q)
    })
}

/// Validates that `[write_start, write_start + write_len)` does NOT intersect a protected boot/ROM region
/// for a known motorcycle profile (`Err` with exact boundary explanation if unsafe).
pub fn check_moto_write_boundaries(
    profile_id: &str,
    write_start: u32,
    write_len: usize,
) -> Result<(), String> {
    if write_len == 0 {
        return Err("Write length must be > 0".to_string());
    }
    let profile = find_moto_ecu_profile(profile_id)
        .ok_or_else(|| format!("Unknown motorcycle ECU profile '{profile_id}'"))?;
    let last_off = u32::try_from(write_len - 1).map_err(|_| "Write range overflow".to_string())?;
    let write_end = write_start
        .checked_add(last_off)
        .ok_or_else(|| "Write range overflow".to_string())?;

    for &(ps, pe) in &profile.protected_ranges {
        if write_start <= pe && write_end >= ps {
            return Err(format!(
                "{} ({}) write 0x{:X}..=0x{:X} overlaps protected bootloader/calibration guard 0x{:X}..=0x{:X}",
                profile.make, profile.model, write_start, write_end, ps, pe
            ));
        }
    }
    if write_start < profile.writable_start || write_end > profile.writable_end_incl {
        return Err(format!(
            "{} ({}) allows writes only within 0x{:X}..=0x{:X} (requested 0x{:X}..=0x{:X})",
            profile.make, profile.model, profile.writable_start, profile.writable_end_incl, write_start, write_end
        ));
    }
    Ok(())
}
