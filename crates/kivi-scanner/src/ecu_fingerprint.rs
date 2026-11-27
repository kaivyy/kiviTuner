//! Multi-OEM ECU fingerprint & binary ROM identification engine.
//!
//! Combines BMW MS41/MS42/MS43/MS45/GS20/GS8604/SMG patterns (`scan_bin_cal_ids.py`),
//! Bosch EDC15P/VM, EDC16U1/U31/U34, EDC17/MED17, ME7/MED9 (`ZedSuite/ecu_identifier.rs`),
//! Subaru/Toyota/Honda/Yamaha Denso SH705x (`5AA5A55A`), Hitachi M32R/SH7058/SH72543R,
//! Mitsubishi MH8104, Keihin MPC55xx/SH7058, and GM Delco P01/P59/E38/E39/E67/E92.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcuFingerprint {
    pub manufacturer: String,
    pub family: String,
    pub variant: Option<String>,
    pub cal_ids: Vec<String>,
    pub hw_numbers: Vec<String>,
    pub sw_numbers: Vec<String>,
    pub part_numbers: Vec<String>,
    pub confidence: f32,
    pub evidence: Vec<String>,
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn extract_ascii_runs(data: &[u8], min_len: usize, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for &b in data {
        if (0x20..=0x7E).contains(&b) {
            cur.push(b);
        } else {
            if cur.len() >= min_len {
                out.push(String::from_utf8_lossy(&cur).trim().to_string());
                if out.len() >= limit {
                    return out;
                }
            }
            cur.clear();
        }
    }
    if cur.len() >= min_len && out.len() < limit {
        out.push(String::from_utf8_lossy(&cur).trim().to_string());
    }
    out
}

fn extract_prefixed_digits(runs: &[String], prefix: &str, total_len: usize) -> Vec<String> {
    let mut res = Vec::new();
    for s in runs {
        let mut idx = 0usize;
        while let Some(rel) = s[idx..].find(prefix) {
            let start = idx + rel;
            if start + total_len <= s.len()
                && s.as_bytes()[start..start + total_len]
                    .iter()
                    .all(|b| b.is_ascii_digit())
            {
                let cand = s[start..start + total_len].to_string();
                if !res.contains(&cand) {
                    res.push(cand);
                }
            }
            idx = start + 1;
        }
    }
    res
}

fn collect_token_matches<F: Fn(&str) -> bool>(runs: &[String], pred: F) -> Vec<String> {
    let mut out = Vec::new();
    for s in runs {
        for tok in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-') {
            let clean = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            if pred(clean) && !out.contains(&clean.to_string()) {
                out.push(clean.to_string());
            }
        }
    }
    out
}

/// Identifies ECU manufacturer, family, and calibration/HW/SW IDs from raw flash bytes.
pub fn identify_ecu_rom(rom: &[u8]) -> EcuFingerprint {
    let size = rom.len();
    let runs = extract_ascii_runs(rom, 5, 4096);
    let mut hw_numbers = extract_prefixed_digits(&runs, "0281", 10);
    for h in extract_prefixed_digits(&runs, "0261", 10) {
        if !hw_numbers.contains(&h) {
            hw_numbers.push(h);
        }
    }
    for h in extract_prefixed_digits(&runs, "0265", 10) {
        if !hw_numbers.contains(&h) {
            hw_numbers.push(h);
        }
    }
    let mut sw_numbers = extract_prefixed_digits(&runs, "1037", 10);
    for s in extract_prefixed_digits(&runs, "1039", 10) {
        if !sw_numbers.contains(&s) {
            sw_numbers.push(s);
        }
    }

    // 1. BMW Siemens MS41 / MS42 / MS43 / MS45 / GS20 / SMG2 / Mini EMS2000
    if (size == 24 * 1024 || size == 256 * 1024) && contains_bytes(rom, &[0x4E, 0x00, 0xFF, 0xFF]) {
        return EcuFingerprint {
            manufacturer: "Siemens".to_string(),
            family: "BMW MS41".to_string(),
            variant: Some(if size == 24 * 1024 { "24 KiB partial" } else { "256 KiB full" }.to_string()),
            cal_ids: collect_token_matches(&runs, |t| t.starts_with("MS41") || t.starts_with("1429")),
            hw_numbers,
            sw_numbers,
            part_numbers: Vec::new(),
            confidence: 0.94,
            evidence: vec!["MS41 cal magic [4E 00 FF FF] + size match".to_string()],
        };
    }
    if (size == 0x80000 || size == 0x10000) && contains_bytes(rom, b"MS43") {
        let cals = collect_token_matches(&runs, |t| t.starts_with("4300") && t.len() >= 6);
        return EcuFingerprint {
            manufacturer: "Siemens".to_string(),
            family: "BMW MS43".to_string(),
            variant: Some(format!("{} KiB", size / 1024)),
            cal_ids: cals,
            hw_numbers,
            sw_numbers,
            part_numbers: Vec::new(),
            confidence: 0.93,
            evidence: vec!["MS43 signature + Siemens calibration strings".to_string()],
        };
    }
    if (size == 0x80000 || size == 0x8000) && runs.iter().any(|r| r.contains("0110")) {
        let cals = collect_token_matches(&runs, |t| t.starts_with("0110") && t.len() >= 6);
        return EcuFingerprint {
            manufacturer: "Siemens".to_string(),
            family: "BMW MS42".to_string(),
            variant: Some(format!("{} KiB", size / 1024)),
            cal_ids: cals,
            hw_numbers,
            sw_numbers,
            part_numbers: Vec::new(),
            confidence: 0.88,
            evidence: vec!["MS42 0110xx calibration ID pattern".to_string()],
        };
    }
    if (size == 0x10000 && contains_bytes(rom, b"GS20")) || contains_bytes(rom, b"GS860") {
        return EcuFingerprint {
            manufacturer: "Siemens / Bosch".to_string(),
            family: "BMW EGS/GS20 TCU".to_string(),
            variant: None,
            cal_ids: collect_token_matches(&runs, |t| t.starts_with("GS20") || t.starts_with("GS86")),
            hw_numbers,
            sw_numbers,
            part_numbers: Vec::new(),
            confidence: 0.88,
            evidence: vec!["BMW transmission controller identifier".to_string()],
        };
    }

    // 2. Bosch TriCore / EDC17 / MED17
    if contains_bytes(rom, b"EDC17") || contains_bytes(rom, b"MED17") {
        let fam = if contains_bytes(rom, b"MED17") { "Bosch MED17" } else { "Bosch EDC17" };
        let vag = collect_token_matches(&runs, |t| {
            t.len() >= 9 && t.len() <= 12 && t.chars().take(3).all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
                && (t.starts_with("03L") || t.starts_with("04L") || t.starts_with("8K") || t.starts_with("4G") || t.starts_with("06"))
        });
        return EcuFingerprint {
            manufacturer: "Bosch".to_string(),
            family: fam.to_string(),
            variant: Some(format!("{} KiB TriCore", size / 1024)),
            cal_ids: Vec::new(),
            hw_numbers,
            sw_numbers,
            part_numbers: vag,
            confidence: 0.92,
            evidence: vec![format!("TriCore signature '{fam}'")],
        };
    }

    // 3. Bosch EDC15P / VM & EDC16
    if contains_bytes(rom, b"0281") || contains_bytes(rom, b"EDC16") || contains_bytes(rom, b"EDC15") {
        let has_v41 = contains_bytes(rom, &[0x67, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, b'V', b'4', b'.', b'1']);
        let fam = if has_v41 || contains_bytes(rom, b"EDC15") || contains_bytes(rom, b"038906019") {
            if size == 0x10_0000 { "Bosch EDC15VM+" } else { "Bosch EDC15P" }
        } else if contains_bytes(rom, &[0xFA, 0xDE, 0xCA, 0xFE, 0xCA, 0xFE, 0xAF, 0xFE]) || contains_bytes(rom, b"EDC16") {
            match size {
                0x20_0000 => "Bosch EDC16U34",
                0x10_0000 => "Bosch EDC16U31",
                _ => "Bosch EDC16U1",
            }
        } else {
            "Bosch Diesel"
        };
        let vag = collect_token_matches(&runs, |t| {
            t.starts_with("038906") || t.starts_with("03G906") || t.starts_with("074906") || t.starts_with("8E0907")
        });
        return EcuFingerprint {
            manufacturer: "Bosch".to_string(),
            family: fam.to_string(),
            variant: Some(format!("{} KiB", size / 1024)),
            cal_ids: Vec::new(),
            hw_numbers,
            sw_numbers,
            part_numbers: vag,
            confidence: 0.90,
            evidence: vec![format!("Bosch diesel HW/SW prefix & layout ({fam})")],
        };
    }

    // 4. Bosch ME7 / MED9 / Continental Simos / DQ250 / DQ381
    if contains_bytes(rom, b"ME7.") || contains_bytes(rom, b"MED9") || contains_bytes(rom, b"0261") {
        let fam = if contains_bytes(rom, b"MED9") {
            "Bosch MED9"
        } else if contains_bytes(rom, b"ME7.") {
            "Bosch ME7"
        } else {
            "Bosch Motronic Petrol"
        };
        return EcuFingerprint {
            manufacturer: "Bosch".to_string(),
            family: fam.to_string(),
            variant: Some(format!("{} KiB", size / 1024)),
            cal_ids: Vec::new(),
            hw_numbers,
            sw_numbers,
            part_numbers: collect_token_matches(&runs, |t| t.contains("907551") || t.contains("906018") || t.contains("906032")),
            confidence: 0.87,
            evidence: vec![format!("Motronic string/HW signature ({fam})")],
        };
    }
    if contains_bytes(rom, b"SIMOS") || contains_bytes(rom, b"DQ250") || contains_bytes(rom, b"DQ381") {
        return EcuFingerprint {
            manufacturer: "Siemens / Continental".to_string(),
            family: if contains_bytes(rom, b"DQ381") {
                "VAG DQ381 TCU"
            } else if contains_bytes(rom, b"DQ250") {
                "VAG DQ250 DSG"
            } else {
                "VAG Simos"
            }
            .to_string(),
            variant: None,
            cal_ids: Vec::new(),
            hw_numbers,
            sw_numbers,
            part_numbers: Vec::new(),
            confidence: 0.89,
            evidence: vec!["Simos/DSG ASCII signature".to_string()],
        };
    }

    // 5. Denso SH705x / SH725xx (Subaru/Toyota/Honda/Yamaha/Mazda)
    if contains_bytes(rom, &[0x5A, 0xA5, 0xA5, 0x5A]) {
        return EcuFingerprint {
            manufacturer: "Denso".to_string(),
            family: "Denso SuperH SH705x/SH725xx".to_string(),
            variant: Some(format!("{} KiB", size / 1024)),
            cal_ids: collect_token_matches(&runs, |t| t.len() == 8 && t.starts_with("AZ") || t.starts_with("A2Z")),
            hw_numbers: Vec::new(),
            sw_numbers: Vec::new(),
            part_numbers: collect_token_matches(&runs, |t| t.starts_with("38770") || t.starts_with("22611")),
            confidence: 0.91,
            evidence: vec!["Denso checksum magic 0x5AA5A55A".to_string()],
        };
    }

    // 6. Honda / Keihin
    if contains_bytes(rom, b"38770") || contains_bytes(rom, b"KEIHIN") {
        return EcuFingerprint {
            manufacturer: "Keihin".to_string(),
            family: "Honda PGM-FI / Keihin".to_string(),
            variant: Some(format!("{} KiB", size / 1024)),
            cal_ids: Vec::new(),
            hw_numbers: Vec::new(),
            sw_numbers: Vec::new(),
            part_numbers: collect_token_matches(&runs, |t| t.starts_with("38770")),
            confidence: 0.89,
            evidence: vec!["Honda 38770-xxx / Keihin string".to_string()],
        };
    }

    // 7. GM Delco P01/P59/E38/E92
    if matches!(size, 0x80000 | 0x10_0000 | 0x20_0000 | 0x30_0000 | 0x40_0000)
        && contains_bytes(rom, b"Delco")
    {
        return EcuFingerprint {
            manufacturer: "GM / Delco".to_string(),
            family: match size {
                0x80000 => "GM P01 (512K)",
                0x10_0000 => "GM P59 (1M)",
                0x20_0000 => "GM E38 (2M)",
                0x30_0000 => "GM E39 (3M)",
                _ => "GM E92 (4M)",
            }
            .to_string(),
            variant: None,
            cal_ids: Vec::new(),
            hw_numbers: Vec::new(),
            sw_numbers: Vec::new(),
            part_numbers: Vec::new(),
            confidence: 0.84,
            evidence: vec!["GM Delco marker + PCM flash size".to_string()],
        };
    }

    EcuFingerprint {
        manufacturer: "Unknown".to_string(),
        family: "Unclassified Binary".to_string(),
        variant: Some(format!("{} KiB", size / 1024)),
        cal_ids: Vec::new(),
        hw_numbers,
        sw_numbers,
        part_numbers: Vec::new(),
        confidence: 0.10,
        evidence: Vec::new(),
    }
}
