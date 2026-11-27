//! Engine physics primitives.
//!
//! Ported from `ecu_engine_rust::physics`. The calibration maths layer: unit
//! conversions, charge temperature, speed density, fuel metering, combustion
//! timing, torque structure, sensors, airflow, transients, and the specialised
//! domain models.
//!
//! ## Relationship to existing target code
//!
//! `kivi-protocol/autotune` is a **recommendation engine over telemetry**. This
//! crate is the **physics primitives** beneath it — different layers. AutoTune
//! decides what to change; these functions compute the quantities those
//! decisions rest on.
//!
//! ## Safety
//!
//! Pure maths. No I/O, no hardware access, deterministic.
//!
//! ## Verification status
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU — there is no device in this environment, and no such
//! claim is made anywhere in this crate.

// Style lints relaxed for the verbatim port; correctness lints stay denied.
#![allow(
    clippy::manual_range_contains,
    clippy::manual_is_multiple_of,
    clippy::should_implement_trait,
    clippy::inherent_to_string,
    clippy::op_ref,
    clippy::needless_range_loop,
    clippy::manual_div_ceil,
    clippy::needless_borrow
)]

pub mod airflow_dynamics;
pub mod autotune;
pub mod axis_rescaling;
pub mod bin_stacker;
pub mod boost;
pub mod chemistry;
pub mod dtc;
pub mod fuel;
pub mod ignition;
pub mod motorsport;
pub mod multi_cylinder;
pub mod sensors;
pub mod stage2;
pub mod supercar;
pub mod thermo;
pub mod torque;
pub mod transients;
