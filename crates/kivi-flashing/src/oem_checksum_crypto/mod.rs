//! Ported from `ecu_engine_rust::flashing::oem_checksum_crypto`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.


mod asian_oem;
mod ford_vbf_phf;
mod legacy_misc;
mod vag_bosch;

pub use asian_oem::{
    DensoSh705xBootAndEepromEngine, HondaEcuHardwareType,
    HondaKeihinBikeKLineAndIsuzuTranstronEngine, HondaKeihinShindengenUnifiedCore,
    SimkAndSmartraEngine, SubaruHitachiUnbrickAndSsm2,
};
pub use ford_vbf_phf::{
    VBF_FORMAT01_FEISTEL_P_BOX, VBF_FORMAT01_FEISTEL_S_BOX, VbfPhfAndMcuMapEngine,
};
pub use legacy_misc::{
    BmwDs2AndMoatesBurner, PassthruAndTraceCodec, SaabTrionicSecurityAndChecksum,
    St10BslAndCanBcmEngine,
};
pub use vag_bosch::{
    BoschMe7ChecksumSuite, DsgMechatronicCryptoAndChecksum, SimosChecksumAndCrypto,
    VagComponentProtectionCipher,
};
