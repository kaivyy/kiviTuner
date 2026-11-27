//! ECU flashing protocol logic, plus a device layer that can carry it.
//!
//! Ported from `ecu_engine_rust::flashing`, then extended with the transport
//! seam that was missing. Twenty-nine modules covering UDS, CAN-FD, J2534,
//! K-Line, DoIP, TP2.0, bench protocols, RAM kernels, cloning, recovery
//! journaling, chip descriptors, the OEM checksum/CVN machinery, and the
//! device/kernel-session/flash-engine chain.
//!
//! ## Two layers
//!
//! **Protocol logic** — message builders, parsers, encoders, checksum tables
//! and state machines. Input bytes in, output bytes out. No I/O, no `unsafe`.
//!
//! **Transport layer** — [`device`], [`kernel_session`], [`flash_engine`]:
//! the contracts a real driver satisfies, a deterministic mock for testing, and
//! the orchestrator that drives a dump or flash while guaranteeing the ECU gets
//! its stock OS back. Still no I/O and no `unsafe`.
//!
//! ## Where I/O and `unsafe` actually live
//!
//! Two modules break the "no I/O, no `unsafe`" rule, both deliberately:
//!
//! | Module | What it does | Why |
//! |---|---|---|
//! | [`j2534_loader`] | `std::fs::read_dir` to discover driver files | Discovery is filesystem work by definition |
//! | [`j2534_ffi`] | `libloading` + `unsafe` FFI calls | Loading a vendor DLL is an FFI boundary |
//!
//! `j2534_ffi` is behind the **`j2534-ffi` feature, off by default**, so a
//! default build has no `unsafe` and no dynamic linking. `j2534_loader` runs
//! everywhere but only reads directory entries.
//!
//! ## Verification status
//!
//! Unit-tested against hand-derived expectations and against the published SAE
//! J2534 struct layout. **No function has been executed against a physical
//! ECU** — no device exists in this environment and no such claim is made.
//! Loading a driver and resolving its symbols is tested; putting a frame on a
//! bus is not. Hardware acceptance remains the caller's responsibility.
//!
//! ## Security-sensitive components
//!
//! `oem_checksum_crypto`, `specialized_flasher`, `bytecode_security_vm` and
//! `bus_silencing` are included because the project owner confirmed the source is
//! their own work. They are reproduced faithfully and remain clearly marked as
//! security-sensitive in their module docs.

// Lint policy for this crate.
//
// The 24 modules here are a **verbatim port** of the source protocol logic.
// Clippy flags ~50 style lints in them (`needless_range_loop`, `manual
// is_multiple_of`, `manual bit rotation`, …). Rewriting those would touch
// byte-level protocol code that cannot be re-verified against a physical ECU in
// this environment, so the risk of a silent behavioural change outweighs the
// style benefit.
//
// Correctness lints are NOT suppressed: `clippy::correctness` and
// `clippy::suspicious` stay at `deny` via the workspace's `-D warnings`.
#![allow(
    clippy::needless_range_loop,
    clippy::manual_is_multiple_of,
    clippy::manual_range_contains,
    clippy::manual_rotate,
    clippy::manual_memcpy,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::chunks_exact_to_as_chunks,
    clippy::should_implement_trait,
    clippy::inherent_to_string,
    clippy::approx_constant,
    clippy::manual_div_ceil,
    clippy::needless_borrow,
    clippy::op_ref,
    clippy::unnecessary_map_or,
    clippy::match_like_matches_macro
)]

pub mod bus_transmuter;
pub mod adaptations;
pub mod bench_hardware;
pub mod bench_protocol;
pub mod bus_silencing;
pub mod bytecode_security_vm;
pub mod can_fd;
pub mod catalogue;
pub mod chip_catalogue;
pub mod chip_verifier;
pub mod device;
pub mod doip_device;
pub mod doip_transport;
pub mod dtc;
pub mod elm_device;
pub mod ecu_cloning;
pub mod flash_engine;
pub mod flash_safety;
pub mod flash_writer;
pub mod flight_recorder;
pub mod gm_seed_key_tables;
pub mod hardware_kernel_session;
pub mod hardware_matrix;
pub mod honda_bike_probe;
pub mod honda_kline;
pub mod j2534_abi;
pub mod j2534_bridge;
pub mod j2534_channel;
#[cfg(feature = "j2534-ffi")]
pub mod j2534_device;
#[cfg(feature = "j2534-ffi")]
pub mod j2534_ffi;
pub mod j2534_loader;
pub mod j2534_types;
pub mod kernel_session;
pub mod lzss_container;
pub mod module_registry;
pub mod moto_ecu_db;
pub mod nissan_hitachi;
pub mod oem_checksum_crypto;
pub mod openport2_serial;
pub mod passthru_discovery;
pub mod quick_diag;
pub mod ram_kernel;
pub mod reference_modules;
pub mod region_overlap;
pub mod recovery_journal;
pub mod specialized_flasher;
pub mod specialized_kernel_sessions;
pub mod tp20_transport;
pub mod sa2_seed_key;
pub mod seed_key_registry;
pub mod serial_elm_device;
pub mod socketcan_device;
pub mod uds;
pub mod uds_dump;
pub mod universal_dumper;
pub mod vid_block;
pub mod vehicle_catalogue;
