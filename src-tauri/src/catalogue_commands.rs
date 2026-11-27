//! Tauri commands for the offline vehicle/ECU identity and capability catalogue.
//!
//! ## What this exposes
//!
//! 874 ECU families and 7,311 vehicle models from the static identity catalogue.
//! The panel uses it to answer "does this vehicle exist in the identity catalogue"
//! before a tuner starts guessing at a ROM.
//!
//! ## Why the result is capped
//!
//! A one-letter query matches thousands of rows. Serialising all of them through
//! IPC would stall the panel, so [`MAX_RESULTS`] bounds every response. The cap
//! is surfaced to the UI so it can say "showing the first N" rather than
//! pretending the list is complete.
//!
//! ## What this is NOT
//!
//! It is an **identity** list, not a capability list. Finding a vehicle here
//! means "this vehicle identity is catalogued", not "we can flash it". The
//! capability list is [`kivi_flashing::catalogue`], which is separate on purpose.

use kivi_flashing::catalogue::{search_catalogue, CatalogueEntry, CATALOGUE_LEN};
use kivi_flashing::chip_catalogue::{
    chip_catalogue_count as flash_chip_count, find_chips, find_chips_by_jedec, ChipCatalogEntry,
};
use kivi_flashing::reference_modules::{find_modules, ReferenceModule, REFERENCE_MODULES};
use kivi_flashing::vehicle_catalogue::{
    find_ecu_families, find_vehicle_models, EcuFamily, VehicleModel, ECU_FAMILY_COUNT,
    VEHICLE_MODEL_COUNT,
};
use serde::{Deserialize, Serialize};

/// Maximum rows returned by one query.
///
/// Chosen so the payload stays well under a megabyte while still being wide
/// enough that a normal search is never truncated.
pub const MAX_RESULTS: usize = 500;

/// Table sizes, for a header line in the panel.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogueStats {
    pub ecu_family_count: usize,
    pub vehicle_model_count: usize,
}

/// One ECU family, as the frontend receives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EcuFamilyRow {
    pub category: String,
    pub manufacturer: String,
    pub family: String,
    pub variant: String,
}

/// One vehicle model, as the frontend receives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VehicleModelRow {
    pub class: String,
    pub make: String,
    pub model: String,
    pub variant: String,
    pub engine: String,
}

/// Table sizes.
pub fn catalogue_stats_inner() -> CatalogueStats {
    CatalogueStats {
        ecu_family_count: ECU_FAMILY_COUNT,
        vehicle_model_count: VEHICLE_MODEL_COUNT,
    }
}

/// Search ECU families.
///
/// An empty query returns nothing rather than the whole table: a panel that
/// dumps 874 rows on open is noise, not information.
pub fn list_ecu_families_inner(query: &str) -> Result<Vec<EcuFamilyRow>, String> {
    let hits: Vec<&EcuFamily> = find_ecu_families(query);
    Ok(hits
        .into_iter()
        .take(MAX_RESULTS)
        .map(|e| EcuFamilyRow {
            category: e.category.to_string(),
            manufacturer: e.manufacturer.to_string(),
            family: e.family.to_string(),
            variant: e.variant.to_string(),
        })
        .collect())
}

/// Search vehicle models. Same cap and same empty-query rule.
pub fn list_vehicle_models_inner(query: &str) -> Result<Vec<VehicleModelRow>, String> {
    let hits: Vec<&VehicleModel> = find_vehicle_models(query);
    Ok(hits
        .into_iter()
        .take(MAX_RESULTS)
        .map(|v| VehicleModelRow {
            class: v.class.to_string(),
            make: v.make.to_string(),
            model: v.model.to_string(),
            variant: v.variant.to_string(),
            engine: v.engine.to_string(),
        })
        .collect())
}

#[tauri::command]
pub fn catalogue_stats() -> CatalogueStats {
    catalogue_stats_inner()
}

#[tauri::command]
pub fn list_ecu_families(query: String) -> Result<Vec<EcuFamilyRow>, String> {
    list_ecu_families_inner(&query)
}

#[tauri::command]
pub fn list_vehicle_models(query: String) -> Result<Vec<VehicleModelRow>, String> {
    list_vehicle_models_inner(&query)
}

/// One memory chip entry, as the frontend receives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChipCatalogueRow {
    pub manufacturer: String,
    pub chip_name: String,
    pub package: String,
    pub programmer_id: Option<u32>,
    pub jedec_ids: Vec<(u32, u32)>,
}

impl From<&ChipCatalogEntry> for ChipCatalogueRow {
    fn from(e: &ChipCatalogEntry) -> Self {
        Self {
            manufacturer: e.manufacturer.clone(),
            chip_name: e.chip_name.clone(),
            package: e.package.clone(),
            programmer_id: e.programmer_id,
            jedec_ids: e.jedec_id_pairs.clone(),
        }
    }
}

/// Total number of chips in the embedded programmer chip catalogue.
pub fn chip_catalogue_count_inner() -> usize {
    flash_chip_count()
}

/// Search embedded memory chips by manufacturer, chip name, or package (capped at `MAX_RESULTS`).
pub fn list_chip_catalogue_inner(query: &str) -> Result<Vec<ChipCatalogueRow>, String> {
    let hits = find_chips(query);
    Ok(hits
        .into_iter()
        .take(MAX_RESULTS)
        .map(ChipCatalogueRow::from)
        .collect())
}

/// Lookup memory chips matching JEDEC `(manufacturer_id, device_id)` (capped at `MAX_RESULTS`).
pub fn lookup_chip_by_jedec_inner(
    manufacturer_id: u32,
    device_id: u32,
) -> Result<Vec<ChipCatalogueRow>, String> {
    let hits = find_chips_by_jedec(manufacturer_id, device_id);
    Ok(hits
        .into_iter()
        .take(MAX_RESULTS)
        .map(ChipCatalogueRow::from)
        .collect())
}

#[tauri::command]
pub fn chip_catalogue_count() -> usize {
    chip_catalogue_count_inner()
}

#[tauri::command]
pub fn list_chip_catalogue(query: String) -> Result<Vec<ChipCatalogueRow>, String> {
    list_chip_catalogue_inner(&query)
}

#[tauri::command]
pub fn lookup_chip_by_jedec(
    manufacturer_id: u32,
    device_id: u32,
) -> Result<Vec<ChipCatalogueRow>, String> {
    lookup_chip_by_jedec_inner(manufacturer_id, device_id)
}

/// Combined reference catalogue counts across all five offline tables.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuperCatalogueStats {
    pub ecu_family_count: usize,
    pub vehicle_model_count: usize,
    pub chip_catalogue_count: usize,
    pub flasher_capability_count: usize,
    pub reference_module_count: usize,
    pub total_entries: usize,
}

/// One vendor flasher capability row (honestly labelled from shipped capability list).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlasherCapabilityRow {
    pub description: String,
    pub direct_read: bool,
    pub virtual_read: bool,
    pub write_flash: bool,
    pub checksum_correct: bool,
    pub no_checksum: bool,
}

impl From<&CatalogueEntry> for FlasherCapabilityRow {
    fn from(e: &CatalogueEntry) -> Self {
        Self {
            description: e.description.to_string(),
            direct_read: e.direct_read,
            virtual_read: e.virtual_read,
            write_flash: e.write_flash,
            checksum_correct: e.checksum_correct,
            no_checksum: e.no_checksum,
        }
    }
}

/// One reference binary module inventory entry from the extracted flasher reference modules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferenceModuleRow {
    pub file_index: u32,
    pub vendor_number: u32,
    pub coverage: String,
    pub file_size: u32,
    pub encrypted_blobs: u32,
}

impl From<&ReferenceModule> for ReferenceModuleRow {
    fn from(m: &ReferenceModule) -> Self {
        Self {
            file_index: m.file_index,
            vendor_number: m.vendor_number,
            coverage: m.coverage.to_string(),
            file_size: m.file_size,
            encrypted_blobs: m.encrypted_blobs,
        }
    }
}

pub fn super_catalogue_stats_inner() -> SuperCatalogueStats {
    let ecu_family_count = ECU_FAMILY_COUNT;
    let vehicle_model_count = VEHICLE_MODEL_COUNT;
    let chip_catalogue_count = flash_chip_count();
    let flasher_capability_count = CATALOGUE_LEN;
    let reference_module_count = REFERENCE_MODULES.len();
    SuperCatalogueStats {
        ecu_family_count,
        vehicle_model_count,
        chip_catalogue_count,
        flasher_capability_count,
        reference_module_count,
        total_entries: ecu_family_count
            + vehicle_model_count
            + chip_catalogue_count
            + flasher_capability_count
            + reference_module_count,
    }
}

pub fn list_flasher_capabilities_inner(query: &str) -> Result<Vec<FlasherCapabilityRow>, String> {
    let hits = search_catalogue(query);
    Ok(hits
        .into_iter()
        .take(MAX_RESULTS)
        .map(FlasherCapabilityRow::from)
        .collect())
}

pub fn list_reference_modules_inner(query: &str) -> Result<Vec<ReferenceModuleRow>, String> {
    let hits = find_modules(query);
    Ok(hits
        .into_iter()
        .take(MAX_RESULTS)
        .map(ReferenceModuleRow::from)
        .collect())
}

#[tauri::command]
pub fn super_catalogue_stats() -> SuperCatalogueStats {
    super_catalogue_stats_inner()
}

#[tauri::command]
pub fn list_flasher_capabilities(query: String) -> Result<Vec<FlasherCapabilityRow>, String> {
    list_flasher_capabilities_inner(&query)
}

#[tauri::command]
pub fn list_reference_modules(query: String) -> Result<Vec<ReferenceModuleRow>, String> {
    list_reference_modules_inner(&query)
}
