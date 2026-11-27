//! Behavioral parity tests for `kivi_core::verify` against the reference
//! implementation in `ecu_engine_rust::flashing::chip_verifier`.
//!
//! The expected values below were produced by executing the **unmodified**
//! source module through a disposable oracle harness (see
//! `reference.txt` in the audit scratch directory). They are not hand-derived.
//!
//! If the source implementation changes, regenerate the reference and update
//! this table; do not adjust the assertions to match the target.

use kivi_core::verify::{check_blank, verify_buffer, BlankCheckResult, VerifyDifference};

fn diff(offset: usize, exp: u8, act: u8) -> VerifyDifference {
    VerifyDifference {
        offset,
        expected_local_byte: exp,
        actual_chip_byte: act,
    }
}

// ---------------------------------------------------------------------------
// check_blank — reference cases
// ---------------------------------------------------------------------------

#[test]
fn parity_check_blank_empty_is_blank() {
    assert_eq!(check_blank(&[]), BlankCheckResult::Blank);
}

#[test]
fn parity_check_blank_single_ff_is_blank() {
    assert_eq!(check_blank(&[0xFF]), BlankCheckResult::Blank);
}

#[test]
fn parity_check_blank_all_ff_16_is_blank() {
    assert_eq!(check_blank(&[0xFF; 16]), BlankCheckResult::Blank);
}

#[test]
fn parity_check_blank_first_byte_zero() {
    assert_eq!(
        check_blank(&[0x00, 0xFF, 0xFF]),
        BlankCheckResult::NotBlank {
            first_non_blank_offset: 0,
            found_byte: 0x00
        }
    );
}

#[test]
fn parity_check_blank_mid_byte_zero() {
    assert_eq!(
        check_blank(&[0xFF, 0xFF, 0x00, 0xFF]),
        BlankCheckResult::NotBlank {
            first_non_blank_offset: 2,
            found_byte: 0x00
        }
    );
}

#[test]
fn parity_check_blank_last_byte_zero() {
    assert_eq!(
        check_blank(&[0xFF, 0xFF, 0x00]),
        BlankCheckResult::NotBlank {
            first_non_blank_offset: 2,
            found_byte: 0x00
        }
    );
}

#[test]
fn parity_check_blank_all_zero() {
    assert_eq!(
        check_blank(&[0x00; 8]),
        BlankCheckResult::NotBlank {
            first_non_blank_offset: 0,
            found_byte: 0x00
        }
    );
}

#[test]
fn parity_check_blank_ff_then_fe() {
    assert_eq!(
        check_blank(&[0xFF, 0xFE]),
        BlankCheckResult::NotBlank {
            first_non_blank_offset: 1,
            found_byte: 0xFE
        }
    );
}

// ---------------------------------------------------------------------------
// verify_buffer — reference cases
// ---------------------------------------------------------------------------

#[test]
fn parity_verify_buffer_identical() {
    assert_eq!(
        verify_buffer(&[0x12, 0x34, 0x56, 0x78], &[0x12, 0x34, 0x56, 0x78], 10),
        Ok(())
    );
}

#[test]
fn parity_verify_buffer_one_diff_at_2() {
    assert_eq!(
        verify_buffer(&[0x12, 0x34, 0x56, 0x78], &[0x12, 0x34, 0x00, 0x78], 10),
        Err(vec![diff(2, 0x56, 0x00)])
    );
}

#[test]
fn parity_verify_buffer_empty_both() {
    assert_eq!(verify_buffer(&[], &[], 10), Ok(()));
}

#[test]
fn parity_verify_buffer_empty_local_nonempty_chip() {
    // Synthetic difference at cmp_len = 0; missing local side reads as 0x00.
    assert_eq!(
        verify_buffer(&[], &[0x01, 0x02], 10),
        Err(vec![diff(0, 0x00, 0x01)])
    );
}

#[test]
fn parity_verify_buffer_nonempty_local_empty_chip() {
    assert_eq!(
        verify_buffer(&[0x01, 0x02], &[], 10),
        Err(vec![diff(0, 0x01, 0x00)])
    );
}

#[test]
fn parity_verify_buffer_local_longer_equal_prefix() {
    // The compared prefix matches, so the length mismatch produces a synthetic
    // difference at offset 2 with the chip side defaulting to 0x00.
    assert_eq!(
        verify_buffer(&[0x01, 0x02, 0x03], &[0x01, 0x02], 10),
        Err(vec![diff(2, 0x03, 0x00)])
    );
}

#[test]
fn parity_verify_buffer_chip_longer_equal_prefix() {
    assert_eq!(
        verify_buffer(&[0x01, 0x02], &[0x01, 0x02, 0x03], 10),
        Err(vec![diff(2, 0x00, 0x03)])
    );
}

#[test]
fn parity_verify_buffer_local_longer_with_diff() {
    // A real byte difference inside the compared range takes precedence: the
    // synthetic length-mismatch entry is NOT added.
    assert_eq!(
        verify_buffer(&[0x01, 0x09, 0x03], &[0x01, 0x02], 10),
        Err(vec![diff(1, 0x09, 0x02)])
    );
}

#[test]
fn parity_verify_buffer_max_diff_zero_still_records_one() {
    // Source quirk: the limit check runs after the push, so max = 0 yields one
    // recorded difference. Preserved deliberately.
    assert_eq!(
        verify_buffer(&[0x01, 0x02], &[0x03, 0x04], 0),
        Err(vec![diff(0, 0x01, 0x03)])
    );
}

#[test]
fn parity_verify_buffer_max_diff_one() {
    assert_eq!(
        verify_buffer(&[0x01, 0x02, 0x03], &[0x09, 0x08, 0x07], 1),
        Err(vec![diff(0, 0x01, 0x09)])
    );
}

#[test]
fn parity_verify_buffer_max_diff_exact_two() {
    assert_eq!(
        verify_buffer(&[0x01, 0x02, 0x03], &[0x09, 0x08, 0x07], 2),
        Err(vec![diff(0, 0x01, 0x09), diff(1, 0x02, 0x08)])
    );
}

#[test]
fn parity_verify_buffer_max_diff_large_all_differ() {
    assert_eq!(
        verify_buffer(&[0x00; 5], &[0xFF; 5], 100),
        Err(vec![
            diff(0, 0x00, 0xFF),
            diff(1, 0x00, 0xFF),
            diff(2, 0x00, 0xFF),
            diff(3, 0x00, 0xFF),
            diff(4, 0x00, 0xFF),
        ])
    );
}

#[test]
fn parity_verify_buffer_all_ff_vs_all_zero() {
    assert_eq!(
        verify_buffer(&[0xFF], &[0x00], 10),
        Err(vec![diff(0, 0xFF, 0x00)])
    );
}

#[test]
fn parity_verify_buffer_boundary_255() {
    assert_eq!(
        verify_buffer(&[0x00, 0xFF], &[0xFF, 0x00], 10),
        Err(vec![diff(0, 0x00, 0xFF), diff(1, 0xFF, 0x00)])
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn verify_buffer_is_deterministic_across_repeated_calls() {
    let local: Vec<u8> = (0..=255u8).collect();
    let chip: Vec<u8> = (0..=255u8).rev().collect();
    let first = verify_buffer(&local, &chip, 32);
    let second = verify_buffer(&local, &chip, 32);
    assert_eq!(first, second);
    assert!(first.is_err());
    assert_eq!(first.unwrap_err().len(), 32);
}
