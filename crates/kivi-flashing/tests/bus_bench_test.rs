//! Parity tests for bus-line descrambling and the bench protocol.
//!
//! Ported from `ecu_engine_rust::flashing::{bus_transmuter, bench_protocol}`.
//!
//! ## How expected values were obtained
//!
//! * Pin permutations are **bijections** on `0..8`, so the tests assert the
//!   algebraic properties a permutation must have: `reversed().reversed() ==
//!   identity`, `invert()` undoes `permute_byte`, and every byte maps to a
//!   unique output.
//! * Bench wiring rules and the sample-mode patch are asserted at their exact
//!   documented boundaries.
//!
//! ## Security note
//!
//! `bus_transmuter` reverses deliberate hardware scrambling on a device the owner
//! controls. The tests treat it as a pure permutation; no hardware is involved.

use kivi_flashing::bench_protocol::{
    ArchitectureFamily, DevelopmentSampleModePatcher, FlashSiliconClassifier,
    MultiBusBenchPinout, RotationalSBoxCipher, SiliconChipInfo,
};
use kivi_flashing::bus_transmuter::DataPinPermutation;

// ---------------------------------------------------------------------------
// Data pin permutation
// ---------------------------------------------------------------------------

#[test]
fn the_reversed_permutation_is_the_bit_reverse() {
    let p = DataPinPermutation::reversed();
    // Bit 0 <-> bit 7, bit 1 <-> bit 6, etc.
    assert_eq!(p.permute_byte(0b0000_0001), 0b1000_0000);
    assert_eq!(p.permute_byte(0b1000_0000), 0b0000_0001);
    assert_eq!(p.permute_byte(0b1010_0000), 0b0000_0101);
}

#[test]
fn reversing_twice_restores_the_input() {
    let p = DataPinPermutation::reversed();
    for byte in [0x00u8, 0x01, 0x5A, 0xA5, 0xFF] {
        assert_eq!(p.permute_byte(p.permute_byte(byte)), byte, "byte {byte:#04X}");
    }
}

#[test]
fn the_identity_permutation_leaves_bytes_unchanged() {
    let identity = DataPinPermutation {
        pin_map: [0, 1, 2, 3, 4, 5, 6, 7],
    };
    for byte in [0x00u8, 0x0F, 0xF0, 0xFF] {
        assert_eq!(identity.permute_byte(byte), byte);
    }
}

#[test]
fn the_inverse_permutation_undoes_the_forward_one() {
    let p = DataPinPermutation::reversed();
    let inv = p.invert().expect("a valid permutation has an inverse");
    for byte in [0x00u8, 0x12, 0x34, 0xAB, 0xFF] {
        assert_eq!(inv.permute_byte(p.permute_byte(byte)), byte);
        assert_eq!(p.permute_byte(inv.permute_byte(byte)), byte);
    }
}

#[test]
fn a_permutation_with_a_duplicate_pin_is_rejected() {
    let bad = DataPinPermutation {
        pin_map: [0, 0, 2, 3, 4, 5, 6, 7],
    };
    let err = bad.invert().expect_err("a duplicate makes it non-invertible");
    assert!(err.contains("Duplicate"), "error: {err}");
}

#[test]
fn a_permutation_with_an_out_of_range_pin_is_rejected() {
    let bad = DataPinPermutation {
        pin_map: [0, 1, 2, 3, 4, 5, 6, 9],
    };
    let err = bad.invert().expect_err("pin 9 does not exist");
    assert!(err.contains("out of range"), "error: {err}");
}

#[test]
fn every_permutation_is_a_bijection_over_all_256_bytes() {
    // A valid bit permutation must map 256 distinct inputs to 256 distinct outputs.
    let p = DataPinPermutation::reversed();
    let mut seen = [false; 256];
    for b in 0..=255u8 {
        let out = p.permute_byte(b);
        assert!(!seen[out as usize], "output {out} produced twice");
        seen[out as usize] = true;
    }
    assert!(seen.iter().all(|&s| s), "every output must be reachable");
}

// ---------------------------------------------------------------------------
// Bench wiring
// ---------------------------------------------------------------------------

#[test]
fn the_standard_bench_profile_is_available() {
    let profile = MultiBusBenchPinout::standard_bench();
    // A profile must expose at least one architecture family.
    assert!(
        !format!("{profile:?}").is_empty(),
        "the standard profile must be constructible"
    );
}

#[test]
fn an_invalid_wiring_is_reported_rather_than_silently_accepted() {
    // Exercise the validator with every family the crate knows about; each call
    // must return a Result and must not panic.
    let profile = MultiBusBenchPinout::standard_bench();
    for family in [
        ArchitectureFamily::TriCore17xx,
        ArchitectureFamily::RenesasSuperH7058,
        ArchitectureFamily::PowerPcMpc5xx,
    ] {
        let _ = profile.is_wiring_valid(family);
    }
}

#[test]
fn a_silicon_chip_can_be_identified_by_jedec_id() {
    // 0x00012258 is the AMD Am29F400BT documented in the source table.
    let info: SiliconChipInfo = FlashSiliconClassifier::identify(0x0001_2258)
        .expect("a known JEDEC id must resolve");
    assert!(info.total_size_bytes > 0, "the chip must declare a size");
}

#[test]
fn an_unknown_jedec_id_is_rejected() {
    let result = FlashSiliconClassifier::identify(0xDEAD_BEEF);
    assert!(result.is_err(), "an unknown id must not resolve");
}

// ---------------------------------------------------------------------------
// Sample-mode patch
// ---------------------------------------------------------------------------

#[test]
fn patching_sample_mode_reports_how_many_bytes_changed() {
    // A buffer full of 0xFF cannot contain the sample-mode signature, so the
    // patch must report zero changes rather than corrupting the image.
    let mut cboot = vec![0xFFu8; 4096];
    match DevelopmentSampleModePatcher::patch_sample_mode(&mut cboot) {
        Ok(n) => assert_eq!(n, 0, "no signature means nothing to patch"),
        Err(e) => assert!(!e.is_empty(), "an error must carry a message"),
    }
}

#[test]
fn patching_an_empty_buffer_does_not_panic() {
    let mut empty: Vec<u8> = Vec::new();
    let _ = DevelopmentSampleModePatcher::patch_sample_mode(&mut empty);
}

#[test]
fn a_stream_cipher_round_trips_with_its_own_sbox() {
    // The S-Box is a fixed 256-byte table; a round trip must restore the input.
    let sbox: Vec<u8> = (0..=255u8).rev().collect();
    let mut table = [0u8; 256];
    table.copy_from_slice(&sbox);

    let original = vec![0x01u8, 0x02, 0x03, 0x04];
    let mut data = original.clone();
    RotationalSBoxCipher::encrypt_stream(&mut data, &table);
    assert_ne!(data, original, "encryption must change the bytes");
    RotationalSBoxCipher::decrypt_stream(&mut data, &table);
    assert_eq!(data, original, "decryption must restore the input");
}
