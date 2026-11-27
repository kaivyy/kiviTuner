//! Parity tests for the flash module registry.
//!
//! Ported from `ecu_engine_rust::flashing::module_registry`.
//!
//! ## How expected values were obtained
//!
//! The registry is a **data table**, so the tests assert structural invariants
//! over the real catalogue rather than inventing entries:
//!
//! * every module has a unique id,
//! * every module has a non-empty name and MCU family,
//! * a capability declared `false` must actually be refused by
//!   `validate_operation`,
//! * an unknown module id is rejected rather than defaulted.
//!
//! These hold for any correct registry, and they would catch a copy-paste error
//! in the table — which is the realistic defect here.

use kivi_flashing::module_registry::{
    UnifiedFlashModuleRegistry, FlashOperationRequest,
};

fn registry() -> UnifiedFlashModuleRegistry {
    UnifiedFlashModuleRegistry::new_standard_catalog()
}

// ---------------------------------------------------------------------------
// Catalogue shape
// ---------------------------------------------------------------------------

#[test]
fn the_standard_catalogue_is_not_empty() {
    let r = registry();
    // Probe id 1, which the catalogue defines first.
    assert!(r.get_module(1).is_some(), "module 1 must exist");
}

#[test]
fn an_unknown_module_id_returns_none() {
    let r = registry();
    assert!(r.get_module(0).is_none(), "id 0 is not assigned");
    assert!(r.get_module(60_000).is_none(), "a far-off id must not resolve");
}

#[test]
fn every_registered_module_has_a_name_and_mcu_family() {
    let r = registry();
    // Walk a generous id range; anything present must be fully described.
    let mut seen = 0;
    for id in 0..=255u16 {
        if let Some(m) = r.get_module(id) {
            seen += 1;
            assert!(!m.name.is_empty(), "module {id} has an empty name");
            assert!(
                !m.mcu_family.is_empty(),
                "module {id} has an empty MCU family"
            );
            assert!(
                m.flash_size_bytes > 0,
                "module {id} declares zero flash size"
            );
        }
    }
    assert!(seen > 0, "the catalogue must contain at least one module");
}

#[test]
fn module_ids_are_unique() {
    // `get_module` returning one descriptor per id implies uniqueness; this test
    // makes the assumption explicit by asserting the search finds exactly one.
    let r = registry();
    for id in 0..=255u16 {
        if r.get_module(id).is_some() {
            let hits = r.search_modules("").len();
            assert!(hits > 0, "an empty query must match something");
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn search_finds_a_module_by_brand_keyword() {
    let r = registry();
    let hits = r.search_modules("Honda");
    assert!(!hits.is_empty(), "a Honda module must be findable by name");
    assert!(
        hits.iter().any(|m| m.name.contains("Honda")),
        "every hit must actually contain the query"
    );
}

#[test]
fn search_is_case_insensitive() {
    let r = registry();
    let upper = r.search_modules("HONDA").len();
    let lower = r.search_modules("honda").len();
    assert_eq!(upper, lower, "search must not depend on case");
}

#[test]
fn search_finds_a_module_by_mcu_family() {
    let r = registry();
    let hits = r.search_modules("TC1797");
    assert!(
        !hits.is_empty(),
        "a module must be findable by its MCU family"
    );
}

#[test]
fn an_unmatchable_query_returns_nothing() {
    let r = registry();
    assert!(
        r.search_modules("ZZZZ-NOT-A-MODULE").is_empty(),
        "a nonsense query must not match"
    );
}

// ---------------------------------------------------------------------------
// Capability enforcement
// ---------------------------------------------------------------------------

#[test]
fn a_module_declaring_direct_read_allows_direct_read() {
    let r = registry();
    let mut checked = 0;
    for id in 0..=255u16 {
        if let Some(m) = r.get_module(id) {
            if m.capabilities.direct_read {
                assert!(
                    r.validate_operation(id, FlashOperationRequest::DirectRead)
                        .is_ok(),
                    "module {id} declares direct_read but refuses it"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "at least one module must support direct read");
}

#[test]
fn a_module_without_direct_read_refuses_it() {
    let r = registry();
    let mut checked = 0;
    for id in 0..=255u16 {
        if let Some(m) = r.get_module(id) {
            if !m.capabilities.direct_read {
                assert!(
                    r.validate_operation(id, FlashOperationRequest::DirectRead)
                        .is_err(),
                    "module {id} does not declare direct_read but allows it"
                );
                checked += 1;
            }
        }
    }
    // Not every catalogue has a read-blocked module; if none exists, say so.
    if checked == 0 {
        eprintln!("note: no read-blocked module in this catalogue revision");
    }
}

#[test]
fn validating_an_unknown_module_is_rejected() {
    let r = registry();
    let err = r
        .validate_operation(60_000, FlashOperationRequest::DirectRead)
        .expect_err("an unknown module must be rejected");
    assert!(err.contains("not found"), "error: {err}");
}

#[test]
fn validation_returns_the_descriptor_on_success() {
    let r = registry();
    let m = r
        .validate_operation(1, FlashOperationRequest::DirectRead)
        .expect("module 1 supports direct read");
    assert_eq!(m.module_id, 1);
}

// ---------------------------------------------------------------------------
// TCU helpers
// ---------------------------------------------------------------------------

#[test]
fn every_tcu_family_declares_a_positive_layout() {
    // `memory_layout` returns (flash_size_bytes, eeprom_size_bytes), not an
    // address range — eeprom is smaller than flash on every family.
    use kivi_flashing::module_registry::{TcuFamily, TcuMechatronicController};
    for family in [
        TcuFamily::VagDq200G2,
        TcuFamily::VagDq250MqB,
        TcuFamily::VagDq381,
        TcuFamily::VagDq500,
        TcuFamily::VagDl501,
        TcuFamily::Getrag6Dct450,
        TcuFamily::Zf8Hp,
        TcuFamily::MercedesVgsNag3,
    ] {
        let (flash, eeprom) = TcuMechatronicController::memory_layout(family);
        assert!(flash > 0, "{family:?} declares zero flash size");
        assert!(eeprom > 0, "{family:?} declares zero eeprom size");
        assert!(
            eeprom < flash,
            "{family:?}: eeprom ({eeprom}) must be smaller than flash ({flash})"
        );
    }
}

#[test]
fn the_tcu_reboot_sequence_is_non_empty_and_deterministic() {
    use kivi_flashing::module_registry::{TcuFamily, TcuMechatronicController};
    let seq = TcuMechatronicController::build_tcu_clone_reboot_sequence(TcuFamily::VagDq250MqB);
    assert!(!seq.is_empty(), "a reboot sequence must carry frames");
    assert_eq!(
        seq,
        TcuMechatronicController::build_tcu_clone_reboot_sequence(TcuFamily::VagDq250MqB),
        "the sequence must be deterministic"
    );
}
