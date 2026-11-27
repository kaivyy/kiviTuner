//! Adversarial deterministic unit and integration tests for Milestone 3D-2A:
//! Read-Only OBD-II Transport Architecture and Fault-Tolerant Session Management.
//!
//! Exercises:
//! - Exclusive session ownership and transaction locking.
//! - Step-by-step allowlisted adapter initialization handshake.
//! - Asynchronous timeout and cancellation handling.
//! - Disconnect, I/O errors, and session fault containment.
//! - Prevention of stale-response cross-attribution after timeout.
//! - ISO-TP modulo-16 sequence index rollover and payload size bounding.
//! - Stoichiometric ratio input validation.
//! - Enforcement of closed typed command boundaries (no arbitrary string injection).


mod codec_and_grammar;
mod fault_and_cancel;
mod handshake;
