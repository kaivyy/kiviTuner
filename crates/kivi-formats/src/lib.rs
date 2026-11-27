//! Firmware container formats.
//!
//! Currently: the VBF (Versatile Binary Format) container used by Ford, Mazda,
//! Volvo, Jaguar and Land Rover flash files.
//!
//! This crate exists rather than placing VBF in `kivi-core` because
//! `kivi-checksum` depends on `kivi-core`, so a CRC-using container codec in
//! `kivi-core` would either invert that dependency or duplicate the CRC
//! implementation. `kivi-formats` sits above both and reuses the existing
//! `crc16_ccitt`.
//!
//! Everything here is offline: parsing and serializing byte buffers. No device
//! access, no flashing.

pub mod bcb;
pub mod bmw_spdaten;
pub mod container;
pub mod gm_e92_rle;
pub mod odx_flash;
pub mod ols_project;
pub mod sgo_frf_pack;
pub mod vbf;
