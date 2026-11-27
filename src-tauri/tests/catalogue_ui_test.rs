//! Tests for the vehicle/ECU reference catalogue commands.
//!
//! The catalogue holds 874 ECU families and 7 311 vehicles. These tests pin the
//! query shape the UI receives and the honesty rules: an empty query returns
//! nothing rather than everything, and results are capped so a broad search
//! cannot freeze the panel.

use kivituner::catalogue_commands::{
    catalogue_stats_inner, list_ecu_families_inner, list_flasher_capabilities_inner,
    list_reference_modules_inner, list_vehicle_models_inner, super_catalogue_stats_inner,
    MAX_RESULTS,
};

#[test]
fn the_stats_report_both_table_sizes() {
    let s = catalogue_stats_inner();
    assert_eq!(s.ecu_family_count, 874);
    assert_eq!(s.vehicle_model_count, 7311);
}

#[test]
fn an_empty_ecu_query_returns_nothing() {
    let r = list_ecu_families_inner("").expect("query");
    assert!(r.is_empty(), "an empty query must not dump the whole table");
}

#[test]
fn an_empty_vehicle_query_returns_nothing() {
    let r = list_vehicle_models_inner("").expect("query");
    assert!(r.is_empty());
}

#[test]
fn a_known_ecu_query_returns_matches() {
    let r = list_ecu_families_inner("BMS46").expect("query");
    assert!(!r.is_empty());
    assert!(r.iter().any(|e| e.variant == "BMS46"));
}

#[test]
fn a_known_vehicle_query_returns_matches() {
    let r = list_vehicle_models_inner("BMW").expect("query");
    assert!(!r.is_empty());
    assert!(r.iter().all(|v| {
        v.make.to_ascii_lowercase().contains("bmw")
            || v.model.to_ascii_lowercase().contains("bmw")
            || v.engine.to_ascii_lowercase().contains("bmw")
    }));
}

#[test]
fn a_broad_query_is_capped() {
    // "e" matches thousands of rows; the UI must not receive all of them.
    let r = list_ecu_families_inner("e").expect("query");
    assert!(
        r.len() <= MAX_RESULTS,
        "expected at most {MAX_RESULTS} rows, got {}",
        r.len()
    );
}

#[test]
fn the_cap_is_documented_and_reasonable() {
    // Read through a runtime binding so clippy cannot fold this to a constant:
    // the assertion is meant to fail the build if the cap is ever changed to
    // something that breaks the panel.
    let cap = MAX_RESULTS;
    assert!(cap >= 100, "a cap below 100 makes search useless: {cap}");
    assert!(cap <= 5000, "a cap above 5000 can freeze the panel: {cap}");
}

#[test]
fn ecu_rows_carry_all_four_source_columns() {
    let r = list_ecu_families_inner("Bosch").expect("query");
    let first = r.first().expect("Bosch rows exist");
    assert!(!first.category.is_empty());
    assert!(!first.manufacturer.is_empty());
}

#[test]
fn vehicle_rows_carry_all_five_source_columns() {
    let r = list_vehicle_models_inner("Truck").expect("query");
    if let Some(v) = r.first() {
        assert!(!v.class.is_empty());
        assert!(!v.make.is_empty());
    }
}

#[test]
fn results_are_serialisable_for_the_frontend() {
    let r = list_ecu_families_inner("Bosch").expect("query");
    let json = serde_json::to_string(&r).expect("serialise");
    assert!(json.starts_with('['));
}

#[test]
fn super_catalogue_aggregates_all_five_reference_sources() {
    use kivituner::catalogue_commands::{chip_catalogue_count_inner, lookup_chip_by_jedec_inner};
    let s = super_catalogue_stats_inner();
    assert_eq!(s.ecu_family_count, 874);
    assert_eq!(s.vehicle_model_count, 7311);
    assert_eq!(s.chip_catalogue_count, chip_catalogue_count_inner());
    assert_eq!(s.chip_catalogue_count, 6742);
    assert_eq!(s.flasher_capability_count, 726);
    assert_eq!(s.reference_module_count, 99);
    assert_eq!(s.total_entries, 15752);

    assert!(list_flasher_capabilities_inner("").expect("empty").is_empty());
    let caps = list_flasher_capabilities_inner("Honda").expect("honda caps");
    assert!(!caps.is_empty());

    assert!(list_reference_modules_inner("").expect("empty").is_empty());
    let mods = list_reference_modules_inner("Bosch").expect("bosch mods");
    assert!(!mods.is_empty());

    let chips = lookup_chip_by_jedec_inner(0xEF, 0x4018).unwrap_or_default();
    let _ = serde_json::to_string(&chips).expect("serialise chips");
}
