//! Behavioral parity tests for `kivi_core::sector` against the reference
//! implementation in `ecu_engine_rust::flashing::nissan_hitachi`
//! (`DifferentialFlashPlanner`).
//!
//! Expected values were produced by executing the **unmodified** extracted
//! source through a disposable oracle harness. They are not hand-derived.

use kivi_core::sector::{
    plan_differential_flash, total_bytes_to_flash, DifferentialSectorPlan, SectorPlanError,
};

fn sector(
    idx: usize,
    start: u32,
    len: usize,
    modified: bool,
    bytes: usize,
) -> DifferentialSectorPlan {
    DifferentialSectorPlan {
        sector_index: idx,
        start_address: start,
        length: len,
        is_modified: modified,
        modified_bytes_count: bytes,
    }
}

#[test]
fn parity_plan_identical_single_sector() {
    let stock = vec![0xAA; 8];
    let tuned = vec![0xAA; 8];
    let plan = plan_differential_flash(&stock, &tuned, &[8]).expect("plan");
    assert_eq!(plan, vec![sector(0, 0x0, 8, false, 0)]);
    assert_eq!(total_bytes_to_flash(&plan), 0);
}

#[test]
fn parity_plan_one_byte_changed() {
    let stock = vec![0xAA; 8];
    let mut tuned = vec![0xAA; 8];
    tuned[1] = 0xAB;
    let plan = plan_differential_flash(&stock, &tuned, &[8]).expect("plan");
    assert_eq!(plan, vec![sector(0, 0x0, 8, true, 1)]);
    assert_eq!(total_bytes_to_flash(&plan), 8);
}

#[test]
fn parity_plan_two_sectors_first_changed() {
    let stock = vec![0x00; 8];
    let mut tuned = vec![0x00; 8];
    tuned[0] = 0x01;
    let plan = plan_differential_flash(&stock, &tuned, &[4, 4]).expect("plan");
    assert_eq!(
        plan,
        vec![sector(0, 0x0, 4, true, 1), sector(1, 0x4, 4, false, 0)]
    );
    assert_eq!(total_bytes_to_flash(&plan), 4);
}

#[test]
fn parity_plan_two_sectors_both_changed() {
    let stock = vec![0x00; 8];
    let mut tuned = vec![0x00; 8];
    tuned[0] = 0x01;
    tuned[7] = 0x02;
    let plan = plan_differential_flash(&stock, &tuned, &[4, 4]).expect("plan");
    assert_eq!(
        plan,
        vec![sector(0, 0x0, 4, true, 1), sector(1, 0x4, 4, true, 1)]
    );
    assert_eq!(total_bytes_to_flash(&plan), 8);
}

#[test]
fn parity_plan_sector_sizes_exceed_rom_stops_at_end() {
    // Sector list is longer than the ROM; iteration stops once the ROM ends.
    let stock = vec![0x00; 4];
    let tuned = vec![0x01; 4];
    let plan = plan_differential_flash(&stock, &tuned, &[2, 2, 2, 2]).expect("plan");
    assert_eq!(
        plan,
        vec![sector(0, 0x0, 2, true, 2), sector(1, 0x2, 2, true, 2)]
    );
    assert_eq!(total_bytes_to_flash(&plan), 4);
}

#[test]
fn parity_plan_sector_sizes_undershoot_rom() {
    // Only 4 of 6 bytes are covered; the trailing bytes are never planned.
    let stock = vec![0x00; 6];
    let tuned = vec![0x00; 6];
    let plan = plan_differential_flash(&stock, &tuned, &[4]).expect("plan");
    assert_eq!(plan, vec![sector(0, 0x0, 4, false, 0)]);
    assert_eq!(total_bytes_to_flash(&plan), 0);
}

#[test]
fn parity_plan_empty_rom_empty_sectors() {
    let plan = plan_differential_flash(&[], &[], &[]).expect("plan");
    assert!(plan.is_empty());
    assert_eq!(total_bytes_to_flash(&plan), 0);
}

#[test]
fn parity_plan_empty_sector_list_nonempty_rom() {
    let plan = plan_differential_flash(&[0x00; 4], &[0x01; 4], &[]).expect("plan");
    assert!(plan.is_empty());
}

#[test]
fn parity_plan_zero_length_sector_then_data() {
    // A zero-length sector produces a zero-length entry at the same offset.
    let stock = vec![0x00; 4];
    let tuned = vec![0x01; 4];
    let plan = plan_differential_flash(&stock, &tuned, &[0, 4]).expect("plan");
    assert_eq!(
        plan,
        vec![sector(0, 0x0, 0, false, 0), sector(1, 0x0, 4, true, 4)]
    );
    assert_eq!(total_bytes_to_flash(&plan), 4);
}

#[test]
fn parity_plan_length_mismatch_is_error() {
    let err = plan_differential_flash(&[0x00; 3], &[0x00; 4], &[4])
        .expect_err("length mismatch must be rejected");
    assert_eq!(err, SectorPlanError::LengthMismatch);
    // The externally observable message must match the source string exactly.
    assert_eq!(
        err.to_string(),
        "Stock and Tuned ROM sizes must match for differential planning"
    );
}

#[test]
fn parity_plan_all_ff_vs_all_zero() {
    let plan = plan_differential_flash(&[0xFF; 4], &[0x00; 4], &[4]).expect("plan");
    assert_eq!(plan, vec![sector(0, 0x0, 4, true, 4)]);
    assert_eq!(total_bytes_to_flash(&plan), 4);
}

#[test]
fn plan_is_deterministic() {
    let stock: Vec<u8> = (0..64u8).collect();
    let tuned: Vec<u8> = (0..64u8).map(|b| b.wrapping_add(1)).collect();
    let first = plan_differential_flash(&stock, &tuned, &[16, 16, 16, 16]).expect("plan");
    let second = plan_differential_flash(&stock, &tuned, &[16, 16, 16, 16]).expect("plan");
    assert_eq!(first, second);
    assert_eq!(total_bytes_to_flash(&first), 64);
}
