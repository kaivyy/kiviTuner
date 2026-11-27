//! Parity tests for the specialised flasher cipher and the security bytecode VM.
//!
//! Ported from `ecu_engine_rust::flashing::{specialized_flasher,
//! bytecode_security_vm}`.
//!
//! ## How expected values were obtained
//!
//! These are OEM key schedules and a bytecode interpreter, so magic vectors would
//! only prove the tables match themselves. Instead the tests assert the algebraic
//! properties the constructs must have:
//!
//! * `inverse_round(round(x, k), k) == x` for every key — a Feistel round must be
//!   invertible;
//! * the key solver is deterministic and seed-sensitive;
//! * the kernel cipher round-trips;
//! * the bytecode VM rejects malformed scripts instead of panicking.
//!
//! ## Security note
//!
//! These routines implement OEM security access on hardware the project owner
//! controls. They are offline: no device is opened.

use kivi_flashing::bytecode_security_vm::{GmSeedKeyBytecodeVm, VagSa2BytecodeVm};
use kivi_flashing::specialized_flasher::FeistelCanKernelSecurity;

// ---------------------------------------------------------------------------
// Feistel round invertibility
// ---------------------------------------------------------------------------

#[test]
fn a_feistel_round_is_inverted_by_inverse_round() {
    // This is the defining property of a Feistel network: every round is a
    // bijection whose inverse is computable with the same key.
    for key in [0x0000u16, 0x1234, 0xFFFF, 0xA5A5] {
        for state in [0x0000_0000u32, 0x0000_FFFF, 0xFFFF_0000, 0xDEAD_BEEF] {
            let fwd = FeistelCanKernelSecurity::round(state, key);
            let back = FeistelCanKernelSecurity::inverse_round(fwd, key);
            assert_eq!(back, state, "round/inverse failed for state {state:#010X} key {key:#06X}");
        }
    }
}

#[test]
fn a_feistel_round_changes_the_state_for_a_nonzero_key() {
    // A round that never changes the state would not be a cipher.
    let changed = (0..8u16).any(|k| FeistelCanKernelSecurity::round(0x1234_5678, k * 0x1111) != 0x1234_5678);
    assert!(changed, "some key must alter the state");
}

#[test]
fn the_round_function_is_deterministic() {
    assert_eq!(
        FeistelCanKernelSecurity::round(0x1122_3344, 0x5566),
        FeistelCanKernelSecurity::round(0x1122_3344, 0x5566)
    );
}

#[test]
fn round_key_is_deterministic_and_key_sensitive() {
    let a = FeistelCanKernelSecurity::round_key(0x1234, 0x5678);
    let b = FeistelCanKernelSecurity::round_key(0x1234, 0x5678);
    assert_eq!(a, b, "same inputs must give the same subkey output");
    let c = FeistelCanKernelSecurity::round_key(0x1234, 0x5679);
    assert_ne!(a, c, "a different key must change the output");
}

// ---------------------------------------------------------------------------
// Kernel payload cipher
// ---------------------------------------------------------------------------

#[test]
fn the_kernel_payload_cipher_round_trips() {
    let original: Vec<u8> = (0..64).map(|i| (i * 7 % 251) as u8).collect();
    let encrypted = FeistelCanKernelSecurity::encrypt_kernel_payload(&original);
    assert_ne!(encrypted, original, "encryption must change the bytes");
    let decrypted = FeistelCanKernelSecurity::decrypt_kernel_payload(&encrypted);
    assert_eq!(decrypted, original, "decryption must restore the payload");
}

#[test]
fn an_empty_kernel_payload_round_trips() {
    let encrypted = FeistelCanKernelSecurity::encrypt_kernel_payload(&[]);
    let decrypted = FeistelCanKernelSecurity::decrypt_kernel_payload(&encrypted);
    assert!(decrypted.is_empty());
}

#[test]
fn the_kernel_crc_is_deterministic_and_input_sensitive() {
    let a = FeistelCanKernelSecurity::calculate_kernel_crc(&[1, 2, 3, 4]);
    let b = FeistelCanKernelSecurity::calculate_kernel_crc(&[1, 2, 3, 4]);
    let c = FeistelCanKernelSecurity::calculate_kernel_crc(&[1, 2, 3, 5]);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

// ---------------------------------------------------------------------------
// Security key solver
// ---------------------------------------------------------------------------

#[test]
fn the_stock_security_key_is_deterministic() {
    let seed = [0x11u8, 0x22, 0x33, 0x44];
    assert_eq!(
        FeistelCanKernelSecurity::solve_stock_security_key(seed),
        FeistelCanKernelSecurity::solve_stock_security_key(seed)
    );
}

#[test]
fn different_seeds_produce_different_security_keys() {
    let a = FeistelCanKernelSecurity::solve_stock_security_key([0, 0, 0, 0]);
    let b = FeistelCanKernelSecurity::solve_stock_security_key([0, 0, 0, 1]);
    assert_ne!(a, b, "the seed must influence the key");
}

// ---------------------------------------------------------------------------
// Security bytecode VM
// ---------------------------------------------------------------------------

#[test]
fn an_empty_script_returns_the_seed_unchanged() {
    // SOURCE BEHAVIOUR, documented: with no bytecode to execute the VM performs
    // no transformation and returns the input seed verbatim. That is a silent
    // identity, not an error — a caller that forgets to load a script would get
    // a plausible-looking key that is simply the seed.
    //
    // Pinned so that any future change to reject an empty script is a deliberate,
    // visible decision.
    let vm = VagSa2BytecodeVm::new(vec![]);
    match vm.execute(0x1234) {
        Ok(k) => assert_eq!(k, 0x1234, "an empty script must be the identity"),
        Err(_) => panic!("the source returns Ok(seed) for an empty script"),
    }
}

#[test]
fn executing_a_script_is_deterministic() {
    // A short, benign script: the VM must at least be a pure function of
    // (script, seed). Whatever it computes, it must compute the same twice.
    let script = vec![0x01u8, 0x02, 0x03, 0x04];
    let vm = VagSa2BytecodeVm::new(script);
    match (vm.execute(0xABCD), vm.execute(0xABCD)) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "the VM must be deterministic"),
        (Err(_), Err(_)) => {} // a malformed script may be rejected consistently
        _ => panic!("the VM must be consistent for the same input"),
    }
}

#[test]
fn execution_does_not_panic_on_arbitrary_scripts() {
    for len in [0usize, 1, 2, 4, 8, 64] {
        let script: Vec<u8> = (0..len).map(|i| (i * 53 % 256) as u8).collect();
        let vm = VagSa2BytecodeVm::new(script);
        let _ = vm.execute(0xDEAD_BEEF);
    }
}

#[test]
fn extracting_sa2_bytecode_from_a_buffer_without_the_signature_returns_none() {
    // A uniform buffer cannot contain the SA2 marker; the extractor must say so
    // rather than returning a bogus slice.
    let binary = vec![0x00u8; 4096];
    assert!(
        VagSa2BytecodeVm::extract_sa2_bytecode_from_cboot(&binary).is_none(),
        "a buffer without the signature must yield None"
    );
}

#[test]
fn the_u16_key_computation_is_deterministic() {
    let script = vec![0x10u8, 0x20, 0x30, 0x40];
    match (
        GmSeedKeyBytecodeVm::compute_key_u16(0x1234, &script),
        GmSeedKeyBytecodeVm::compute_key_u16(0x1234, &script),
    ) {
        (Ok(a), Ok(b)) => assert_eq!(a, b),
        (Err(_), Err(_)) => {}
        _ => panic!("key computation must be consistent"),
    }
}

#[test]
fn vag_sa2_vm_matches_canonical_sa2_engine_and_cboot_extractor() {
    use kivi_flashing::sa2_seed_key::compute_sa2_key;

    let script = vec![
        0x68, 0x05, // FOR 5
        0x81,       // RSL
        0x4A, 0x05, // BCC +5
        0x84, 0x01, 0x02, 0x03, 0x04, // SUB 0x01020304
        0x87, 0x11, 0x22, 0x33, 0x44, // EOR 0x11223344
        0x49, // NEXT
        0x4C, // FINISH
    ];
    let expected = compute_sa2_key(&script, 0xDEAD_BEEF).expect("canonical sa2");
    let vm = VagSa2BytecodeVm::new(script.clone());
    assert_eq!(vm.execute(0xDEAD_BEEF).unwrap(), expected);

    let mut cboot = vec![0u8; 64];
    cboot[12..12 + script.len()].copy_from_slice(&script);
    let (off, extracted) =
        VagSa2BytecodeVm::extract_sa2_bytecode_from_cboot(&cboot).expect("extract canonical SA2");
    assert_eq!(off, 12);
    assert_eq!(extracted, script);
}

#[test]
fn gm_bytecode_vm_matches_gmlan_table_and_registry_enforces_seed_widths() {
    use kivi_flashing::gm_seed_key_tables::{gm_can_key, gmlan_table::GM_GMLAN_KEY_TABLE};
    use kivi_flashing::seed_key_registry::compute_oem_seed_key;

    for algo in 1u16..16 {
        let prog = &GM_GMLAN_KEY_TABLE[algo as usize][..12];
        for seed in [0x0000u16, 0x1234, 0xABCD] {
            assert_eq!(
                GmSeedKeyBytecodeVm::compute_key_u16(seed, prog).unwrap(),
                gm_can_key(algo, seed),
                "mismatch for algo {algo} seed {seed:#06X}"
            );
        }
    }

    let subaru = compute_oem_seed_key("subaru_denso_sh705x", "11223344", 0, None, None)
        .expect("subaru sh705x solver");
    assert_eq!(
        subaru.key_bytes,
        FeistelCanKernelSecurity::solve_stock_security_key([0x11, 0x22, 0x33, 0x44]).to_vec()
    );

    for alg in ["gm_vpw", "gm_gmlan", "gm_other", "honda_shindengen_rom"] {
        assert!(
            compute_oem_seed_key(alg, "11223344", 1, None, Some(&[0u8; 64])).is_err(),
            "{alg} must reject 4-byte seeds"
        );
    }
}
