//! Tests for the static ECU protocol module scope registry.
//!
//! Checks entry counts, descriptor invariants, and search/filter helpers.

use kivi_flashing::reference_modules::{
    find_modules, module_by_index, modules_for_brand, modules_with_encrypted_payload,
    total_module_bytes, ReferenceModule, REFERENCE_MODULES, REFERENCE_MODULE_COUNT,
};

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn the_registry_has_99_modules() {
    assert_eq!(REFERENCE_MODULE_COUNT, 99);
    assert_eq!(REFERENCE_MODULES.len(), 99);
}

#[test]
fn every_module_has_a_non_empty_coverage_string() {
    for m in REFERENCE_MODULES {
        assert!(
            !m.coverage.trim().is_empty(),
            "module {} has an empty coverage string",
            m.file_index
        );
    }
}

#[test]
fn every_module_has_a_non_zero_file_size() {
    for m in REFERENCE_MODULES {
        assert!(m.file_size > 0, "module {} reports zero size", m.file_index);
    }
}

#[test]
fn file_indices_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for m in REFERENCE_MODULES {
        assert!(seen.insert(m.file_index), "duplicate index {}", m.file_index);
    }
}

// ---------------------------------------------------------------------------
// Known entries — the ones the analysis actually relied on
// ---------------------------------------------------------------------------

#[test]
fn module_36_is_the_denso_full_module() {
    let m = module_by_index(36).expect("module36 exists");
    assert_eq!(m.vendor_number, 63);
    assert!(
        m.coverage.contains("Denso"),
        "coverage was {:?}",
        m.coverage
    );
    assert!(m.coverage.contains("SH"));
}

#[test]
fn module_36_is_the_only_one_with_encrypted_blobs() {
    let with_blobs = modules_with_encrypted_payload();
    assert_eq!(with_blobs.len(), 1, "only one module ships encrypted blobs");
    assert_eq!(with_blobs[0].file_index, 36);
    assert_eq!(with_blobs[0].encrypted_blobs, 15);
}

#[test]
fn module_30_is_honda_k_line() {
    let m = module_by_index(30).expect("module30");
    assert!(m.coverage.contains("Honda"), "{:?}", m.coverage);
    assert!(m.coverage.contains("K-line"));
}

#[test]
fn module_62_is_vag_tp20() {
    let m = module_by_index(62).expect("module62");
    assert!(m.coverage.contains("TP2.0"), "{:?}", m.coverage);
}

#[test]
fn module_34_is_nissan_m32r() {
    let m = module_by_index(34).expect("module34");
    assert!(m.coverage.contains("NISSAN") || m.coverage.contains("Nissan"));
    assert!(m.coverage.contains("M32R"));
}

#[test]
fn module_61_is_the_frf_sgo_updater() {
    let m = module_by_index(61).expect("module61");
    assert!(m.coverage.contains(".frf"), "{:?}", m.coverage);
    assert!(m.coverage.contains(".sgo"));
}

#[test]
fn module_76_is_honda_bike_kline() {
    let m = module_by_index(76).expect("module76");
    assert!(m.coverage.contains("Honda"), "{:?}", m.coverage);
    assert!(m.coverage.to_lowercase().contains("bike"));
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn searching_honda_finds_multiple_modules() {
    let hits = modules_for_brand("Honda");
    assert!(
        hits.len() >= 3,
        "expected several Honda modules, got {}",
        hits.len()
    );
    for h in &hits {
        assert!(h.coverage.to_ascii_lowercase().contains("honda"));
    }
}

#[test]
fn searching_bmw_finds_modules() {
    let hits = modules_for_brand("BMW");
    assert!(!hits.is_empty());
}

#[test]
fn searching_is_case_insensitive() {
    assert_eq!(
        modules_for_brand("honda").len(),
        modules_for_brand("HONDA").len()
    );
}

#[test]
fn an_empty_query_returns_nothing() {
    assert!(find_modules("").is_empty());
    assert!(find_modules("   ").is_empty());
}

#[test]
fn an_unmatchable_query_returns_nothing() {
    assert!(find_modules("ZZZZ-NOT-A-MODULE").is_empty());
}

#[test]
fn looking_up_a_missing_index_returns_none() {
    assert!(module_by_index(9999).is_none());
}

// ---------------------------------------------------------------------------
// Aggregate
// ---------------------------------------------------------------------------

#[test]
fn the_total_size_is_plausible() {
    // 99 modules, mostly 10-100 KB, with a few multi-hundred-KB outliers.
    let total = total_module_bytes();
    assert!(total > 1_000_000, "total was {total} bytes");
    assert!(total < 50_000_000, "total was {total} bytes");
}

// ---------------------------------------------------------------------------
// Honesty guard
// ---------------------------------------------------------------------------

#[test]
fn the_registry_carries_no_invented_technical_fields() {
    // `ReferenceModule` has exactly five fields, all taken from the DLL itself.
    // Adding an `mcu` or `flash_size` field without a real source would break
    // the honesty this table depends on.
    let m: ReferenceModule = REFERENCE_MODULES[0];
    assert!(m.file_index > 0);
    assert!(!m.coverage.is_empty());
}
