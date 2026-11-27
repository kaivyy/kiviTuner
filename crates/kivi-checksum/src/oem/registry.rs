//! Unified OEM Checksum Registry covering both new OEM engines (`oem::*`) and existing
//! motorcycle engines (`multi_ecu::*`) without duplicating any implementation.

use super::{
    bosch_edc15, bosch_edc16, gm_pcm, siemens_bmw, subaru_hitachi_mitsu, vag_haldex_me7,
    OemAlgorithmInfo, OemChecksumBlock, OemChecksumReport,
};
use crate::multi_ecu::{
    auto_detect_checksums as auto_detect_moto, list_supported_checksum_engines,
    repair_checksum_by_id as repair_moto_by_id, verify_checksum_by_id as verify_moto_by_id,
    ChecksumRepairMode, DetectedChecksumMatch,
};

fn convert_moto_match(m: DetectedChecksumMatch) -> OemChecksumReport {
    let supports_repair = !matches!(m.algorithm_id.as_str(), "none");
    let blocks = m
        .blocks
        .into_iter()
        .map(|b| OemChecksumBlock {
            label: b.label,
            start: b.start,
            end: b.end,
            store_offset: Some(b.offset),
            stored: Some(b.stored),
            computed: Some(b.computed),
            width_bits: b.width_bits,
            correctable: supports_repair,
            ok: b.ok,
            status: if b.ok { "ok" } else { "stale" }.to_string(),
        })
        .collect();
    OemChecksumReport {
        algorithm_id: m.algorithm_id,
        algorithm_name: m.algorithm_name,
        family: "Motorcycle OEM".to_string(),
        variant: m.status,
        blocks,
        all_ok: m.all_ok,
        notes: if m.notes.is_empty() { vec![] } else { vec![m.notes] },
    }
}

/// Lists every OEM and powersports checksum engine registered in `kivi-checksum`.
pub fn list_oem_algorithms() -> Vec<OemAlgorithmInfo> {
    let mut out: Vec<OemAlgorithmInfo> = list_supported_checksum_engines()
        .into_iter()
        .map(|info| OemAlgorithmInfo {
            id: info.id,
            name: info.name,
            family: "Motorcycle OEM".to_string(),
            supports_repair: true,
            description: info.description,
        })
        .collect();

    out.extend([
        OemAlgorithmInfo {
            id: "bmw_ms41".to_string(),
            name: "BMW Siemens MS41".to_string(),
            family: "Siemens BMW".to_string(),
            supports_repair: true,
            description: "256 KiB full / 24 KiB partial CRC-16/ARC boot & calibration tables".to_string(),
        },
        OemAlgorithmInfo {
            id: "bmw_ms43".to_string(),
            name: "BMW Siemens MS43".to_string(),
            family: "Siemens BMW".to_string(),
            supports_repair: true,
            description: "512 KiB boot/program/calibration CRC-16/ARC descriptors".to_string(),
        },
        OemAlgorithmInfo {
            id: "bmw_gs20".to_string(),
            name: "BMW Siemens GS20 TCU".to_string(),
            family: "Siemens BMW".to_string(),
            supports_repair: true,
            description: "64 KiB calibration (BE16 @0x0D) / 256 KiB program CRC-16/ARC".to_string(),
        },
        OemAlgorithmInfo {
            id: "bmw_smg2".to_string(),
            name: "BMW Siemens SMG II TCU".to_string(),
            family: "Siemens BMW".to_string(),
            supports_repair: true,
            description: "32 KiB data CRC-16/ARC init 0x7878 @8320".to_string(),
        },
        OemAlgorithmInfo {
            id: "mini_ems2k".to_string(),
            name: "Mini Siemens EMS2000".to_string(),
            family: "Siemens BMW".to_string(),
            supports_repair: true,
            description: "512 KiB 3x CRC-16/ARC (calibration @0x77FE0 repairable)".to_string(),
        },
        OemAlgorithmInfo {
            id: "gm_pcm".to_string(),
            name: "GM Delco PCM (P01/P59/P08/P10/P12/E38/E39/E92/E54)".to_string(),
            family: "GM Delco".to_string(),
            supports_repair: true,
            description: "Auto-detects GM segment word sums and E38/E39/E92 CRC-16 CVNs".to_string(),
        },
        OemAlgorithmInfo {
            id: "bosch_edc15".to_string(),
            name: "Bosch VAG EDC15P/V/VM".to_string(),
            family: "Bosch EDC15".to_string(),
            supports_repair: true,
            description: "TDI v4.1 / v4.1v2 / v4.1-2002 rotating XOR/sub zone checksums".to_string(),
        },
        OemAlgorithmInfo {
            id: "bosch_edc16".to_string(),
            name: "Bosch EDC16U (FADECAFE)".to_string(),
            family: "Bosch EDC16".to_string(),
            supports_repair: true,
            description: "Descriptor-based BE32 sum = 0xD01FE500 across 0x100-aligned regions".to_string(),
        },
        OemAlgorithmInfo {
            id: "vag_haldex_cal".to_string(),
            name: "VAG Haldex 4Motion CAL (block 2)".to_string(),
            family: "VAG Haldex".to_string(),
            supports_repair: true,
            description: "16-bit LE one's-complement word sum stored @0x18".to_string(),
        },
        OemAlgorithmInfo {
            id: "vag_haldex_asw".to_string(),
            name: "VAG Haldex 4Motion ASW (block 3)".to_string(),
            family: "VAG Haldex".to_string(),
            supports_repair: true,
            description: "16-bit LE one's-complement word sum stored @0x208".to_string(),
        },
        OemAlgorithmInfo {
            id: "vag_me7xx_crc".to_string(),
            name: "VAG Bosch ME7.XX CRC-32".to_string(),
            family: "Bosch ME7".to_string(),
            supports_repair: true,
            description: "IronFelix C166 signature-driven 3-zone ME7.x CRC-32 verifier & repair".to_string(),
        },
        OemAlgorithmInfo {
            id: "bosch_me3x".to_string(),
            name: "Bosch M3.x / M5.x".to_string(),
            family: "Bosch Legacy".to_string(),
            supports_repair: true,
            description: "128/256 KiB page-marker byte sum verifier & repair".to_string(),
        },
        OemAlgorithmInfo {
            id: "bosch_m797".to_string(),
            name: "Hyundai Bosch M7.9.7".to_string(),
            family: "Bosch M7.9".to_string(),
            supports_repair: true,
            description: "512 KiB 5-zone LE16 sum @0x7FFE0 verifier & repair".to_string(),
        },
        OemAlgorithmInfo {
            id: "bosch_china_m797".to_string(),
            name: "China Bosch M7.9.7".to_string(),
            family: "Bosch M7.9".to_string(),
            supports_repair: true,
            description: "1 MiB 2-zone LE16 sum @0xFFFE8 verifier & repair".to_string(),
        },
        OemAlgorithmInfo {
            id: "bosch_me745".to_string(),
            name: "Citroen Bosch ME7.4.5".to_string(),
            family: "Bosch ME7".to_string(),
            supports_repair: true,
            description: "832 KiB page blocks + 3x CRC-32 verifier & repair".to_string(),
        },
        OemAlgorithmInfo {
            id: "sagem_samand".to_string(),
            name: "Sagem Iran Khodro Samand".to_string(),
            family: "Sagem".to_string(),
            supports_repair: true,
            description: "832 KiB dual sum8 main & tail verifier & repair".to_string(),
        },
        OemAlgorithmInfo {
            id: "subaru_denso_sh7xxx".to_string(),
            name: "Subaru Denso SH7xxx / Diesel".to_string(),
            family: "Subaru Denso".to_string(),
            supports_repair: true,
            description: "Auto-discovered 12-byte BE descriptors (inclusive end + diesel overrides)".to_string(),
        },
        OemAlgorithmInfo {
            id: "subaru_hitachi_m32r_kline".to_string(),
            name: "Subaru Hitachi M32R K-Line".to_string(),
            family: "Subaru Hitachi".to_string(),
            supports_repair: true,
            description: "512 KiB sum8/xor8 + 0x5AA5 balance @0x7FFFA".to_string(),
        },
        OemAlgorithmInfo {
            id: "subaru_hitachi_m32r_can".to_string(),
            name: "Subaru Hitachi M32R CAN".to_string(),
            family: "Subaru Hitachi".to_string(),
            supports_repair: true,
            description: "512 KiB multi-zone BE32 sum/XOR + 0x5AA5 balance".to_string(),
        },
        OemAlgorithmInfo {
            id: "subaru_hitachi_sh7058".to_string(),
            name: "Subaru Hitachi SH7058 CAN".to_string(),
            family: "Subaru Hitachi".to_string(),
            supports_repair: true,
            description: "1 MiB multi-zone BE32 sum/XOR + 0x5AA5A55A balance @0xFFFF8".to_string(),
        },
        OemAlgorithmInfo {
            id: "subaru_hitachi_sh72543r".to_string(),
            name: "Subaru Hitachi SH72543R".to_string(),
            family: "Subaru Hitachi".to_string(),
            supports_repair: true,
            description: "2 MiB BE16 sum = 0x5AA5 balanced @0x1FFFFE".to_string(),
        },
        OemAlgorithmInfo {
            id: "subaru_hitachi_tcu_m32r".to_string(),
            name: "Subaru Hitachi M32R CAN TCU".to_string(),
            family: "Subaru TCU".to_string(),
            supports_repair: true,
            description: "BE32 @0x8020 balance + two's complement @0x8000".to_string(),
        },
        OemAlgorithmInfo {
            id: "subaru_denso_tcu_sh7055".to_string(),
            name: "Subaru Denso SH7055 TCU".to_string(),
            family: "Subaru TCU".to_string(),
            supports_repair: true,
            description: "512 KiB 12-area BE16 sum = 0x5AA5 @0x7FFF4".to_string(),
        },
        OemAlgorithmInfo {
            id: "mitsu_mh8104_tcu".to_string(),
            name: "Mitsubishi MH8104 CAN CVT TCU".to_string(),
            family: "Mitsubishi TCU".to_string(),
            supports_repair: true,
            description: "512 KiB BE32 balance @0x81FC -> 0x5AA45AAB".to_string(),
        },
    ]);
    out
}

/// Auto-detects matching checksum schemes (motorcycle + automotive OEM) for `rom`.
pub fn auto_detect_oem_checksums(rom: &[u8]) -> Vec<OemChecksumReport> {
    let mut out: Vec<OemChecksumReport> = auto_detect_moto(rom)
        .into_iter()
        .map(convert_moto_match)
        .collect();

    for opt in [
        siemens_bmw::verify_ms41(rom),
        siemens_bmw::verify_ms43(rom),
        siemens_bmw::auto_detect_gs20(rom),
        siemens_bmw::auto_detect_smg2(rom),
        siemens_bmw::verify_ems2k(rom),
        gm_pcm::verify_gm_pcm(rom),
        bosch_edc15::verify_edc15(rom),
        bosch_edc16::verify_edc16(rom),
        vag_haldex_me7::verify_vag_me7xx_crc(rom),
        vag_haldex_me7::verify_bosch_me3x(rom),
        vag_haldex_me7::verify_hyundai_m797(rom),
        vag_haldex_me7::verify_china_m797(rom),
        vag_haldex_me7::verify_citroen_me745(rom),
        vag_haldex_me7::verify_sagem_samand(rom),
        subaru_hitachi_mitsu::verify_subaru_denso_auto(rom),
    ]
    .into_iter()
    .flatten()
    {
        out.push(opt);
    }
    out
}

pub fn verify_oem_by_id(rom: &[u8], algorithm_id: &str) -> Result<OemChecksumReport, String> {
    if let Ok(m) = verify_moto_by_id(rom, algorithm_id) {
        return Ok(convert_moto_match(m));
    }
    let opt = match algorithm_id {
        "bmw_ms41" => siemens_bmw::verify_ms41(rom),
        "bmw_ms43" => siemens_bmw::verify_ms43(rom),
        "bmw_gs20" => siemens_bmw::verify_gs20(rom),
        "bmw_smg2" => siemens_bmw::verify_smg2(rom),
        "mini_ems2k" => siemens_bmw::verify_ems2k(rom),
        "gm_pcm" | "gm_p01" | "gm_p59" | "gm_e38" | "gm_e39" | "gm_e92" => gm_pcm::verify_gm_pcm(rom),
        "bosch_edc15" => bosch_edc15::verify_edc15(rom),
        "bosch_edc16" => bosch_edc16::verify_edc16(rom),
        "vag_haldex_cal" => return vag_haldex_me7::verify_haldex_block(rom, 2),
        "vag_haldex_asw" => return vag_haldex_me7::verify_haldex_block(rom, 3),
        "vag_me7xx_crc" => vag_haldex_me7::verify_vag_me7xx_crc(rom),
        "bosch_me3x" => vag_haldex_me7::verify_bosch_me3x(rom),
        "bosch_m797" => vag_haldex_me7::verify_hyundai_m797(rom),
        "bosch_china_m797" => vag_haldex_me7::verify_china_m797(rom),
        "bosch_me745" => vag_haldex_me7::verify_citroen_me745(rom),
        "sagem_samand" => vag_haldex_me7::verify_sagem_samand(rom),
        "subaru_denso_sh7xxx" => subaru_hitachi_mitsu::verify_subaru_denso_auto(rom),
        "subaru_hitachi_m32r_kline" => return subaru_hitachi_mitsu::verify_hitachi_m32r_kline(rom),
        "subaru_hitachi_m32r_can" => return subaru_hitachi_mitsu::verify_hitachi_m32r_can(rom),
        "subaru_hitachi_sh7058" => return subaru_hitachi_mitsu::verify_hitachi_sh7058(rom),
        "subaru_hitachi_sh72543r" => return subaru_hitachi_mitsu::verify_hitachi_sh72543r(rom),
        "subaru_hitachi_tcu_m32r" => return subaru_hitachi_mitsu::verify_hitachi_tcu_m32r(rom),
        "subaru_denso_tcu_sh7055" => return subaru_hitachi_mitsu::verify_subaru_denso_tcu_sh7055(rom),
        "mitsu_mh8104_tcu" => return subaru_hitachi_mitsu::verify_mitsu_mh8104_tcu(rom),
        other => return Err(format!("unknown OEM checksum algorithm '{other}'")),
    };
    opt.ok_or_else(|| format!("ROM does not match layout signature for '{algorithm_id}'"))
}

pub fn repair_oem_by_id(rom: &[u8], algorithm_id: &str) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if let Ok((bytes, m)) = repair_moto_by_id(rom, algorithm_id, ChecksumRepairMode::UpdateChecksum) {
        return Ok((bytes, convert_moto_match(m)));
    }
    match algorithm_id {
        "bmw_ms41" => siemens_bmw::repair_ms41(rom),
        "bmw_ms43" => siemens_bmw::repair_ms43(rom),
        "bmw_gs20" => siemens_bmw::repair_gs20(rom),
        "bmw_smg2" => siemens_bmw::repair_smg2(rom),
        "mini_ems2k" => siemens_bmw::repair_ems2k(rom),
        "gm_pcm" | "gm_p01" | "gm_p59" | "gm_e38" | "gm_e39" | "gm_e92" => gm_pcm::repair_gm_pcm(rom),
        "bosch_edc15" => bosch_edc15::repair_edc15(rom),
        "bosch_edc16" => bosch_edc16::repair_edc16(rom),
        "vag_haldex_cal" => vag_haldex_me7::repair_haldex_block(rom, 2),
        "vag_haldex_asw" => vag_haldex_me7::repair_haldex_block(rom, 3),
        "vag_me7xx_crc" => vag_haldex_me7::repair_vag_me7xx_crc(rom),
        "bosch_me3x" => vag_haldex_me7::repair_bosch_me3x(rom),
        "bosch_m797" => vag_haldex_me7::repair_hyundai_m797(rom),
        "bosch_china_m797" => vag_haldex_me7::repair_china_m797(rom),
        "bosch_me745" => vag_haldex_me7::repair_citroen_me745(rom),
        "sagem_samand" => vag_haldex_me7::repair_sagem_samand(rom),
        "subaru_denso_sh7xxx" => subaru_hitachi_mitsu::repair_subaru_denso_auto(rom),
        "subaru_hitachi_m32r_kline" => subaru_hitachi_mitsu::repair_hitachi_m32r_kline(rom),
        "subaru_hitachi_m32r_can" => subaru_hitachi_mitsu::repair_hitachi_m32r_can(rom),
        "subaru_hitachi_sh7058" => subaru_hitachi_mitsu::repair_hitachi_sh7058(rom),
        "subaru_hitachi_sh72543r" => subaru_hitachi_mitsu::repair_hitachi_sh72543r(rom),
        "subaru_hitachi_tcu_m32r" => subaru_hitachi_mitsu::repair_hitachi_tcu_m32r(rom),
        "subaru_denso_tcu_sh7055" => subaru_hitachi_mitsu::repair_subaru_denso_tcu_sh7055(rom),
        "mitsu_mh8104_tcu" => subaru_hitachi_mitsu::repair_mitsu_mh8104_tcu(rom),
        other => Err(format!("repair not supported or unknown algorithm '{other}'")),
    }
}
