//! Static ECU protocol module scope registry (99 ECU coverage entries).
//!
//! ## Structure
//!
//! Each entry pairs a sequential index, a protocol module number (e.g.
//! `MODULE 36(063)` -> index `36`, module number `63`), and its human-readable
//! ECU family coverage description.
//!
//! ## Scope Note
//!
//! This registry provides **coverage metadata** only — listing which ECU families
//! correspond to numbered protocol modules without claiming live flashing
//! support unless backed by verified protocol and adapter drivers.
//!
//! ## What is deliberately absent
//!
//! MCU part numbers, flash size, and checksum algorithms are omitted unless
//! independently verified, avoiding wrong-geometry write assumptions.

/// One static ECU protocol module scope entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceModule {
    /// Sequential module file index (e.g. `36`).
    pub file_index: u32,
    /// Protocol module number from the coverage descriptor (e.g. `63`).
    pub vendor_number: u32,
    /// ECU family coverage description.
    pub coverage: &'static str,
    /// Descriptor binary payload size metadata (bytes).
    pub file_size: u32,
    /// Number of encrypted RCDATA blobs in the file.
    ///
    /// Only one module ships any, which is why the rest of this crate could be
    /// built from protocol knowledge alone.
    pub encrypted_blobs: u32,
}

/// Number of modules in the registry.
pub const REFERENCE_MODULE_COUNT: usize = 99;

/// Every shipped module, in file-index order.
pub const REFERENCE_MODULES: &[ReferenceModule] = &[

    ReferenceModule { file_index: 1, vendor_number: 42, coverage: "MH7201,*7202,*7203,*8206,*8305,*8306", file_size: 20480, encrypted_blobs: 0 },
    ReferenceModule { file_index: 2, vendor_number: 44, coverage: "MH5002,*8102,*8201,*8202,*8301,*8302,*8303,*8304", file_size: 21504, encrypted_blobs: 0 },
    ReferenceModule { file_index: 3, vendor_number: 34, coverage: "MH8104,*8106", file_size: 21504, encrypted_blobs: 0 },
    ReferenceModule { file_index: 4, vendor_number: 35, coverage: "Mazda", file_size: 16896, encrypted_blobs: 0 },
    ReferenceModule { file_index: 5, vendor_number: 4, coverage: "Yamaha MH8102 MH8115 MH8106", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 6, vendor_number: 22, coverage: "Mitsubishi L200 + Pajero", file_size: 13824, encrypted_blobs: 0 },
    ReferenceModule { file_index: 7, vendor_number: 19, coverage: "Mazda Restyle", file_size: 15360, encrypted_blobs: 0 },
    ReferenceModule { file_index: 8, vendor_number: 6, coverage: "Ford SIM28", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 9, vendor_number: 16, coverage: "Ford Visteon ESU-131/411/418", file_size: 15360, encrypted_blobs: 0 },
    ReferenceModule { file_index: 10, vendor_number: 16, coverage: "Hyundai SIM2K", file_size: 15872, encrypted_blobs: 0 },
    ReferenceModule { file_index: 11, vendor_number: 14, coverage: "Ford Visteon ESU-412", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 12, vendor_number: 72, coverage: "VAG SIMOS PCR 2.1, 8.XX", file_size: 36352, encrypted_blobs: 0 },
    ReferenceModule { file_index: 13, vendor_number: 1, coverage: "Mazda Diesel", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 14, vendor_number: 18, coverage: "Ford delphi DCM 3.5 CAN", file_size: 14848, encrypted_blobs: 0 },
    ReferenceModule { file_index: 15, vendor_number: 11, coverage: "Ford Bosch ME 9.0", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 17, vendor_number: 7, coverage: "BMW Continental MSV80/MSD81", file_size: 25600, encrypted_blobs: 0 },
    ReferenceModule { file_index: 18, vendor_number: 15, coverage: "Mazda mh8110", file_size: 14336, encrypted_blobs: 0 },
    ReferenceModule { file_index: 19, vendor_number: 5, coverage: "Ford SIM210/22", file_size: 13824, encrypted_blobs: 0 },
    ReferenceModule { file_index: 20, vendor_number: 7, coverage: "Hyundai Bosch", file_size: 19968, encrypted_blobs: 0 },
    ReferenceModule { file_index: 21, vendor_number: 3, coverage: "Ford Sid-206", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 22, vendor_number: 14, coverage: "Ford EMS-2101,2102,2103", file_size: 13824, encrypted_blobs: 0 },
    ReferenceModule { file_index: 23, vendor_number: 2, coverage: "Mazda Denso CX5 2.2D", file_size: 14336, encrypted_blobs: 0 },
    ReferenceModule { file_index: 24, vendor_number: 10, coverage: "Mazda Z6 1.6 Denso", file_size: 14336, encrypted_blobs: 0 },
    ReferenceModule { file_index: 25, vendor_number: 25, coverage: "MH8115F", file_size: 15360, encrypted_blobs: 0 },
    ReferenceModule { file_index: 27, vendor_number: 16, coverage: "Ford Bosch MED17.XX", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 28, vendor_number: 42, coverage: "Ford EMS-2204", file_size: 15872, encrypted_blobs: 0 },
    ReferenceModule { file_index: 29, vendor_number: 61, coverage: "Subaru Hitachi\\Ujecs", file_size: 16384, encrypted_blobs: 0 },
    ReferenceModule { file_index: 30, vendor_number: 105, coverage: "Honda K-line", file_size: 103936, encrypted_blobs: 0 },
    ReferenceModule { file_index: 31, vendor_number: 12, coverage: "Mazda SkyA 2.0 2.5 (ECU Mitsubishi)", file_size: 23040, encrypted_blobs: 0 },
    ReferenceModule { file_index: 32, vendor_number: 29, coverage: "VW MM/SIMOS K-line", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 33, vendor_number: 8, coverage: "Mazda Denso 2.0i 2.5i SkyA", file_size: 14336, encrypted_blobs: 0 },
    ReferenceModule { file_index: 34, vendor_number: 402, coverage: "NISSAN M32R/SH72xxx TCM/CVT", file_size: 141312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 35, vendor_number: 28, coverage: "PSA SID208", file_size: 19968, encrypted_blobs: 0 },
    ReferenceModule { file_index: 36, vendor_number: 63, coverage: "Denso SH FULL + EEPROM CAN", file_size: 333824, encrypted_blobs: 15 },
    ReferenceModule { file_index: 37, vendor_number: 90, coverage: "Nissan Hitachi CAN", file_size: 24064, encrypted_blobs: 0 },
    ReferenceModule { file_index: 38, vendor_number: 510, coverage: "Honda CAN", file_size: 16384, encrypted_blobs: 0 },
    ReferenceModule { file_index: 39, vendor_number: 115, coverage: "Nissan K-line", file_size: 19968, encrypted_blobs: 0 },
    ReferenceModule { file_index: 40, vendor_number: 4, coverage: "Suzuki", file_size: 12288, encrypted_blobs: 0 },
    ReferenceModule { file_index: 41, vendor_number: 49, coverage: "BMW Bosch E-Net", file_size: 54272, encrypted_blobs: 0 },
    ReferenceModule { file_index: 42, vendor_number: 17, coverage: "Renaut", file_size: 20480, encrypted_blobs: 0 },
    ReferenceModule { file_index: 43, vendor_number: 125, coverage: "Nissan Hitachi Generation 2 CAN", file_size: 21504, encrypted_blobs: 0 },
    ReferenceModule { file_index: 44, vendor_number: 21, coverage: "Toyota CAN", file_size: 16896, encrypted_blobs: 0 },
    ReferenceModule { file_index: 45, vendor_number: 40, coverage: "Subaru Denso", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 46, vendor_number: 8, coverage: "Ford DCU-101", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 47, vendor_number: 128, coverage: "Ford USA", file_size: 113664, encrypted_blobs: 0 },
    ReferenceModule { file_index: 48, vendor_number: 22, coverage: "Mazda Denso Crypted", file_size: 15360, encrypted_blobs: 0 },
    ReferenceModule { file_index: 49, vendor_number: 36, coverage: "Ford SID208/209", file_size: 19968, encrypted_blobs: 0 },
    ReferenceModule { file_index: 50, vendor_number: 5, coverage: "Kia/Hyundai", file_size: 16384, encrypted_blobs: 0 },
    ReferenceModule { file_index: 51, vendor_number: 3, coverage: "EMS2214", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 52, vendor_number: 3, coverage: "EMS2205", file_size: 13824, encrypted_blobs: 0 },
    ReferenceModule { file_index: 53, vendor_number: 5, coverage: "EMS2211", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 54, vendor_number: 3, coverage: "EMS2510/2511", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 55, vendor_number: 3, coverage: "DCM6.1", file_size: 15360, encrypted_blobs: 0 },
    ReferenceModule { file_index: 56, vendor_number: 243, coverage: "Bosch UDS ME(D)17/EDC17 CAN", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 57, vendor_number: 3, coverage: "Ford Focus 3 TCM PowerShift", file_size: 17408, encrypted_blobs: 0 },
    ReferenceModule { file_index: 59, vendor_number: 10, coverage: "Valeo V46", file_size: 12288, encrypted_blobs: 0 },
    ReferenceModule { file_index: 60, vendor_number: 1, coverage: "Ford EMS-2302", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 61, vendor_number: 22, coverage: "VAG ECM Update(.frf/.sgo)", file_size: 65536, encrypted_blobs: 0 },
    ReferenceModule { file_index: 62, vendor_number: 96, coverage: "VAG TP2.0/KWP2000", file_size: 117760, encrypted_blobs: 0 },
    ReferenceModule { file_index: 63, vendor_number: 27, coverage: "Mitsubishi Coding", file_size: 12288, encrypted_blobs: 0 },
    ReferenceModule { file_index: 65, vendor_number: 10, coverage: "Suzuki 8115 CAN", file_size: 14848, encrypted_blobs: 0 },
    ReferenceModule { file_index: 66, vendor_number: 8, coverage: "Ford EMS24xx", file_size: 84480, encrypted_blobs: 0 },
    ReferenceModule { file_index: 67, vendor_number: 30, coverage: "BMW Bosch CAN", file_size: 300032, encrypted_blobs: 0 },
    ReferenceModule { file_index: 68, vendor_number: 1, coverage: "Rotax Bosch CAN", file_size: 14336, encrypted_blobs: 0 },
    ReferenceModule { file_index: 69, vendor_number: 85, coverage: "Denso Gen2", file_size: 43008, encrypted_blobs: 0 },
    ReferenceModule { file_index: 70, vendor_number: 118, coverage: "Isuzu Transtron", file_size: 23552, encrypted_blobs: 0 },
    ReferenceModule { file_index: 71, vendor_number: 14, coverage: "TCM Mitsubishi", file_size: 27648, encrypted_blobs: 0 },
    ReferenceModule { file_index: 72, vendor_number: 8, coverage: "Ford DCT450", file_size: 58368, encrypted_blobs: 0 },
    ReferenceModule { file_index: 73, vendor_number: 111, coverage: "Mercedes BOSCH", file_size: 20480, encrypted_blobs: 0 },
    ReferenceModule { file_index: 74, vendor_number: 1, coverage: "TCM Mazda", file_size: 17920, encrypted_blobs: 0 },
    ReferenceModule { file_index: 75, vendor_number: 2, coverage: "Mazda Gen1", file_size: 32256, encrypted_blobs: 0 },
    ReferenceModule { file_index: 76, vendor_number: 1, coverage: "Honda bike kline", file_size: 34816, encrypted_blobs: 0 },
    ReferenceModule { file_index: 78, vendor_number: 11, coverage: "Mazda Gen4", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 79, vendor_number: 14, coverage: "Yamaha SH725xx", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 80, vendor_number: 30, coverage: "Nissan UDS", file_size: 19456, encrypted_blobs: 0 },
    ReferenceModule { file_index: 81, vendor_number: 8, coverage: "Mitsubishi FUSO EDC17", file_size: 17920, encrypted_blobs: 0 },
    ReferenceModule { file_index: 82, vendor_number: 17, coverage: "Isuzu Denso", file_size: 17920, encrypted_blobs: 0 },
    ReferenceModule { file_index: 83, vendor_number: 2, coverage: "Nissan Hitachi disable immobilizer", file_size: 16896, encrypted_blobs: 0 },
    ReferenceModule { file_index: 85, vendor_number: 14, coverage: "Isuzu Transtron with patch ROM", file_size: 32256, encrypted_blobs: 0 },
    ReferenceModule { file_index: 86, vendor_number: 18, coverage: "Isuzu UDS", file_size: 16384, encrypted_blobs: 0 },
    ReferenceModule { file_index: 87, vendor_number: 11, coverage: "TCM PowerStroke", file_size: 79872, encrypted_blobs: 0 },
    ReferenceModule { file_index: 88, vendor_number: 2, coverage: "Ford SID211", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 89, vendor_number: 29, coverage: "Ford SID208 OBD", file_size: 14336, encrypted_blobs: 0 },
    ReferenceModule { file_index: 91, vendor_number: 1, coverage: "Isuzu ACM EDC17C09", file_size: 40448, encrypted_blobs: 0 },
    ReferenceModule { file_index: 92, vendor_number: 0, coverage: "Mitsubishi MH8601 Recovery", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 93, vendor_number: 4, coverage: "Suzuki MH8610 Recovery", file_size: 12800, encrypted_blobs: 0 },
    ReferenceModule { file_index: 94, vendor_number: 3, coverage: "Kawasaki MH8610 Recovery", file_size: 12288, encrypted_blobs: 0 },
    ReferenceModule { file_index: 95, vendor_number: 2, coverage: "Ford EDC17C05", file_size: 37888, encrypted_blobs: 0 },
    ReferenceModule { file_index: 96, vendor_number: 3, coverage: "(Slave)Kawasaki MH8610 Recovery", file_size: 11264, encrypted_blobs: 0 },
    ReferenceModule { file_index: 97, vendor_number: 1, coverage: "Mazda Mitsu MH8501 CAN", file_size: 13312, encrypted_blobs: 0 },
    ReferenceModule { file_index: 98, vendor_number: 3, coverage: "Nissan EV/HEV", file_size: 15360, encrypted_blobs: 0 },
    ReferenceModule { file_index: 99, vendor_number: 1, coverage: "Kenwroth Delphi ECT3", file_size: 51712, encrypted_blobs: 0 },
    ReferenceModule { file_index: 100, vendor_number: 6, coverage: "NISSAN RE5 TCM", file_size: 14336, encrypted_blobs: 0 },
    ReferenceModule { file_index: 101, vendor_number: 5, coverage: "Renault RH850", file_size: 39936, encrypted_blobs: 0 },
    ReferenceModule { file_index: 103, vendor_number: 34, coverage: "Nissan Gen2", file_size: 16384, encrypted_blobs: 0 },
    ReferenceModule { file_index: 104, vendor_number: 9, coverage: "Nissan Gen1 Full", file_size: 16896, encrypted_blobs: 0 },
    ReferenceModule { file_index: 105, vendor_number: 9, coverage: "Isuzu Delphi", file_size: 15872, encrypted_blobs: 0 },
    ReferenceModule { file_index: 106, vendor_number: 5, coverage: "Denso MPC", file_size: 15872, encrypted_blobs: 0 },
    ReferenceModule { file_index: 107, vendor_number: 4, coverage: "Nissan SH72531 EV/HEV Full", file_size: 16384, encrypted_blobs: 0 },
];

/// Find modules whose coverage mentions `query` (case-insensitive).
///
/// Searches the coverage text only, because that is the field a user would
/// recognise. An empty query returns nothing rather than the whole table.
pub fn find_modules(query: &str) -> Vec<&'static ReferenceModule> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    REFERENCE_MODULES
        .iter()
        .filter(|m| m.coverage.to_ascii_lowercase().contains(&q))
        .collect()
}

/// Look up one module by file index.
pub fn module_by_index(index: u32) -> Option<&'static ReferenceModule> {
    REFERENCE_MODULES.iter().find(|m| m.file_index == index)
}

/// Every module whose coverage mentions `brand`.
///
/// A thin wrapper that makes the common question ("which modules cover Honda?")
/// read clearly at the call site.
pub fn modules_for_brand(brand: &str) -> Vec<&'static ReferenceModule> {
    find_modules(brand)
}

/// Total bytes across all shipped modules.
pub fn total_module_bytes() -> u64 {
    REFERENCE_MODULES.iter().map(|m| m.file_size as u64).sum()
}

/// Modules that ship encrypted payload blobs.
pub fn modules_with_encrypted_payload() -> Vec<&'static ReferenceModule> {
    REFERENCE_MODULES
        .iter()
        .filter(|m| m.encrypted_blobs > 0)
        .collect()
}

