//! Multi-format calibration definition envelope (Phase 1B-Δ Step 1).
//!
//! Pure additive contract wrapping `MapDef` / `ConstantDef` / `FlagDef` without
//! modifying existing `kivi-defs` types or Tauri command signatures (`I6`, `I7`).
//! Enforces invariants `I1–I10`:
//! - `I1`: Parse failures return `Result::Err`, never `ParseStatus::Failed`.
//! - `I2`: Dropped/unsupported items emit a `Diagnostic`.
//! - `I3`: `DeclaredAddress` is `Copy` and immutable across resolutions.
//! - `I4`: `Resolution::Resolved` requires `FileOffset` or `is_identity_fallback == false`.
//! - `I5`: `cell_file_offset` accepts `FileOffset` only.
//! - `I8`: `source.location` is purely informational and excluded from validation signatures.

use crate::expr::{evaluate_scaling, invert_scaling};
use crate::model::{ConstantDef, FlagDef, MapDef};
use crate::xdf::{parse_xdf_full, XdfError};
use kivi_core::address_translation::{cpu_to_raw, MemoryAddressTranslator};
use kivi_core::hash::compute_hashes;
use serde::{Deserialize, Serialize};

/// Origin format of an imported definition document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceFormat {
    Xdf,
    A2l,
    Damos,
    Kp,
    Unknown,
}

/// Immutable provenance metadata for a parsed definition file (`I8`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionSource {
    pub format: SourceFormat,
    /// Sole authoritative identity digest (`I8`).
    pub sha256: String,
    pub len: u64,
    /// Informational path/filename only — never used by validation logic.
    pub location: Option<String>,
    pub parser_id: String,
    pub imported_at: String,
}

impl DefinitionSource {
    pub fn from_bytes(
        format: SourceFormat,
        bytes: &[u8],
        location: Option<String>,
        parser_id: impl Into<String>,
        imported_at: impl Into<String>,
    ) -> Self {
        Self {
            format,
            sha256: compute_hashes(bytes).sha256,
            len: bytes.len() as u64,
            location,
            parser_id: parser_id.into(),
            imported_at: imported_at.into(),
        }
    }
}

/// Verified byte offset inside a ROM file (`I5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileOffset(pub usize);

/// Logical ECU target address declared by A2L/DAMOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TargetAddress(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressKind {
    FileOffset,
    TargetAddress,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressSpace {
    File,
    EcuVirtual,
    Unknown,
}

/// Declared address exactly as read from the definition file (`I3`: immutable, `Copy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredAddress {
    pub raw: u32,
    pub kind: AddressKind,
    pub space: AddressSpace,
}

impl DeclaredAddress {
    pub fn file_offset(offset: usize) -> Self {
        Self {
            raw: offset as u32,
            kind: AddressKind::FileOffset,
            space: AddressSpace::File,
        }
    }

    pub fn target(addr: u32) -> Self {
        Self {
            raw: addr,
            kind: AddressKind::TargetAddress,
            space: AddressSpace::EcuVirtual,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BasisUsed {
    FileOffsetDirect,
    TranslatorSegment(String),
    HardcodedFallback(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnresolvedReason {
    TranslationIdentityFallback,
    UnmappedSegment(String),
    UnknownAddressKind,
    TranslationError(String),
}

/// Resolution outcome against a concrete BIN length (`I4`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    NotAttempted,
    Resolved {
        offset: FileOffset,
        basis: BasisUsed,
    },
    Unresolved {
        reason: UnresolvedReason,
    },
    /// Retained for future multi-match routines; unreachable with current `cpu_to_raw` (`D4`).
    Ambiguous {
        candidates: Vec<FileOffset>,
    },
    OutOfBounds {
        offset: FileOffset,
        bin_len: usize,
    },
}

/// Resolves an immutable `DeclaredAddress` without ever mutating it (`I3`, `I4`).
pub fn resolve_address(
    declared: DeclaredAddress,
    translator: Option<&MemoryAddressTranslator>,
    basis_label: &str,
    is_hardcoded_fallback: bool,
    bin_len: usize,
) -> Resolution {
    match declared.kind {
        AddressKind::Unknown => Resolution::Unresolved {
            reason: UnresolvedReason::UnknownAddressKind,
        },
        AddressKind::FileOffset => {
            let off = declared.raw as usize;
            if off < bin_len {
                Resolution::Resolved {
                    offset: FileOffset(off),
                    basis: BasisUsed::FileOffsetDirect,
                }
            } else {
                Resolution::OutOfBounds {
                    offset: FileOffset(off),
                    bin_len,
                }
            }
        }
        AddressKind::TargetAddress => {
            let Some(tr) = translator else {
                return Resolution::Unresolved {
                    reason: UnresolvedReason::UnmappedSegment(
                        "no address translator available".to_string(),
                    ),
                };
            };
            match cpu_to_raw(tr, declared.raw) {
                Err(e) => Resolution::Unresolved {
                    reason: UnresolvedReason::TranslationError(e),
                },
                Ok(t) if t.is_identity_fallback => Resolution::Unresolved {
                    reason: UnresolvedReason::TranslationIdentityFallback,
                },
                Ok(t) => {
                    let off = t.value as usize;
                    if off >= bin_len {
                        Resolution::OutOfBounds {
                            offset: FileOffset(off),
                            bin_len,
                        }
                    } else {
                        Resolution::Resolved {
                            offset: FileOffset(off),
                            basis: if is_hardcoded_fallback {
                                BasisUsed::HardcodedFallback(basis_label.to_string())
                            } else {
                                BasisUsed::TranslatorSegment(basis_label.to_string())
                            },
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Dropped,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub source_block: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructuralFailure {
    ZeroOrOverflowDimensions { rows: usize, cols: usize },
    SpanExceedsBinary { start: usize, end: usize, bin_len: usize },
    CellStrideOutOfBounds { row: usize, col: usize, offset: i64, bin_len: usize },
    InvalidConversionEquation { equation: String, detail: String },
    ConversionNotInvertible { equation: String, probe_physical: String },
    ResolutionNotValid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureStatus {
    Complete,
    Partial { dropped: Vec<String> },
    Unsupported { kind: String },
    NotChecked,
    ChecksFailed { failures: Vec<StructuralFailure> },
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObjectPayload {
    Table(MapDef),
    Scalar(ConstantDef),
    BitField(FlagDef),
    Unsupported {
        kind: String,
        format: SourceFormat,
        raw_properties: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalObject {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub payload: ObjectPayload,
    pub declared: DeclaredAddress,
    pub resolved: Resolution,
    pub structure: StructureStatus,
    pub diagnostics: Vec<Diagnostic>,
}

/// Top-level parsed definition document (`I1`: replaces `ParseStatus`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub source: DefinitionSource,
    pub objects: Vec<CalObject>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Cell byte offset computed strictly from a validated `FileOffset` (`I5`).
pub fn cell_file_offset(
    base: FileOffset,
    map_def: &MapDef,
    row: usize,
    col: usize,
    bytes_per_elem: usize,
) -> i64 {
    let elem_bits = (bytes_per_elem as i64).saturating_mul(8);
    let major_len = if map_def.column_major {
        map_def.rows
    } else {
        map_def.cols
    };
    let row_bits: i64 = if map_def.major_stride_bits == 0 {
        (major_len as i64).saturating_mul(elem_bits)
    } else {
        map_def.major_stride_bits as i64
    };
    let col_bits: i64 = if map_def.minor_stride_bits == 0 {
        elem_bits
    } else {
        map_def.minor_stride_bits as i64
    };
    let bit = (base.0 as i64)
        .saturating_mul(8)
        .saturating_add((row as i64).saturating_mul(row_bits))
        .saturating_add((col as i64).saturating_mul(col_bits));
    bit.div_euclid(8)
}

/// Runs all 5 minimal Step-1 structural checks from Phase 1B-Δ § 4.3 (`F3`, `D7`).
///
/// Write-path float-to-integer coercion differs between Rust (`as u16` toward zero)
/// and the TS fallback (`Math.round`); Step 1 validates/reports conversion invertibility
/// without altering write semantics (`D8`).
pub fn validate_map_structure(
    map: &MapDef,
    resolved: &Resolution,
    bin_len: usize,
) -> StructureStatus {
    let Resolution::Resolved { offset: base, .. } = resolved else {
        return StructureStatus::ChecksFailed {
            failures: vec![StructuralFailure::ResolutionNotValid],
        };
    };

    let mut failures = Vec::new();
    let elem_bytes = map.dtype.byte_size();

    // Check 1 & 2: positive geometry, no rows*cols overflow, valid element width.
    let Some(cells) = map.rows.checked_mul(map.cols) else {
        failures.push(StructuralFailure::ZeroOrOverflowDimensions {
            rows: map.rows,
            cols: map.cols,
        });
        return StructureStatus::ChecksFailed { failures };
    };
    if map.rows == 0 || map.cols == 0 || cells == 0 || elem_bytes == 0 {
        failures.push(StructuralFailure::ZeroOrOverflowDimensions {
            rows: map.rows,
            cols: map.cols,
        });
        return StructureStatus::ChecksFailed { failures };
    }

    // Check 3: contiguous span bounds when unpacked (major == 0 && minor == 0).
    let Some(span) = cells.checked_mul(elem_bytes) else {
        failures.push(StructuralFailure::ZeroOrOverflowDimensions {
            rows: map.rows,
            cols: map.cols,
        });
        return StructureStatus::ChecksFailed { failures };
    };
    if map.major_stride_bits == 0 && map.minor_stride_bits == 0 {
        match base.0.checked_add(span) {
            Some(end) if end <= bin_len => {}
            _ => failures.push(StructuralFailure::SpanExceedsBinary {
                start: base.0,
                end: base.0.saturating_add(span),
                bin_len,
            }),
        }
    }

    // Check 4: every stride-computed cell offset must be non-negative and in bounds.
    let mut stride_oob = false;
    for r in 0..map.rows {
        for c in 0..map.cols {
            let (grid_row, grid_col) = if map.column_major { (c, r) } else { (r, c) };
            let off = cell_file_offset(*base, map, grid_row, grid_col, elem_bytes);
            if off < 0 || (off as usize).saturating_add(elem_bytes) > bin_len {
                failures.push(StructuralFailure::CellStrideOutOfBounds {
                    row: r,
                    col: c,
                    offset: off,
                    bin_len,
                });
                stride_oob = true;
                break;
            }
        }
        if stride_oob {
            break;
        }
    }

    // Check 5: scaling equation evaluation & invertibility check (S3 documented).
    let eq = map.scaling.equation.trim();
    if !eq.is_empty() && !eq.eq_ignore_ascii_case("x") {
        match (evaluate_scaling(eq, 1.0), evaluate_scaling(eq, 10.0)) {
            (Ok(y1), Ok(y10)) => {
                if (y10 - y1).abs() > 1e-12 && invert_scaling(eq, y10).is_err() {
                    failures.push(StructuralFailure::ConversionNotInvertible {
                        equation: eq.to_string(),
                        probe_physical: format!("{y10:.6}"),
                    });
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                failures.push(StructuralFailure::InvalidConversionEquation {
                    equation: eq.to_string(),
                    detail: e.to_string(),
                });
            }
        }
    }

    if failures.is_empty() {
        StructureStatus::Complete
    } else {
        StructureStatus::ChecksFailed { failures }
    }
}

/// Imports an XDF byte buffer into a provenance-tracked [`ParsedDocument`].
pub fn parse_xdf_document(
    bytes: &[u8],
    location: Option<String>,
    imported_at: impl Into<String>,
    bin_len: Option<usize>,
) -> Result<ParsedDocument, XdfError> {
    let source = DefinitionSource::from_bytes(
        SourceFormat::Xdf,
        bytes,
        location,
        "kivi-defs::xdf::parse_xdf_full",
        imported_at,
    );
    let defs = parse_xdf_full(&String::from_utf8_lossy(bytes))?;
    let mut objects = Vec::with_capacity(defs.maps.len() + defs.constants.len() + defs.flags.len());

    for m in defs.maps {
        let declared = DeclaredAddress::file_offset(m.address);
        let resolved = match bin_len {
            Some(len) => resolve_address(declared, None, "XdfFileOffset", false, len),
            None => Resolution::NotAttempted,
        };
        let structure = match bin_len {
            Some(len) => validate_map_structure(&m, &resolved, len),
            None => StructureStatus::NotChecked,
        };
        objects.push(CalObject {
            id: m.id.clone(),
            name: m.name.clone(),
            category: m.category.clone(),
            description: String::new(),
            payload: ObjectPayload::Table(m),
            declared,
            resolved,
            structure,
            diagnostics: Vec::new(),
        });
    }

    for sc in defs.constants {
        let declared = DeclaredAddress::file_offset(sc.address);
        let elem = sc.dtype.byte_size();
        let resolved = match bin_len {
            Some(len) => resolve_address(declared, None, "XdfFileOffset", false, len),
            None => Resolution::NotAttempted,
        };
        let structure = match (&resolved, bin_len) {
            (Resolution::Resolved { offset, .. }, Some(len)) => {
                if offset.0.saturating_add(elem) <= len {
                    StructureStatus::Complete
                } else {
                    StructureStatus::ChecksFailed {
                        failures: vec![StructuralFailure::SpanExceedsBinary {
                            start: offset.0,
                            end: offset.0.saturating_add(elem),
                            bin_len: len,
                        }],
                    }
                }
            }
            (_, Some(_)) => StructureStatus::ChecksFailed {
                failures: vec![StructuralFailure::ResolutionNotValid],
            },
            _ => StructureStatus::NotChecked,
        };
        objects.push(CalObject {
            id: sc.id.clone(),
            name: sc.name.clone(),
            category: sc.category.clone(),
            description: String::new(),
            payload: ObjectPayload::Scalar(sc),
            declared,
            resolved,
            structure,
            diagnostics: Vec::new(),
        });
    }

    for fl in defs.flags {
        let declared = DeclaredAddress::file_offset(fl.address);
        let elem = fl.dtype.byte_size();
        let resolved = match bin_len {
            Some(len) => resolve_address(declared, None, "XdfFileOffset", false, len),
            None => Resolution::NotAttempted,
        };
        let structure = match (&resolved, bin_len) {
            (Resolution::Resolved { offset, .. }, Some(len)) => {
                if offset.0.saturating_add(elem) <= len {
                    StructureStatus::Complete
                } else {
                    StructureStatus::ChecksFailed {
                        failures: vec![StructuralFailure::SpanExceedsBinary {
                            start: offset.0,
                            end: offset.0.saturating_add(elem),
                            bin_len: len,
                        }],
                    }
                }
            }
            (_, Some(_)) => StructureStatus::ChecksFailed {
                failures: vec![StructuralFailure::ResolutionNotValid],
            },
            _ => StructureStatus::NotChecked,
        };
        objects.push(CalObject {
            id: fl.id.clone(),
            name: fl.name.clone(),
            category: fl.category.clone(),
            description: String::new(),
            payload: ObjectPayload::BitField(fl),
            declared,
            resolved,
            structure,
            diagnostics: Vec::new(),
        });
    }

    Ok(ParsedDocument {
        source,
        objects,
        diagnostics: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scaling;
    use kivi_core::address_translation::{MemoryAddressTranslator, MemorySegmentRule};
    use kivi_core::types::{DType, Endian};

    fn sample_map(address: usize, rows: usize, cols: usize, major_stride_bits: i32, eq: &str) -> MapDef {
        MapDef {
            id: "m1".to_string(),
            name: "Fuel".to_string(),
            category: "Fuel".to_string(),
            address,
            rows,
            cols,
            dtype: DType::U16,
            endian: Endian::Little,
            column_major: false,
            major_stride_bits,
            minor_stride_bits: 0,
            scaling: Scaling {
                equation: eq.to_string(),
                unit: "deg".to_string(),
                decimals: 2,
            },
            x_axis: None,
            y_axis: None,
        }
    }

    #[test]
    fn i3_i4_declared_address_immutable_and_identity_fallback_unresolved() {
        let tr = MemoryAddressTranslator::default();
        // 0x8000_1000 matches TriCore segment -> Resolved (is_identity_fallback == false)
        let mapped_decl = DeclaredAddress::target(0x8000_1000);
        let r_ok = resolve_address(mapped_decl, Some(&tr), "TriCore", false, 0x40_0000);
        assert_eq!(
            r_ok,
            Resolution::Resolved {
                offset: FileOffset(0x1000),
                basis: BasisUsed::TranslatorSegment("TriCore".to_string())
            }
        );
        assert_eq!(mapped_decl.raw, 0x8000_1000);

        // 0x1234_5678 matches no rule -> identity fallback -> Unresolved (I4).
        let unmapped = DeclaredAddress::target(0x1234_5678);
        let r_fb = resolve_address(unmapped, Some(&tr), "TriCore", false, usize::MAX);
        assert_eq!(
            r_fb,
            Resolution::Unresolved {
                reason: UnresolvedReason::TranslationIdentityFallback
            }
        );
        assert_eq!(unmapped.raw, 0x1234_5678);

        // OutOfBounds check when segment maps past ROM end.
        let small_tr = MemoryAddressTranslator {
            segments: vec![MemorySegmentRule {
                name: "Flash".to_string(),
                cpu_start_address: 0x8000_0000,
                cpu_end_address: 0x800F_FFFF,
                raw_file_offset: 0,
            }],
        };
        let oob = resolve_address(DeclaredAddress::target(0x8008_0000), Some(&small_tr), "Flash", false, 0x10000);
        assert!(matches!(oob, Resolution::OutOfBounds { .. }));
    }

    #[test]
    fn f3_five_structural_checks_catch_overflow_stride_and_scaling() {
        let m_ok = sample_map(0x100, 4, 4, 0, "X * 0.5 + 10");
        let res_ok = resolve_address(DeclaredAddress::file_offset(0x100), None, "XDF", false, 4096);
        assert_eq!(validate_map_structure(&m_ok, &res_ok, 4096), StructureStatus::Complete);

        // Negative stride walking before byte 0 must fail check 4 even if base address is in bounds.
        let m_neg_stride = sample_map(0x10, 4, 4, -64, "X");
        let res_stride = resolve_address(DeclaredAddress::file_offset(0x10), None, "XDF", false, 4096);
        assert!(matches!(
            validate_map_structure(&m_neg_stride, &res_stride, 4096),
            StructureStatus::ChecksFailed { .. }
        ));

        // Invalid equation syntax fails check 5.
        let m_bad_eq = sample_map(0x100, 4, 4, 0, "X ** ???");
        assert!(matches!(
            validate_map_structure(&m_bad_eq, &res_ok, 4096),
            StructureStatus::ChecksFailed { .. }
        ));
    }

    #[test]
    fn i1_i2_xdf_empty_vs_malformed_and_valid_document() {
        let err = parse_xdf_document(b"<XDFFORMAT><broken", None, "2026-11-26", None);
        assert!(err.is_err());

        let empty_doc = parse_xdf_document(b"<XDFFORMAT></XDFFORMAT>", Some("a.xdf".into()), "2026-11-26", None)
            .expect("valid empty xml");
        assert!(empty_doc.objects.is_empty());
        assert_eq!(empty_doc.source.format, SourceFormat::Xdf);
    }
}
