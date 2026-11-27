pub mod arithmetic;
pub mod custom_plugin;
pub mod denso_descriptor;
pub mod eeprom_me7;
pub mod multi_ecu;
pub mod oem;
pub mod plugins {
    pub mod honda_keihin;
}
pub mod primitives;
pub mod rolling_rom_table;
pub mod xdf_checksum;

pub use plugins::honda_keihin::{fix_keihin_checksum, verify_keihin_checksum, KeihinChecksumError};
pub use primitives::{
    compute_crc32, compute_crc32_with_initial, crc16_arc, crc16_ccitt, crc32_castagnoli,
    crc32_simos, crc32_unreflected, crc8_maxim, sum_modulo_256, sum_modulo_65536,
    twos_complement_16, twos_complement_8,
};
