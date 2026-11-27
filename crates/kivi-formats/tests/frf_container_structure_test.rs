//! FRF is a rolling-XOR over a ZIP containing one ODX — verify that shape.
//!
//! ## What this pins
//!
//! The cipher in `transform_frf_stream` matches `bri3d/VW_Flash`
//! `frf/decryptfrf.py` byte for byte. What kiviTuner did **not** model is what
//! the plaintext is: a ZIP archive holding a single `.odx` XML document.
//!
//! Without that, a caller decrypts to bytes and has no way to tell whether the
//! key was right. The check is simple and decisive: correct key → `PK\x03\x04`.

use kivi_formats::container::SgoContainer as FrfContainerCipher;

/// The documented test key. Real VAG `frf.key` files are 4095 bytes; any
/// non-empty key exercises the cipher.
const KEY: &[u8] = b"VAG_TEST_KEY_MATERIAL_0123456789";

/// A minimal ZIP local file header, enough to identify the container.
fn zip_magic() -> [u8; 4] {
    [0x50, 0x4B, 0x03, 0x04]
}

#[test]
fn the_cipher_is_self_inverse() {
    // A rolling XOR is its own inverse, which is what makes one function serve
    // both directions. This is the property the packer relies on.
    let plain: Vec<u8> = (0..512u16).map(|i| (i % 251) as u8).collect();
    let enc = FrfContainerCipher::transform_frf_stream(&plain, KEY).expect("encrypt");
    let dec = FrfContainerCipher::transform_frf_stream(&enc, KEY).expect("decrypt");
    assert_eq!(dec, plain);
}

#[test]
fn an_empty_key_is_refused() {
    assert!(FrfContainerCipher::transform_frf_stream(b"data", b"").is_err());
}

#[test]
fn a_zip_magic_round_trips_through_the_cipher() {
    // Encrypt a payload that begins with the ZIP signature, then decrypt and
    // confirm the signature comes back. That is the exact check a loader uses
    // to decide whether the key was correct.
    let mut plain = zip_magic().to_vec();
    plain.extend_from_slice(b"PK\x01\x02rest of the archive");

    let enc = FrfContainerCipher::transform_frf_stream(&plain, KEY).expect("encrypt");
    assert_ne!(&enc[..4], &zip_magic(), "ciphertext must not start with PK");

    let dec = FrfContainerCipher::transform_frf_stream(&enc, KEY).expect("decrypt");
    assert_eq!(&dec[..4], &zip_magic(), "plaintext must start with PK");
}

#[test]
fn the_cipher_depends_on_the_key() {
    let plain = b"same plaintext".to_vec();
    let a = FrfContainerCipher::transform_frf_stream(&plain, b"KEY_A").unwrap();
    let b = FrfContainerCipher::transform_frf_stream(&plain, b"KEY_B").unwrap();
    assert_ne!(a, b);
}

#[test]
fn the_cipher_is_position_dependent() {
    // The same byte at two offsets must encrypt differently, because the seeds
    // advance. If it did not, the stream would be a repeating pad.
    let plain = vec![0x00u8; 64];
    let enc = FrfContainerCipher::transform_frf_stream(&plain, KEY).unwrap();
    let distinct: std::collections::HashSet<u8> = enc.iter().copied().collect();
    assert!(
        distinct.len() > 8,
        "keystream should vary across positions, got {} distinct bytes",
        distinct.len()
    );
}

#[test]
fn a_short_key_cycles() {
    // The key index wraps, so a 1-byte key is legal and must not panic.
    let enc = FrfContainerCipher::transform_frf_stream(&[0u8; 32], b"K").expect("encrypt");
    assert_eq!(enc.len(), 32);
}

// ---------------------------------------------------------------------------
// What the plaintext actually is
// ---------------------------------------------------------------------------

#[test]
fn a_decrypted_frf_is_identified_as_a_zip_by_its_magic() {
    // The loader's decisive check: a correct key produces `PK\x03\x04`. Wrong key
    // produces noise. This is how a caller knows the key was right before
    // attempting to parse anything.
    use kivi_formats::container::frf_plaintext_kind;

    let mut plain = zip_magic().to_vec();
    plain.extend_from_slice(&[0u8; 100]);
    let enc = FrfContainerCipher::transform_frf_stream(&plain, KEY).unwrap();
    let dec = FrfContainerCipher::transform_frf_stream(&enc, KEY).unwrap();

    assert_eq!(
        frf_plaintext_kind(&dec),
        kivi_formats::container::FrfPlaintext::Zip,
        "a correct key must reveal a ZIP"
    );
}

#[test]
fn a_wrong_key_is_reported_as_not_a_zip() {
    use kivi_formats::container::frf_plaintext_kind;

    let mut plain = zip_magic().to_vec();
    plain.extend_from_slice(&[0u8; 100]);
    let enc = FrfContainerCipher::transform_frf_stream(&plain, KEY).unwrap();
    // Decrypt with the wrong key.
    let wrong = FrfContainerCipher::transform_frf_stream(&enc, b"WRONG_KEY").unwrap();

    assert_ne!(
        frf_plaintext_kind(&wrong),
        kivi_formats::container::FrfPlaintext::Zip,
        "a wrong key must not be mistaken for a valid archive"
    );
}

#[test]
fn an_empty_buffer_is_reported_as_unknown() {
    use kivi_formats::container::{frf_plaintext_kind, FrfPlaintext};
    assert_eq!(frf_plaintext_kind(&[]), FrfPlaintext::Unknown);
}

#[test]
fn a_short_buffer_is_reported_as_unknown() {
    use kivi_formats::container::{frf_plaintext_kind, FrfPlaintext};
    assert_eq!(frf_plaintext_kind(&[0x50, 0x4B]), FrfPlaintext::Unknown);
}

#[test]
fn the_zip_magic_is_detected_at_the_start_only() {
    use kivi_formats::container::{frf_plaintext_kind, FrfPlaintext};
    // PK appearing mid-buffer is not a ZIP container.
    let mut buf = vec![0u8; 16];
    buf[8] = 0x50;
    buf[9] = 0x4B;
    buf[10] = 0x03;
    buf[11] = 0x04;
    assert_eq!(frf_plaintext_kind(&buf), FrfPlaintext::Unknown);
}
