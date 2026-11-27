//! Behavioral parity tests for the ported checksum primitives against the
//! reference implementations in `ecu_engine_rust`.
//!
//! **Every expected value below was produced by executing the unmodified
//! source** through a disposable oracle harness (`/tmp/parity-slice3`), not
//! derived by hand and not copied from textbook vectors.
//!
//! Source locations:
//! * `crc32_ieee`, `crc32_castagnoli`, `crc32_unreflected`, `crc32_simos`
//!   — `src/math/checksum.rs` (`ChecksumEngine`)
//! * `crc16_ccitt` — `src/formats/mod.rs` (`VbfContainerCodec`)
//!
//! ## Not ported: `crc16_modbus`
//!
//! The source declares the enum variant `ChecksumAlgorithm::Crc16Modbus` but
//! **never implements it**: `calculate_arithmetic_checksum` returns
//! `Err("Unsupported arithmetic checksum algorithm")` for every CRC variant.
//! There is no source behavior to preserve, so porting it would mean inventing
//! an algorithm rather than porting one. See the final report.

use kivi_checksum::primitives::{
    compute_crc32, crc16_ccitt, crc32_castagnoli, crc32_simos, crc32_unreflected,
};

/// Inputs used by the oracle, reproduced exactly so the fixtures apply.
fn input(name: &str) -> Vec<u8> {
    match name {
        "empty" => vec![],
        "one_byte_00" => vec![0x00],
        "one_byte_ff" => vec![0xFF],
        "one_byte_01" => vec![0x01],
        "zero_4" => vec![0x00; 4],
        "ff_4" => vec![0xFF; 4],
        "ascii_123456789" => b"123456789".to_vec(),
        "ascii_abc" => b"abc".to_vec(),
        "zero_and_highbit" => vec![0x00, 0x80, 0xFF, 0x7F],
        "len_1" => vec![0x42],
        "len_2" => vec![0x42, 0x43],
        "len_3" => vec![0x42, 0x43, 0x44],
        "len_7" => vec![1, 2, 3, 4, 5, 6, 7],
        "len_8" => vec![1, 2, 3, 4, 5, 6, 7, 8],
        "len_15" => (0u8..15).collect(),
        "len_16" => (0u8..16).collect(),
        "len_31" => (0u8..31).collect(),
        "len_32" => (0u8..32).collect(),
        "len_255" => (0u8..=254).collect(),
        "len_256" => (0u8..=255).collect(),
        "len_1024" => (0..1024u32).map(|i| (i % 256) as u8).collect(),
        "all_ff_256" => vec![0xFF; 256],
        other => panic!("unknown oracle input: {}", other),
    }
}

// ---------------------------------------------------------------------------
// crc32_ieee — kiviTuner already provides this as `compute_crc32` (crc32fast).
// These cases prove the existing implementation matches the source exactly,
// so no new function is introduced for it.
// ---------------------------------------------------------------------------

#[test]
fn parity_crc32_ieee_existing_impl_matches_source() {
    let expected: [(&str, u32); 12] = [
        ("empty", 0x0000_0000),
        ("one_byte_00", 0xD202_EF8D),
        ("one_byte_ff", 0xFF00_0000),
        ("one_byte_01", 0xA505_DF1B),
        ("zero_4", 0x2144_DF1C),
        ("ff_4", 0xFFFF_FFFF),
        ("ascii_123456789", 0xCBF4_3926),
        ("ascii_abc", 0x3524_41C2),
        ("zero_and_highbit", 0x93EF_5543),
        ("len_256", 0x2905_8C73),
        ("len_1024", 0xB70B_4C26),
        ("all_ff_256", 0xFEA8_A821),
    ];
    for (name, want) in expected {
        assert_eq!(compute_crc32(&input(name)), want, "crc32_ieee {}", name);
    }
}

// ---------------------------------------------------------------------------
// crc32_castagnoli (CRC-32C, reflected, poly 0x82F63B78, init 0xFFFFFFFF, xor 0xFFFFFFFF)
// ---------------------------------------------------------------------------

#[test]
fn parity_crc32_castagnoli_full_oracle_sweep() {
    let expected: [(&str, u32); 22] = [
        ("empty", 0x0000_0000),
        ("one_byte_00", 0x527D_5351),
        ("one_byte_ff", 0xFF00_0000),
        ("one_byte_01", 0xA016_D052),
        ("zero_4", 0x4867_4BC7),
        ("ff_4", 0xFFFF_FFFF),
        ("ascii_123456789", 0xE306_9283),
        ("ascii_abc", 0x364B_3FB7),
        ("zero_and_highbit", 0x4F28_702E),
        ("len_1", 0xF23D_3E1A),
        ("len_2", 0x7B18_6F70),
        ("len_3", 0xA57F_9590),
        ("len_7", 0xBD3A_64DC),
        ("len_8", 0x4689_1F81),
        ("len_15", 0x68EF_03F6),
        ("len_16", 0xD9C9_08EB),
        ("len_31", 0xE95C_ABCB),
        ("len_32", 0x46DD_794E),
        ("len_255", 0x8953_C482),
        ("len_256", 0x9C44_184B),
        ("len_1024", 0x2CDF_6E8F),
        ("all_ff_256", 0x0F46_4A5C),
    ];
    for (name, want) in expected {
        assert_eq!(
            crc32_castagnoli(&input(name)),
            want,
            "crc32_castagnoli {}",
            name
        );
    }
}

#[test]
fn crc32_castagnoli_differs_from_ieee() {
    // Guards against aliasing Castagnoli to the IEEE polynomial.
    let data = b"123456789";
    assert_ne!(crc32_castagnoli(data), compute_crc32(data));
    assert_eq!(crc32_castagnoli(data), 0xE306_9283);
    assert_eq!(compute_crc32(data), 0xCBF4_3926);
}

// ---------------------------------------------------------------------------
// crc32_unreflected (bit-forward; poly/init/xor_out are caller-supplied)
// ---------------------------------------------------------------------------

#[test]
fn parity_crc32_unreflected_simos_params_full_sweep() {
    // poly 0x04C11DB7, init 0x00000000, xor_out 0x00000000
    let expected: [(&str, u32); 22] = [
        ("empty", 0x0000_0000),
        ("one_byte_00", 0x0000_0000),
        ("one_byte_ff", 0xB1F7_40B4),
        ("one_byte_01", 0x04C1_1DB7),
        ("zero_4", 0x0000_0000),
        ("ff_4", 0xC704_DD7B),
        ("ascii_123456789", 0x89A1_897F),
        ("ascii_abc", 0x2C17_398C),
        ("zero_and_highbit", 0x7A12_C713),
        ("len_1", 0x3D04_4B19),
        ("len_2", 0xD871_A4ED),
        ("len_3", 0x61B5_4DFA),
        ("len_7", 0x23CE_674B),
        ("len_8", 0x7D0F_3681),
        ("len_15", 0x8A82_9E56),
        ("len_16", 0xFC57_DD85),
        ("len_31", 0x9CA1_9BF0),
        ("len_32", 0xC5D4_3637),
        ("len_255", 0x991A_370C),
        ("len_256", 0xAC14_8725),
        ("len_1024", 0x9156_6C1B),
        ("all_ff_256", 0x9EB4_7CCF),
    ];
    for (name, want) in expected {
        assert_eq!(
            crc32_unreflected(&input(name), 0x04C1_1DB7, 0, 0),
            want,
            "crc32_unreflected(simos params) {}",
            name
        );
    }
}

#[test]
fn parity_crc32_unreflected_custom_parameters() {
    // Proves the function honours poly/init/xor_out rather than hardcoding them.
    let d = input("ascii_123456789");
    assert_eq!(
        crc32_unreflected(&[], 0x04C1_1DB7, 0xFFFF_FFFF, 0xFFFF_FFFF),
        0x0000_0000
    );
    assert_eq!(
        crc32_unreflected(&d, 0x04C1_1DB7, 0xFFFF_FFFF, 0xFFFF_FFFF),
        0xFC89_1918
    );
    assert_eq!(crc32_unreflected(&d, 0x0000_0000, 0, 0), 0x0000_0000);
    assert_eq!(
        crc32_unreflected(&[], 0x04C1_1DB7, 0x1234_5678, 0),
        0x1234_5678
    );
    assert_eq!(
        crc32_unreflected(&d, 0x04C1_1DB7, 0x1234_5678, 0),
        0xEBC4_18C4
    );
    assert_eq!(
        crc32_unreflected(&[], 0x04C1_1DB7, 0, 0xDEAD_BEEF),
        0xDEAD_BEEF
    );
    assert_eq!(
        crc32_unreflected(&d, 0x04C1_1DB7, 0, 0xDEAD_BEEF),
        0x570C_3790
    );
}

#[test]
fn parity_crc32_simos_is_unreflected_with_default_params() {
    assert_eq!(crc32_simos(b"123456789"), 0x89A1_897F);
    for name in ["empty", "one_byte_00", "ff_4", "len_1024", "all_ff_256"] {
        let d = input(name);
        assert_eq!(
            crc32_simos(&d),
            crc32_unreflected(&d, 0x04C1_1DB7, 0, 0),
            "simos vs unreflected {}",
            name
        );
    }
}

#[test]
fn crc32_unreflected_differs_from_reflected() {
    // Unreflected and reflected CRC-32 must not be interchangeable.
    let data = b"123456789";
    assert_ne!(
        crc32_unreflected(data, 0x04C1_1DB7, 0, 0),
        compute_crc32(data)
    );
}

// ---------------------------------------------------------------------------
// crc16_ccitt (poly 0x1021, init 0xFFFF, no reflection, no final xor)
// ---------------------------------------------------------------------------

#[test]
fn parity_crc16_ccitt_full_oracle_sweep() {
    let expected: [(&str, u16); 22] = [
        ("empty", 0xFFFF),
        ("one_byte_00", 0xE1F0),
        ("one_byte_ff", 0xFF00),
        ("one_byte_01", 0xF1D1),
        ("zero_4", 0x84C0),
        ("ff_4", 0x1D0F),
        ("ascii_123456789", 0x29B1),
        ("ascii_abc", 0x514A),
        ("zero_and_highbit", 0x331D),
        ("len_1", 0x8976),
        ("len_2", 0x0E06),
        ("len_3", 0xEF8E),
        ("len_7", 0xD77D),
        ("len_8", 0x4792),
        ("len_15", 0xD551),
        ("len_16", 0x3B37),
        ("len_31", 0xC109),
        ("len_32", 0x23B3),
        ("len_255", 0xE3EC),
        ("len_256", 0x3FBD),
        ("len_1024", 0x758F),
        ("all_ff_256", 0x5B2F),
    ];
    for (name, want) in expected {
        assert_eq!(crc16_ccitt(&input(name)), want, "crc16_ccitt {}", name);
    }
}

#[test]
fn crc16_ccitt_is_ccitt_false_not_xmodem() {
    // XMODEM uses init 0x0000; CCITT-FALSE uses 0xFFFF. An empty buffer
    // distinguishes them: 0xFFFF here, 0x0000 for XMODEM.
    assert_eq!(crc16_ccitt(&[]), 0xFFFF);
    // Standard check value for "123456789" under CRC-16/CCITT-FALSE.
    assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn all_primitives_are_deterministic() {
    for name in ["len_256", "len_1024", "all_ff_256"] {
        let data = input(name);
        assert_eq!(crc32_castagnoli(&data), crc32_castagnoli(&data));
        assert_eq!(crc32_simos(&data), crc32_simos(&data));
        assert_eq!(crc16_ccitt(&data), crc16_ccitt(&data));
        assert_eq!(
            crc32_unreflected(&data, 0x04C1_1DB7, 0, 0),
            crc32_unreflected(&data, 0x04C1_1DB7, 0, 0)
        );
    }
}
