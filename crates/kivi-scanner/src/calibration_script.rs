//! Declarative pattern-based calibration patch and auto map-transfer script engine.
//!
//! Locates targets via wildcard byte signatures ([`MaskedSignatureScanner`]) so
//! calibration patches and map value transfers remain portable across OS revisions
//! without hardcoding raw offsets when signatures stay unique.

use crate::pattern_search::MaskedSignatureScanner;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

pub const MAX_SCRIPT_RULES: usize = 64;
pub const MAX_TRANSFER_CELLS: usize = 65_536;

fn default_require_unique() -> bool {
    true
}

fn default_scale() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferMode {
    Absolute,
    AdditiveDelta,
    PercentageDelta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum ScriptAction {
    PatchBytes {
        #[serde(default)]
        expected_hex: Option<String>,
        replacement_hex: String,
    },
    TransferMap {
        rows: usize,
        cols: usize,
        cell_bits: u8,
        little_endian: bool,
        is_signed: bool,
        #[serde(default = "default_scale")]
        scale: f64,
        #[serde(default)]
        offset: f64,
        mode: TransferMode,
        values: Vec<f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptRule {
    pub name: String,
    pub signature: String,
    #[serde(default)]
    pub relative_offset: i64,
    #[serde(default = "default_require_unique")]
    pub require_unique_match: bool,
    pub action: ScriptAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationScript {
    pub schema_version: u32,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub target_hint: Option<String>,
    pub rules: Vec<ScriptRule>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleDryRunStatus {
    Ready,
    NotFound,
    Ambiguous,
    ExpectedMismatch,
    OutOfBounds,
    InvalidRule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleDryRunOutcome {
    pub rule_name: String,
    pub status: RuleDryRunStatus,
    pub match_count: usize,
    pub matched_offsets: Vec<usize>,
    pub target_offset: Option<usize>,
    pub bytes_changed: usize,
    pub before_hex: String,
    pub after_hex: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalibrationScriptDryRun {
    pub script_name: String,
    pub can_apply: bool,
    pub total_bytes_changed: usize,
    pub rules: Vec<RuleDryRunOutcome>,
}

/// Returns an example `.kivi-script.json` template demonstrating byte patching and map transfer.
pub fn sample_calibration_script_json() -> String {
    serde_json::to_string_pretty(&CalibrationScript {
        schema_version: 1,
        name: "Example Stage-1 Rev Limit & Fuel Trim".into(),
        description: "Portable wildcard-signature patch + additive map transfer.".into(),
        target_hint: Some("Adjust signatures for your target ROM binary".into()),
        rules: vec![
            ScriptRule {
                name: "Rev Limiter Scalar".into(),
                signature: "AA BB ?? DD 1E 00".into(),
                relative_offset: 4,
                require_unique_match: true,
                action: ScriptAction::PatchBytes {
                    expected_hex: Some("1E 00".into()),
                    replacement_hex: "22 00".into(),
                },
            },
            ScriptRule {
                name: "2x2 Fuel Enrichment Zone".into(),
                signature: "DE AD BE EF ?? ?? ?? ??".into(),
                relative_offset: 4,
                require_unique_match: true,
                action: ScriptAction::TransferMap {
                    rows: 2,
                    cols: 2,
                    cell_bits: 8,
                    little_endian: false,
                    is_signed: false,
                    scale: 1.0,
                    offset: 0.0,
                    mode: TransferMode::AdditiveDelta,
                    values: vec![2.0, 2.0, 4.0, 4.0],
                },
            },
        ],
    })
    .unwrap_or_else(|_| "{}".to_string())
}

fn parse_hex_bytes(hex: &str) -> Result<Vec<u8>, String> {
    let clean: String = hex
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if clean.is_empty() || !clean.len().is_multiple_of(2) {
        return Err("Hex payload must contain a non-empty even number of digits".to_string());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for chunk in clean.as_bytes().as_chunks::<2>().0 {
        let pair = std::str::from_utf8(chunk).map_err(|_| "Invalid UTF-8 in hex".to_string())?;
        out.push(
            u8::from_str_radix(pair, 16)
                .map_err(|_| format!("Invalid hexadecimal byte '{pair}'"))?,
        );
    }
    Ok(out)
}

fn encode_hex_spaced(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(3));
    for (idx, &b) in bytes.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{b:02X}");
    }
    out
}

/// Parses and structurally validates a declarative [`CalibrationScript`] JSON document.
pub fn parse_calibration_script(json: &str) -> Result<CalibrationScript, String> {
    let script: CalibrationScript =
        serde_json::from_str(json).map_err(|e| format!("Invalid calibration script JSON: {e}"))?;
    if script.schema_version != 1 {
        return Err(format!(
            "Unsupported script schema_version {} (expected 1)",
            script.schema_version
        ));
    }
    if script.name.trim().is_empty() {
        return Err("Script 'name' cannot be empty".to_string());
    }
    if script.rules.is_empty() {
        return Err("Script must contain at least one rule".to_string());
    }
    if script.rules.len() > MAX_SCRIPT_RULES {
        return Err(format!(
            "Script defines {} rules (max {MAX_SCRIPT_RULES})",
            script.rules.len()
        ));
    }
    for rule in &script.rules {
        validate_script_rule(rule)?;
    }
    Ok(script)
}

fn validate_script_rule(rule: &ScriptRule) -> Result<(), String> {
    if rule.name.trim().is_empty() {
        return Err("Rule 'name' cannot be empty".to_string());
    }
    MaskedSignatureScanner::parse_pattern(&rule.signature)
        .map_err(|e| format!("Rule '{}': invalid signature: {e}", rule.name))?;
    match &rule.action {
        ScriptAction::PatchBytes {
            expected_hex,
            replacement_hex,
        } => {
            let rep = parse_hex_bytes(replacement_hex)
                .map_err(|e| format!("Rule '{}': replacement_hex error: {e}", rule.name))?;
            if let Some(exp_str) = expected_hex.as_ref().filter(|s| !s.trim().is_empty()) {
                let exp = parse_hex_bytes(exp_str)
                    .map_err(|e| format!("Rule '{}': expected_hex error: {e}", rule.name))?;
                if exp.len() != rep.len() {
                    return Err(format!(
                        "Rule '{}': expected_hex ({} bytes) must match replacement_hex ({} bytes)",
                        rule.name,
                        exp.len(),
                        rep.len()
                    ));
                }
            }
        }
        ScriptAction::TransferMap {
            rows,
            cols,
            cell_bits,
            scale,
            offset,
            values,
            ..
        } => {
            if *rows == 0 || *cols == 0 {
                return Err(format!(
                    "Rule '{}': rows and cols must be non-zero",
                    rule.name
                ));
            }
            let cells = rows
                .checked_mul(*cols)
                .ok_or_else(|| format!("Rule '{}': dimensions overflow", rule.name))?;
            if cells > MAX_TRANSFER_CELLS {
                return Err(format!(
                    "Rule '{}': cell count {} exceeds max {MAX_TRANSFER_CELLS}",
                    rule.name, cells
                ));
            }
            if !matches!(cell_bits, 8 | 16 | 32) {
                return Err(format!(
                    "Rule '{}': cell_bits must be 8, 16, or 32 (got {})",
                    rule.name, cell_bits
                ));
            }
            if !scale.is_finite() || *scale == 0.0 || !offset.is_finite() {
                return Err(format!(
                    "Rule '{}': scale must be finite non-zero and offset finite",
                    rule.name
                ));
            }
            if values.len() != cells {
                return Err(format!(
                    "Rule '{}': expected {} cell values ({}x{}), got {}",
                    rule.name,
                    cells,
                    rows,
                    cols,
                    values.len()
                ));
            }
            if values.iter().any(|v| !v.is_finite()) {
                return Err(format!(
                    "Rule '{}': map transfer values contain non-finite numbers",
                    rule.name
                ));
            }
        }
    }
    Ok(())
}

fn decode_cell(
    bytes: &[u8],
    cell_bits: u8,
    little_endian: bool,
    is_signed: bool,
    scale: f64,
    offset: f64,
) -> f64 {
    let raw: f64 = match (cell_bits, little_endian, is_signed) {
        (8, _, false) => bytes[0] as f64,
        (8, _, true) => (bytes[0] as i8) as f64,
        (16, true, false) => u16::from_le_bytes([bytes[0], bytes[1]]) as f64,
        (16, false, false) => u16::from_be_bytes([bytes[0], bytes[1]]) as f64,
        (16, true, true) => i16::from_le_bytes([bytes[0], bytes[1]]) as f64,
        (16, false, true) => i16::from_be_bytes([bytes[0], bytes[1]]) as f64,
        (32, true, false) => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
        (32, false, false) => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
        (32, true, true) => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
        (32, false, true) => i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
        _ => 0.0,
    };
    raw * scale + offset
}

fn encode_cell(
    phys: f64,
    cell_bits: u8,
    little_endian: bool,
    is_signed: bool,
    scale: f64,
    offset: f64,
    out: &mut [u8],
) {
    let raw = ((phys - offset) / scale).round();
    match (cell_bits, little_endian, is_signed) {
        (8, _, false) => out[0] = raw.clamp(0.0, u8::MAX as f64) as u8,
        (8, _, true) => out[0] = (raw.clamp(i8::MIN as f64, i8::MAX as f64) as i8) as u8,
        (16, true, false) => {
            out.copy_from_slice(&(raw.clamp(0.0, u16::MAX as f64) as u16).to_le_bytes())
        }
        (16, false, false) => {
            out.copy_from_slice(&(raw.clamp(0.0, u16::MAX as f64) as u16).to_be_bytes())
        }
        (16, true, true) => {
            out.copy_from_slice(&(raw.clamp(i16::MIN as f64, i16::MAX as f64) as i16).to_le_bytes())
        }
        (16, false, true) => {
            out.copy_from_slice(&(raw.clamp(i16::MIN as f64, i16::MAX as f64) as i16).to_be_bytes())
        }
        (32, true, false) => {
            out.copy_from_slice(&(raw.clamp(0.0, u32::MAX as f64) as u32).to_le_bytes())
        }
        (32, false, false) => {
            out.copy_from_slice(&(raw.clamp(0.0, u32::MAX as f64) as u32).to_be_bytes())
        }
        (32, true, true) => {
            out.copy_from_slice(&(raw.clamp(i32::MIN as f64, i32::MAX as f64) as i32).to_le_bytes())
        }
        (32, false, true) => {
            out.copy_from_slice(&(raw.clamp(i32::MIN as f64, i32::MAX as f64) as i32).to_be_bytes())
        }
        _ => {}
    }
}

struct ComputedPatch {
    offset: usize,
    replacement: Vec<u8>,
}

fn evaluate_rule(rom: &[u8], rule: &ScriptRule) -> (RuleDryRunOutcome, Option<ComputedPatch>) {
    if let Err(err) = validate_script_rule(rule) {
        return (
            RuleDryRunOutcome {
                rule_name: rule.name.clone(),
                status: RuleDryRunStatus::InvalidRule,
                match_count: 0,
                matched_offsets: Vec::new(),
                target_offset: None,
                bytes_changed: 0,
                before_hex: String::new(),
                after_hex: String::new(),
                detail: err,
            },
            None,
        );
    }

    let pattern = match MaskedSignatureScanner::parse_pattern(&rule.signature) {
        Ok(p) => p,
        Err(e) => {
            return (
                RuleDryRunOutcome {
                    rule_name: rule.name.clone(),
                    status: RuleDryRunStatus::InvalidRule,
                    match_count: 0,
                    matched_offsets: Vec::new(),
                    target_offset: None,
                    bytes_changed: 0,
                    before_hex: String::new(),
                    after_hex: String::new(),
                    detail: e.to_string(),
                },
                None,
            );
        }
    };

    let matches = MaskedSignatureScanner::find_all(rom, &pattern);
    if matches.is_empty() {
        return (
            RuleDryRunOutcome {
                rule_name: rule.name.clone(),
                status: RuleDryRunStatus::NotFound,
                match_count: 0,
                matched_offsets: matches,
                target_offset: None,
                bytes_changed: 0,
                before_hex: String::new(),
                after_hex: String::new(),
                detail: "Signature did not match any byte sequence in target ROM".into(),
            },
            None,
        );
    }

    if rule.require_unique_match && matches.len() != 1 {
        return (
            RuleDryRunOutcome {
                rule_name: rule.name.clone(),
                status: RuleDryRunStatus::Ambiguous,
                match_count: matches.len(),
                matched_offsets: matches.into_iter().take(16).collect(),
                target_offset: None,
                bytes_changed: 0,
                before_hex: String::new(),
                after_hex: String::new(),
                detail: "Signature matched multiple locations with require_unique_match=true".into(),
            },
            None,
        );
    }

    let anchor = matches[0] as i128 + rule.relative_offset as i128;
    if anchor < 0 || anchor > usize::MAX as i128 {
        return (
            RuleDryRunOutcome {
                rule_name: rule.name.clone(),
                status: RuleDryRunStatus::OutOfBounds,
                match_count: matches.len(),
                matched_offsets: matches,
                target_offset: None,
                bytes_changed: 0,
                before_hex: String::new(),
                after_hex: String::new(),
                detail: format!(
                    "Relative offset {} produces negative or overflowing address",
                    rule.relative_offset
                ),
            },
            None,
        );
    }
    let target_offset = anchor as usize;

    match &rule.action {
        ScriptAction::PatchBytes {
            expected_hex,
            replacement_hex,
        } => {
            let rep = parse_hex_bytes(replacement_hex).unwrap_or_default();
            let Some(end) = target_offset.checked_add(rep.len()) else {
                return (
                    RuleDryRunOutcome {
                        rule_name: rule.name.clone(),
                        status: RuleDryRunStatus::OutOfBounds,
                        match_count: matches.len(),
                        matched_offsets: matches,
                        target_offset: Some(target_offset),
                        bytes_changed: 0,
                        before_hex: String::new(),
                        after_hex: String::new(),
                        detail: "Target patch range overflows address space".into(),
                    },
                    None,
                );
            };
            if end > rom.len() {
                return (
                    RuleDryRunOutcome {
                        rule_name: rule.name.clone(),
                        status: RuleDryRunStatus::OutOfBounds,
                        match_count: matches.len(),
                        matched_offsets: matches,
                        target_offset: Some(target_offset),
                        bytes_changed: 0,
                        before_hex: String::new(),
                        after_hex: String::new(),
                        detail: format!(
                            "Target range {target_offset:#X}..{end:#X} exceeds ROM size {:#X}",
                            rom.len()
                        ),
                    },
                    None,
                );
            }
            let current = &rom[target_offset..end];
            if let Some(exp_str) = expected_hex.as_ref().filter(|s| !s.trim().is_empty()) {
                let exp = parse_hex_bytes(exp_str).unwrap_or_default();
                if current != exp.as_slice() {
                    return (
                        RuleDryRunOutcome {
                            rule_name: rule.name.clone(),
                            status: RuleDryRunStatus::ExpectedMismatch,
                            match_count: matches.len(),
                            matched_offsets: matches,
                            target_offset: Some(target_offset),
                            bytes_changed: 0,
                            before_hex: encode_hex_spaced(current),
                            after_hex: encode_hex_spaced(&rep),
                            detail: format!(
                                "Expected original bytes [{}] did not match [{}]",
                                encode_hex_spaced(&exp),
                                encode_hex_spaced(current)
                            ),
                        },
                        None,
                    );
                }
            }
            let changed = current
                .iter()
                .zip(rep.iter())
                .filter(|(a, b)| a != b)
                .count();
            let preview_len = rep.len().min(32);
            (
                RuleDryRunOutcome {
                    rule_name: rule.name.clone(),
                    status: RuleDryRunStatus::Ready,
                    match_count: matches.len(),
                    matched_offsets: matches,
                    target_offset: Some(target_offset),
                    bytes_changed: changed,
                    before_hex: encode_hex_spaced(&current[..preview_len]),
                    after_hex: encode_hex_spaced(&rep[..preview_len]),
                    detail: format!("Matched unique signature -> {changed} byte(s) changed"),
                },
                Some(ComputedPatch {
                    offset: target_offset,
                    replacement: rep,
                }),
            )
        }
        ScriptAction::TransferMap {
            rows,
            cols,
            cell_bits,
            little_endian,
            is_signed,
            scale,
            offset,
            mode,
            values,
        } => {
            let cell_bytes = (*cell_bits / 8) as usize;
            let total_bytes = rows.saturating_mul(*cols).saturating_mul(cell_bytes);
            let Some(end) = target_offset.checked_add(total_bytes) else {
                return (
                    RuleDryRunOutcome {
                        rule_name: rule.name.clone(),
                        status: RuleDryRunStatus::OutOfBounds,
                        match_count: matches.len(),
                        matched_offsets: matches,
                        target_offset: Some(target_offset),
                        bytes_changed: 0,
                        before_hex: String::new(),
                        after_hex: String::new(),
                        detail: "Map transfer range overflows address space".into(),
                    },
                    None,
                );
            };
            if end > rom.len() {
                return (
                    RuleDryRunOutcome {
                        rule_name: rule.name.clone(),
                        status: RuleDryRunStatus::OutOfBounds,
                        match_count: matches.len(),
                        matched_offsets: matches,
                        target_offset: Some(target_offset),
                        bytes_changed: 0,
                        before_hex: String::new(),
                        after_hex: String::new(),
                        detail: format!(
                            "Map transfer {target_offset:#X}..{end:#X} exceeds ROM size {:#X}",
                            rom.len()
                        ),
                    },
                    None,
                );
            }

            let current = &rom[target_offset..end];
            let mut next = vec![0u8; total_bytes];
            for (idx, &v) in values.iter().enumerate() {
                let start_i = idx * cell_bytes;
                let end_i = start_i + cell_bytes;
                let cur_phys = decode_cell(
                    &current[start_i..end_i],
                    *cell_bits,
                    *little_endian,
                    *is_signed,
                    *scale,
                    *offset,
                );
                let target_phys = match mode {
                    TransferMode::Absolute => v,
                    TransferMode::AdditiveDelta => cur_phys + v,
                    TransferMode::PercentageDelta => cur_phys * (1.0 + v / 100.0),
                };
                encode_cell(
                    target_phys,
                    *cell_bits,
                    *little_endian,
                    *is_signed,
                    *scale,
                    *offset,
                    &mut next[start_i..end_i],
                );
            }
            let changed = current
                .iter()
                .zip(next.iter())
                .filter(|(a, b)| a != b)
                .count();
            let preview_len = next.len().min(32);
            (
                RuleDryRunOutcome {
                    rule_name: rule.name.clone(),
                    status: RuleDryRunStatus::Ready,
                    match_count: matches.len(),
                    matched_offsets: matches,
                    target_offset: Some(target_offset),
                    bytes_changed: changed,
                    before_hex: encode_hex_spaced(&current[..preview_len]),
                    after_hex: encode_hex_spaced(&next[..preview_len]),
                    detail: format!(
                        "{rows}x{cols} map transfer ({mode:?}) -> {changed} byte(s) changed"
                    ),
                },
                Some(ComputedPatch {
                    offset: target_offset,
                    replacement: next,
                }),
            )
        }
    }
}

/// Evaluates all rules in `script` against `rom` without mutating any bytes.
pub fn dry_run_calibration_script(
    rom: &[u8],
    script: &CalibrationScript,
) -> CalibrationScriptDryRun {
    let mut outcomes = Vec::with_capacity(script.rules.len());
    let mut can_apply = true;
    let mut total_bytes_changed = 0usize;

    for rule in &script.rules {
        let (outcome, patch) = evaluate_rule(rom, rule);
        if outcome.status != RuleDryRunStatus::Ready || patch.is_none() {
            can_apply = false;
        }
        total_bytes_changed = total_bytes_changed.saturating_add(outcome.bytes_changed);
        outcomes.push(outcome);
    }

    CalibrationScriptDryRun {
        script_name: script.name.clone(),
        can_apply,
        total_bytes_changed,
        rules: outcomes,
    }
}

/// Atomically applies all rules in `script` to a cloned copy of `rom`.
///
/// Refuses partial writes: if any rule fails signature matching, uniqueness,
/// expected-bytes verification, or bounds checks, returns `Err` without partial output.
pub fn apply_calibration_script(
    rom: &[u8],
    script: &CalibrationScript,
) -> Result<(Vec<u8>, CalibrationScriptDryRun), String> {
    if rom.is_empty() {
        return Err("ROM buffer cannot be empty".to_string());
    }
    let mut outcomes = Vec::with_capacity(script.rules.len());
    let mut patches = Vec::with_capacity(script.rules.len());
    let mut can_apply = true;
    let mut total_bytes_changed = 0usize;

    for rule in &script.rules {
        let (outcome, patch) = evaluate_rule(rom, rule);
        if outcome.status != RuleDryRunStatus::Ready {
            can_apply = false;
        }
        total_bytes_changed = total_bytes_changed.saturating_add(outcome.bytes_changed);
        outcomes.push(outcome);
        if let Some(p) = patch {
            patches.push(p);
        }
    }

    let report = CalibrationScriptDryRun {
        script_name: script.name.clone(),
        can_apply,
        total_bytes_changed,
        rules: outcomes,
    };

    if !report.can_apply || patches.len() != script.rules.len() {
        let failing: Vec<String> = report
            .rules
            .iter()
            .filter(|r| r.status != RuleDryRunStatus::Ready)
            .map(|r| format!("{} ({:?}: {})", r.rule_name, r.status, r.detail))
            .collect();
        return Err(format!(
            "Calibration script '{}' aborted before mutation: {}",
            script.name,
            failing.join("; ")
        ));
    }

    let mut out = rom.to_vec();
    for patch in patches {
        out[patch.offset..patch.offset + patch.replacement.len()]
            .copy_from_slice(&patch.replacement);
    }
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_script_applies_patch_and_additive_map_transfer() {
        let script = parse_calibration_script(&sample_calibration_script_json()).unwrap();
        let mut rom = vec![0x00u8; 128];
        rom[16..22].copy_from_slice(&[0xAA, 0xBB, 0x77, 0xDD, 0x1E, 0x00]);
        rom[48..56].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 10, 20, 30, 40]);

        let dry = dry_run_calibration_script(&rom, &script);
        assert!(dry.can_apply);
        assert_eq!(dry.rules[0].target_offset, Some(20));
        assert_eq!(dry.rules[1].target_offset, Some(52));
        assert_eq!(dry.total_bytes_changed, 5); // 1 byte in 1E->22 + 4 bytes in 2x2 map

        let (patched, applied) = apply_calibration_script(&rom, &script).unwrap();
        assert!(applied.can_apply);
        assert_eq!(&patched[20..22], &[0x22, 0x00]);
        assert_eq!(&patched[52..56], &[12, 22, 34, 44]);
    }

    #[test]
    fn refuses_ambiguous_or_mismatched_expected_bytes_atomically() {
        let mut rom = vec![0xFFu8; 128];
        // Duplicate signature at two offsets
        rom[10..14].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        rom[60..64].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let script = CalibrationScript {
            schema_version: 1,
            name: "Ambiguous Test".into(),
            description: "Should refuse non-unique matches".into(),
            target_hint: None,
            rules: vec![ScriptRule {
                name: "Dup".into(),
                signature: "AA BB CC DD".into(),
                relative_offset: 0,
                require_unique_match: true,
                action: ScriptAction::PatchBytes {
                    expected_hex: None,
                    replacement_hex: "11 22 33 44".into(),
                },
            }],
        };

        let dry = dry_run_calibration_script(&rom, &script);
        assert!(!dry.can_apply);
        assert_eq!(dry.rules[0].status, RuleDryRunStatus::Ambiguous);
        assert_eq!(dry.rules[0].match_count, 2);
        assert!(apply_calibration_script(&rom, &script).is_err());
    }

    #[test]
    fn handles_16bit_percentage_transfer_and_negative_offset_bounds() {
        let mut rom = vec![0u8; 64];
        rom[2..6].copy_from_slice(&[0x55, 0x66, 0x77, 0x88]);
        // Two LE u16 cells right after signature: 1000 and 2000
        rom[6..8].copy_from_slice(&1000u16.to_le_bytes());
        rom[8..10].copy_from_slice(&2000u16.to_le_bytes());

        let script = CalibrationScript {
            schema_version: 1,
            name: "Pct Boost".into(),
            description: "+10% 1x2 table".into(),
            target_hint: None,
            rules: vec![ScriptRule {
                name: "Scale 10pct".into(),
                signature: "55 66 77 88".into(),
                relative_offset: 4,
                require_unique_match: true,
                action: ScriptAction::TransferMap {
                    rows: 1,
                    cols: 2,
                    cell_bits: 16,
                    little_endian: true,
                    is_signed: false,
                    scale: 1.0,
                    offset: 0.0,
                    mode: TransferMode::PercentageDelta,
                    values: vec![10.0, 10.0],
                },
            }],
        };

        let (patched, rep) = apply_calibration_script(&rom, &script).unwrap();
        assert!(rep.can_apply);
        assert_eq!(u16::from_le_bytes([patched[6], patched[7]]), 1100);
        assert_eq!(u16::from_le_bytes([patched[8], patched[9]]), 2200);

        let oob_script = CalibrationScript {
            schema_version: 1,
            name: "Neg OOB".into(),
            description: "Negative offset before start".into(),
            target_hint: None,
            rules: vec![ScriptRule {
                name: "BeforeStart".into(),
                signature: "55 66 77 88".into(),
                relative_offset: -10,
                require_unique_match: true,
                action: ScriptAction::PatchBytes {
                    expected_hex: None,
                    replacement_hex: "00".into(),
                },
            }],
        };
        let oob_dry = dry_run_calibration_script(&rom, &oob_script);
        assert!(!oob_dry.can_apply);
        assert_eq!(oob_dry.rules[0].status, RuleDryRunStatus::OutOfBounds);
    }
}
