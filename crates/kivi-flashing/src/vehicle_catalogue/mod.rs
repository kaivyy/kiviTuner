//! Static vehicle and ECU identity catalogue: ECU families and vehicle models.
//!
//! ## Contents
//!
//! Generated offline from curated ECU family and vehicle model identity tables:
//!
//! - `Ecus.txt` — 874 ECU family entries
//! - `Modelle.txt` — 7,311 vehicle model entries (excluding UI hint row)
//!
//! ## Why this is separate from the capability catalogue
//!
//! [`crate::catalogue`] records protocol capability flags (`RD`, `VR`, `WR`, `CK`)
//! per module variant. This module records **what ECUs and vehicles exist** —
//! an identity list only. Keeping them separate avoids implying flash support
//! where only identity metadata is known.
//!
//! ## Empty fields are preserved intentionally
//!
//! Some source entries have an empty manufacturer (such as `TCU` entries) or an
//! empty family (such as `PSG16`). Those are preserved as empty strings rather
//! than filtered out, avoiding silent data loss.
//!
//! ## What is deliberately absent
//!
//! MCU part numbers, flash size, and checksum algorithms are omitted unless
//! independently verified rather than guessed.

/// One ECU family from the static ECU identity catalogue (`Ecus.txt`).
///
/// Fields correspond to the four columns:
/// `<category>;<manufacturer>;<family>;<variant>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcuFamily {
    /// `Engine`, `TCU`, `CPC`, or `ACM/MCM/EAS`.
    pub category: &'static str,
    pub manufacturer: &'static str,
    pub family: &'static str,
    pub variant: &'static str,
}

/// One vehicle model from the static vehicle identity catalogue (`Modelle.txt`).
///
/// Fields are the five source columns:
/// `<class>;<make>;<model>;<variant>;<engine>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VehicleModel {
    /// `PKW`, `Truck`, `Motorbike`, `Agriculture`, `Construction`, `Boat`, `Bus`.
    pub class: &'static str,
    pub make: &'static str,
    pub model: &'static str,
    pub variant: &'static str,
    pub engine: &'static str,
}

/// Number of ECU families in the table.
pub const ECU_FAMILY_COUNT: usize = 874;

/// Number of vehicle models in the table.
pub const VEHICLE_MODEL_COUNT: usize = 7311;

/// ECU families, in source-file order.
mod ecu_families;
mod models_non_pkw_pre;
mod models_pkw_a;
mod models_pkw_b_c;
mod models_pkw_d_f_h;
mod models_pkw_i_l;
mod models_pkw_m;
mod models_pkw_n_p;
mod models_pkw_r_s;
mod models_pkw_t_v;
mod models_truck;

pub use ecu_families::ECU_FAMILIES;

const EMPTY_VEHICLE_MODEL: VehicleModel = VehicleModel {
    class: "",
    make: "",
    model: "",
    variant: "",
    engine: "",
};

const fn concat_models<const N: usize>(chunks: &[&[VehicleModel]]) -> [VehicleModel; N] {
    let mut out = [EMPTY_VEHICLE_MODEL; N];
    let mut dst = 0;
    let mut ci = 0;
    while ci < chunks.len() {
        let chunk = chunks[ci];
        let mut i = 0;
        while i < chunk.len() {
            out[dst] = chunk[i];
            dst += 1;
            i += 1;
        }
        ci += 1;
    }
    if dst != N {
        panic!("VEHICLE_MODEL_COUNT mismatch in vehicle_catalogue chunks");
    }
    out
}

static VEHICLE_MODELS_ARRAY: [VehicleModel; VEHICLE_MODEL_COUNT] = concat_models(&[
    models_non_pkw_pre::CHUNK_NON_PKW_PRE, models_pkw_a::CHUNK_PKW_A, models_pkw_b_c::CHUNK_PKW_B_C, models_pkw_d_f_h::CHUNK_PKW_D_F_H, models_pkw_i_l::CHUNK_PKW_I_L, models_pkw_m::CHUNK_PKW_M, models_pkw_n_p::CHUNK_PKW_N_P, models_pkw_r_s::CHUNK_PKW_R_S, models_pkw_t_v::CHUNK_PKW_T_V, models_truck::CHUNK_TRUCK
]);

/// Vehicle models, in source-file order.
pub static VEHICLE_MODELS: &[VehicleModel] = &VEHICLE_MODELS_ARRAY;


/// Find ECU families whose manufacturer, family or variant contains `query`.
///
/// Case-insensitive. An empty query returns nothing rather than everything.
pub fn find_ecu_families(query: &str) -> Vec<&'static EcuFamily> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    ECU_FAMILIES
        .iter()
        .filter(|e| {
            e.manufacturer.to_ascii_lowercase().contains(&q)
                || e.family.to_ascii_lowercase().contains(&q)
                || e.variant.to_ascii_lowercase().contains(&q)
        })
        .collect()
}

/// Find vehicle models whose make, model or engine contains `query`.
pub fn find_vehicle_models(query: &str) -> Vec<&'static VehicleModel> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    VEHICLE_MODELS
        .iter()
        .filter(|v| {
            v.make.to_ascii_lowercase().contains(&q)
                || v.model.to_ascii_lowercase().contains(&q)
                || v.engine.to_ascii_lowercase().contains(&q)
        })
        .collect()
}

/// Every ECU family of one category, e.g. `"TCU"`.
pub fn ecu_families_in_category(category: &str) -> Vec<&'static EcuFamily> {
    let c = category.trim();
    ECU_FAMILIES
        .iter()
        .filter(|e| e.category.eq_ignore_ascii_case(c))
        .collect()
}

/// Every vehicle model of one class, e.g. `"Motorbike"`.
pub fn vehicle_models_in_class(class: &str) -> Vec<&'static VehicleModel> {
    let c = class.trim();
    VEHICLE_MODELS
        .iter()
        .filter(|v| v.class.eq_ignore_ascii_case(c))
        .collect()
}
