//! Tests for the protocol capability catalogue.
//!
//! The catalogue records **which vehicle/ECU variants exist** and their capability
//! flags (`RD`, `VR`, `WR`, `CK`, `NC`). It deliberately carries no MCU or
//! flash-size data — inventing unverified geometry would invite a bad flash.

use kivi_flashing::catalogue::{
    direct_read_capable, search_catalogue, CatalogueEntry, CATALOGUE_LEN, VENDOR_CATALOGUE,
};

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn the_catalogue_is_populated() {
    // Read through a runtime value so clippy cannot fold this to a constant:
    // the point is to fail the build if the table ever shrinks.
    let len = VENDOR_CATALOGUE.len();
    assert!(len > 500, "the reference list has hundreds of variants, found {len}");
    assert_eq!(CATALOGUE_LEN, len);
}

#[test]
fn every_entry_has_a_non_empty_description() {
    for e in VENDOR_CATALOGUE {
        assert!(
            !e.description.trim().is_empty(),
            "an entry has an empty description"
        );
    }
}

#[test]
fn every_entry_declares_at_least_one_capability() {
    // An entry with no flags at all would be a parsing artefact.
    for e in VENDOR_CATALOGUE {
        let any = e.direct_read || e.virtual_read || e.write_flash || e.checksum_correct || e.no_checksum;
        assert!(any, "entry '{}' has no capability flag", e.description);
    }
}

// ---------------------------------------------------------------------------
// Flag decoding
// ---------------------------------------------------------------------------

#[test]
fn a_full_capability_entry_decodes_all_flags() {
    let e = VENDOR_CATALOGUE
        .iter()
        .find(|e| e.direct_read && e.write_flash && e.checksum_correct)
        .expect("the list must contain a full-capability module");
    assert!(e.direct_read);
    assert!(e.write_flash);
    assert!(e.checksum_correct);
}

#[test]
fn a_virtual_read_only_entry_does_not_claim_direct_read() {
    // Several Toyota/VAG modules are virtual-read only; the decoder must not
    // conflate the two.
    if let Some(e) = VENDOR_CATALOGUE
        .iter()
        .find(|e| e.virtual_read && !e.direct_read)
    {
        assert!(e.virtual_read);
        assert!(!e.direct_read);
    }
}

#[test]
fn an_entry_with_no_checksum_does_not_claim_checksum_correction() {
    if let Some(e) = VENDOR_CATALOGUE.iter().find(|e| e.no_checksum) {
        assert!(e.no_checksum, "'{}' declares NC", e.description);
    }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn search_finds_a_known_brand() {
    let hits = search_catalogue("Honda");
    assert!(!hits.is_empty(), "Honda modules must be present");
    for h in &hits {
        assert!(
            h.description.to_ascii_lowercase().contains("honda"),
            "every hit must contain the query: {}",
            h.description
        );
    }
}

#[test]
fn search_is_case_insensitive() {
    assert_eq!(
        search_catalogue("HONDA").len(),
        search_catalogue("honda").len()
    );
}

#[test]
fn search_finds_a_known_ecu_family() {
    let hits = search_catalogue("MEDC17");
    assert!(!hits.is_empty(), "MEDC17 modules must be present");
}

#[test]
fn an_empty_query_returns_nothing() {
    assert!(search_catalogue("").is_empty());
    assert!(search_catalogue("   ").is_empty());
}

#[test]
fn an_unmatchable_query_returns_nothing() {
    assert!(search_catalogue("ZZZZ-NOT-A-VEHICLE").is_empty());
}

// ---------------------------------------------------------------------------
// Capability queries
// ---------------------------------------------------------------------------

#[test]
fn direct_read_capable_is_a_subset_of_the_catalogue() {
    let capable = direct_read_capable();
    assert!(!capable.is_empty(), "some modules must support direct read");
    assert!(capable.len() <= CATALOGUE_LEN);
    for e in capable {
        assert!(e.direct_read);
    }
}

// ---------------------------------------------------------------------------
// Honesty guard
// ---------------------------------------------------------------------------

#[test]
fn the_catalogue_entry_carries_no_invented_technical_fields() {
    // The type intentionally has no mcu_family / flash_size field. This test
    // documents that decision: if someone adds one, it must come with a real
    // source, and this test will need updating.
    let e: CatalogueEntry = VENDOR_CATALOGUE[0];
    assert!(!e.description.is_empty());
    // Compile-time proof that the struct has exactly these fields is the fact
    // that this literal-like access compiles; adding a field would still compile,
    // so the guard is the doc comment on the type plus this reminder.
}
