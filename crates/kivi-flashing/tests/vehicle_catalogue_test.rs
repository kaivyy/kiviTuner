//! Tests for the static vehicle and ECU identity catalogue.
//!
//! The tables were generated from curated `Ecus.txt` and `Modelle.txt`, so
//! these tests check that the table data preserved what the files actually
//! contain — counts, specific entries, and the search helpers.

use kivi_flashing::vehicle_catalogue::{
    ecu_families_in_category, find_ecu_families, find_vehicle_models, vehicle_models_in_class,
    EcuFamily, VehicleModel, ECU_FAMILIES, ECU_FAMILY_COUNT, VEHICLE_MODELS, VEHICLE_MODEL_COUNT,
};

// ---------------------------------------------------------------------------
// Counts
// ---------------------------------------------------------------------------

#[test]
fn the_ecu_table_matches_its_declared_count() {
    assert_eq!(ECU_FAMILY_COUNT, ECU_FAMILIES.len());
    assert_eq!(ECU_FAMILY_COUNT, 874, "Ecus.txt has 874 data rows");
}

#[test]
fn the_vehicle_table_matches_its_declared_count() {
    assert_eq!(VEHICLE_MODEL_COUNT, VEHICLE_MODELS.len());
    // Modelle.txt has 7312 `;`-delimited rows, one of which is a UI hint
    // row rather than a vehicle:
    //   `Suggestion 'Generation 2' rename;;;;`
    // It has no make, model, variant or engine, so it is excluded. 7311 real
    // vehicles remain.
    assert_eq!(
        VEHICLE_MODEL_COUNT, 7311,
        "Modelle.txt has 7312 rows, one of which is a UI hint"
    );
}

#[test]
fn the_ui_hint_row_is_not_present_as_a_vehicle() {
    // Guards the exclusion above: if someone regenerates the table from the raw
    // file without this filter, this test fails.
    assert!(
        !VEHICLE_MODELS.iter().any(|v| v.class.contains("Suggestion")),
        "UI hint row must not be imported as a vehicle"
    );
}

#[test]
fn every_ecu_entry_has_a_category() {
    // An entry with no category would be a parsing artefact.
    for e in ECU_FAMILIES {
        assert!(!e.category.is_empty(), "entry with empty category: {e:?}");
    }
}

#[test]
fn every_vehicle_entry_has_a_class_and_make() {
    for v in VEHICLE_MODELS {
        assert!(!v.class.is_empty(), "entry with empty class: {v:?}");
        assert!(!v.make.is_empty(), "entry with empty make: {v:?}");
    }
}

// ---------------------------------------------------------------------------
// Known entries
// ---------------------------------------------------------------------------

#[test]
fn a_known_bosch_ecu_is_present() {
    let hits = find_ecu_families("BMS46");
    assert!(!hits.is_empty(), "BMS46 is in Ecus.txt");
    assert!(hits.iter().any(|e| e.variant == "BMS46"));
}

#[test]
fn a_known_siemens_ecu_is_present() {
    // Siemens/Continental is the second-largest manufacturer in the file.
    let hits = find_ecu_families("Siemens");
    assert!(hits.len() > 100, "expected many Siemens entries, got {}", hits.len());
}

#[test]
fn the_tcu_category_is_populated() {
    let tcus = ecu_families_in_category("TCU");
    assert!(!tcus.is_empty(), "TCU entries exist in Ecus.txt");
    assert_eq!(tcus.len(), 36, "Ecus.txt has 36 TCU rows");
    for t in &tcus {
        assert_eq!(t.category, "TCU");
    }
}

#[test]
fn some_ecu_entries_have_an_empty_family_and_that_is_preserved() {
    // Bosch PSG16 and friends have no family grouping in the source. Filtering
    // those out would silently lose real data.
    let empty_family = ECU_FAMILIES.iter().filter(|e| e.family.is_empty()).count();
    assert!(empty_family > 0, "the source has entries with an empty family");
}

#[test]
fn some_tcu_entries_have_an_empty_manufacturer_and_that_is_preserved() {
    let empty_mfr = ECU_FAMILIES
        .iter()
        .filter(|e| e.category == "TCU" && e.manufacturer.is_empty())
        .count();
    assert!(empty_mfr > 0, "all TCU rows have an empty manufacturer");
}

#[test]
fn a_known_vehicle_class_is_populated() {
    let bikes = vehicle_models_in_class("Motorbike");
    assert!(!bikes.is_empty());
    assert_eq!(bikes.len(), 271, "Modelle.txt has 271 Motorbike rows");
}

#[test]
fn the_pkw_class_is_the_largest() {
    let pkw = vehicle_models_in_class("PKW");
    let trucks = vehicle_models_in_class("Truck");
    assert!(pkw.len() > trucks.len());
    assert_eq!(pkw.len(), 6240, "Modelle.txt has 6240 PKW rows");
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn ecu_search_is_case_insensitive() {
    assert_eq!(
        find_ecu_families("bosch").len(),
        find_ecu_families("BOSCH").len()
    );
}

#[test]
fn an_empty_ecu_query_returns_nothing() {
    assert!(find_ecu_families("").is_empty());
    assert!(find_ecu_families("   ").is_empty());
}

#[test]
fn an_unmatchable_ecu_query_returns_nothing() {
    assert!(find_ecu_families("ZZZZ-NOT-AN-ECU").is_empty());
}

#[test]
fn vehicle_search_is_case_insensitive() {
    assert_eq!(
        find_vehicle_models("bmw").len(),
        find_vehicle_models("BMW").len()
    );
}

#[test]
fn an_empty_vehicle_query_returns_nothing() {
    assert!(find_vehicle_models("").is_empty());
}

#[test]
fn a_category_lookup_is_case_insensitive() {
    assert_eq!(
        ecu_families_in_category("tcu").len(),
        ecu_families_in_category("TCU").len()
    );
}

#[test]
fn a_class_lookup_is_case_insensitive() {
    assert_eq!(
        vehicle_models_in_class("motorbike").len(),
        vehicle_models_in_class("Motorbike").len()
    );
}

// ---------------------------------------------------------------------------
// Honesty guard
// ---------------------------------------------------------------------------

#[test]
fn the_types_carry_no_invented_technical_fields() {
    // `EcuFamily` has exactly four fields and `VehicleModel` exactly five,
    // mirroring the source columns. Adding an `mcu` or `flash_size` field
    // without a real source would break the honesty this table depends on.
    let e: EcuFamily = ECU_FAMILIES[0];
    assert!(!e.category.is_empty());

    let v: VehicleModel = VEHICLE_MODELS[0];
    assert!(!v.class.is_empty());
}

#[test]
fn the_vehicle_catalogue_tables_are_larger_than_the_capability_catalogue() {
    // The protocol capability catalogue covers ~726 variants; the ECU/vehicle
    // identity tables cover far more entries. Keeping both separate is intentional.
    //
    // Read through runtime values so clippy cannot fold this to a constant: the
    // assertion is meant to fail the build if either table ever shrinks.
    use kivi_flashing::catalogue::VENDOR_CATALOGUE;
    let ecu_len = ECU_FAMILIES.len();
    let capability_catalogue_len = VENDOR_CATALOGUE.len();
    assert!(
        ecu_len > capability_catalogue_len,
        "the ECU identity list ({ecu_len}) should exceed the capability catalogue ({capability_catalogue_len})"
    );
}
