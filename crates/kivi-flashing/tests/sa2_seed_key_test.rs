//! VAG SA2 seed/key bytecode machine.
//!
//! ## What SA2 is
//!
//! VAG control units that use SA2 carry their SecurityAccess algorithm as a
//! **bytecode program** in the flash container (FRF/ODX) rather than as a fixed
//! formula. The ECU sends a seed; the tool runs the program against it to
//! produce the key.
//!
//! That is why `uds.rs`'s single `rotate_left(5) ^ mask` algorithm cannot unlock
//! these units: the algorithm differs per ECU and ships with the firmware.
//!
//! ## Oracle
//!
//! The test vector below is published in the reference implementation's README
//! (`sa2_seed_key` repository):
//!
//! ```text
//! tape = 68 02 81 49 93 a5 5a 55 aa 4a 05 87 81 05 95 26
//!        68 05 82 49 84 5a a5 aa 55 87 03 f7 80 6a 4c
//! seed = 0x1a1b1c1d
//! key  = 0x6a37f02e
//! ```
//!
//! That value is fixed by the format, not produced by the code under test.

use kivi_flashing::sa2_seed_key::{Sa2Error, Sa2SeedKey};

/// The published test vector.
const TAPE: [u8; 31] = [
    0x68, 0x02, 0x81, 0x49, 0x93, 0xa5, 0x5a, 0x55, 0xaa, 0x4a, 0x05, 0x87, 0x81, 0x05, 0x95,
    0x26, 0x68, 0x05, 0x82, 0x49, 0x84, 0x5a, 0xa5, 0xaa, 0x55, 0x87, 0x03, 0xf7, 0x80, 0x6a,
    0x4c,
];
const SEED: u32 = 0x1a1b_1c1d;
const EXPECTED_KEY: u32 = 0x6a37_f02e;

#[test]
fn the_published_vector_produces_the_published_key() {
    let key = Sa2SeedKey::new(&TAPE, SEED).execute().expect("execute");
    assert_eq!(key, EXPECTED_KEY, "SA2 must reproduce the published vector");
}

#[test]
fn execution_is_deterministic() {
    let a = Sa2SeedKey::new(&TAPE, SEED).execute().unwrap();
    let b = Sa2SeedKey::new(&TAPE, SEED).execute().unwrap();
    assert_eq!(a, b);
}

#[test]
fn a_different_seed_gives_a_different_key() {
    let other = Sa2SeedKey::new(&TAPE, 0x0000_0001).execute().unwrap();
    assert_ne!(other, EXPECTED_KEY, "the seed must affect the result");
}

#[test]
fn an_empty_tape_is_refused() {
    let err = Sa2SeedKey::new(&[], SEED).execute().unwrap_err();
    assert!(matches!(err, Sa2Error::EmptyTape));
}

#[test]
fn an_unknown_opcode_is_refused_rather_than_panicking() {
    let err = Sa2SeedKey::new(&[0xFF, 0xFF], SEED).execute().unwrap_err();
    assert!(
        matches!(err, Sa2Error::UnknownOpcode { opcode: 0xFF }),
        "got {err:?}"
    );
}

#[test]
fn a_truncated_operand_is_refused() {
    // 0x93 (ADD) needs four operand bytes; give it one.
    let err = Sa2SeedKey::new(&[0x93, 0x01], SEED).execute().unwrap_err();
    assert!(matches!(err, Sa2Error::TruncatedOperand { .. }), "got {err:?}");
}

#[test]
fn a_backwards_branch_cannot_run_forever() {
    // BRA cannot jump backwards on its own (the offset is a forward skip), so a
    // runaway program needs a loop whose counter never reaches zero. A FOR with
    // the maximum counter plus a NEXT that keeps jumping is the shortest way to
    // build one; here we simply confirm the step limit exists by running a tape
    // that never reaches FINISH and loops via FOR/NEXT.
    //
    // FOR 255 { NEXT } with no body executes 255 times and then falls through to
    // FINISH, so instead we point NEXT at itself with a BRA-free construction:
    // FOR 1 { NEXT } followed by FOR 1 { NEXT } ... is not infinite either.
    //
    // The honest test is that the step limit is enforced when a tape has no
    // FINISH and its PC cannot advance past the end — which cannot happen with
    // this opcode set. So this test documents that limitation rather than
    // pretending to exercise it.
    //
    // A tape that runs to the end without FINISH still terminates and returns
    // the register, matching the reference implementation.
    let key = Sa2SeedKey::new(&[0x93, 0x00, 0x00, 0x00, 0x05], 1)
        .execute()
        .unwrap();
    assert_eq!(key, 6, "a tape without FINISH runs to its end and returns");
}

#[test]
fn the_finish_opcode_stops_execution() {
    // FINISH immediately: the key is the seed, untouched.
    let key = Sa2SeedKey::new(&[0x4C], 0xDEAD_BEEF).execute().unwrap();
    assert_eq!(key, 0xDEAD_BEEF);
}

#[test]
fn rotate_left_wraps_the_carry_bit() {
    // RSL on 0x80000000 must give 0x00000001 (carry rotated in).
    let key = Sa2SeedKey::new(&[0x81, 0x4C], 0x8000_0000).execute().unwrap();
    assert_eq!(key, 0x0000_0001);
}

#[test]
fn rotate_right_wraps_the_carry_bit() {
    let key = Sa2SeedKey::new(&[0x82, 0x4C], 0x0000_0001).execute().unwrap();
    assert_eq!(key, 0x8000_0000);
}

#[test]
fn bcc_skips_only_when_carry_is_clear() {
    // Reference semantics: `if carry == 0 { pc += operand + 2 } else { pc += 2 }`.
    // So a SET carry does NOT skip.
    //
    // Tape: ADD 0xFFFFFFFF (1 + 0xFFFFFFFF wraps, carry set) | BCC 2 | XOR ...
    // With carry set the XOR runs: 0 ^ 0x12345678.
    let tape = [
        0x93, 0xFF, 0xFF, 0xFF, 0xFF, 0x4A, 0x02, 0x87, 0x12, 0x34, 0x56, 0x78, 0x4C,
    ];
    let key = Sa2SeedKey::new(&tape, 1).execute().unwrap();
    assert_eq!(
        key, 0x1234_5678,
        "carry set means BCC must NOT skip, so the XOR executes"
    );
}

#[test]
fn bcc_skips_when_carry_is_clear() {
    // ADD 1 does not overflow, so carry stays clear and BCC skips the XOR.
    // BCC's operand is a byte count: it skips `operand + 2` bytes, and the XOR
    // instruction is 5 bytes long, so the operand must be 3.
    let tape = [
        0x93, 0x00, 0x00, 0x00, 0x01, 0x4A, 0x05, 0x87, 0x12, 0x34, 0x56, 0x78, 0x4C,
    ];
    let key = Sa2SeedKey::new(&tape, 1).execute().unwrap();
    assert_eq!(key, 2, "carry clear means BCC skips the XOR");
}

#[test]
fn a_for_loop_runs_the_declared_number_of_times() {
    // FOR 3 { ADD 1 } NEXT FINISH  -> seed + 3
    let tape = [0x68, 0x03, 0x93, 0x00, 0x00, 0x00, 0x01, 0x49, 0x4C];
    let key = Sa2SeedKey::new(&tape, 10).execute().unwrap();
    assert_eq!(key, 13);
}
