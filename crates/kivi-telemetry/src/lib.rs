//! Telemetry parsing and analysis.
//!
//! Ported from `ecu_engine_rust::telemetry`. Pure byte-level parsers and
//! analysers: ADX protocol framing, XCP, histogram hit-tracing, a live
//! emulator, RAM-hook decoding and an onboard logger format.
//!
//! ## Safety
//!
//! No socket, no serial port, no filesystem, no `unsafe`. These functions
//! transform byte buffers; the caller owns transport.
//!
//! ## Verification
//!
//! Unit-tested against hand-derived expectations. **Not** validated against live
//! hardware — no device exists in this environment.
#![allow(
    clippy::needless_range_loop,
    clippy::manual_is_multiple_of,
    clippy::manual_range_contains,
    clippy::should_implement_trait,
    clippy::inherent_to_string,
    clippy::op_ref,
    clippy::manual_div_ceil,
    clippy::collapsible_if
)]

pub mod adx;
pub mod histogram;
pub mod live_emulator;
pub mod onboard_logger;
pub mod ram_hook;
pub mod xcp;
