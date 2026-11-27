//! Parity tests for the ported masked-signature scanner.
//!
//! Ported from `ecu_engine_rust::recognition::pattern_search`
//! (`MaskedSignatureScanner`). Expected values are taken from the source
//! module's own assertions and from hand-computed cases, not from running the
//! new implementation.
//!
//! ## Documented deviations from the source
//!
//! The source exposes `replace_all(&mut [u8], ...)`, an in-place mutator. This
//! target crate is analysis-oriented and read-only by convention, so the
//! mutation is exposed as `replace_all_copy(&[u8], ...) -> (Vec<u8>, usize)`,
//! which returns a new buffer. The counting semantics are identical.

use kivi_scanner::pattern_search::{
    count_different_bytes, find_all, find_first, parse_pattern, replace_all_copy,
};

// ---------------------------------------------------------------------------
// parse_pattern
// ---------------------------------------------------------------------------

#[test]
fn parses_hex_bytes_with_wildcards() {
    let p = parse_pattern("AA ?? CC DD").expect("parse");
    assert_eq!(p.len(), 4);
    assert_eq!(p[0], Some(0xAA));
    assert_eq!(p[1], None);
    assert_eq!(p[2], Some(0xCC));
    assert_eq!(p[3], Some(0xDD));
}

#[test]
fn single_question_mark_is_also_a_wildcard() {
    let p = parse_pattern("AA ? CC").expect("parse");
    assert_eq!(p, vec![Some(0xAA), None, Some(0xCC)]);
}

#[test]
fn parses_lowercase_hex() {
    let p = parse_pattern("aa bb").expect("parse");
    assert_eq!(p, vec![Some(0xAA), Some(0xBB)]);
}

#[test]
fn rejects_an_empty_pattern() {
    assert_eq!(parse_pattern(""), Err("Pattern cannot be empty"));
    assert_eq!(parse_pattern("   "), Err("Pattern cannot be empty"));
}

#[test]
fn rejects_an_invalid_hex_token() {
    assert_eq!(
        parse_pattern("AA ZZ"),
        Err("Invalid hexadecimal byte in pattern")
    );
}

#[test]
fn rejects_a_token_longer_than_one_byte() {
    // "123" is valid hex but exceeds u8, so from_str_radix must reject it.
    assert_eq!(
        parse_pattern("123"),
        Err("Invalid hexadecimal byte in pattern")
    );
}

// ---------------------------------------------------------------------------
// find_all / find_first
// ---------------------------------------------------------------------------

#[test]
fn finds_every_match_including_overlaps_at_distinct_offsets() {
    // Source test fixture, reproduced verbatim.
    let pattern = parse_pattern("AA ?? CC DD").expect("parse");
    let buffer = [
        0x00, 0x11, 0xAA, 0x55, 0xCC, 0xDD, 0x99, 0xAA, 0xFF, 0xCC, 0xDD,
    ];
    assert_eq!(find_all(&buffer, &pattern), vec![2, 7]);
}

#[test]
fn find_first_returns_the_lowest_offset() {
    let pattern = parse_pattern("AA ?? CC DD").expect("parse");
    let buffer = [
        0x00, 0x11, 0xAA, 0x55, 0xCC, 0xDD, 0x99, 0xAA, 0xFF, 0xCC, 0xDD,
    ];
    assert_eq!(find_first(&buffer, &pattern), Some(2));
}

#[test]
fn returns_no_match_when_the_buffer_is_shorter_than_the_pattern() {
    let pattern = parse_pattern("AA BB CC").expect("parse");
    assert!(find_all(&[0xAA, 0xBB], &pattern).is_empty());
    assert_eq!(find_first(&[0xAA, 0xBB], &pattern), None);
}

#[test]
fn an_empty_pattern_never_matches() {
    // Guards the source's early return: `pattern.is_empty()` yields no matches
    // rather than matching at every offset.
    let empty: Vec<Option<u8>> = Vec::new();
    assert!(find_all(&[1, 2, 3], &empty).is_empty());
    assert_eq!(find_first(&[1, 2, 3], &empty), None);
}

#[test]
fn a_match_at_the_final_position_is_found() {
    let pattern = parse_pattern("CC DD").expect("parse");
    let buffer = [0x00, 0x00, 0xCC, 0xDD];
    assert_eq!(find_all(&buffer, &pattern), vec![2]);
    assert_eq!(find_first(&buffer, &pattern), Some(2));
}

#[test]
fn an_all_wildcard_pattern_matches_every_offset() {
    let pattern = parse_pattern("?? ??").expect("parse");
    let buffer = [1, 2, 3, 4];
    assert_eq!(find_all(&buffer, &pattern), vec![0, 1, 2]);
}

// ---------------------------------------------------------------------------
// replace_all_copy (documented deviation: pure, not in-place)
// ---------------------------------------------------------------------------

#[test]
fn replaces_all_matches_and_reports_the_count() {
    // Source test fixture, reproduced verbatim.
    let buffer = vec![0x10, 0xAA, 0x20, 0xBB, 0xAA, 0x99, 0xBB];
    let pattern = parse_pattern("AA ?? BB").expect("parse");
    let replacement = [0x00, 0x00, 0x00];

    let (out, count) = replace_all_copy(&buffer, &pattern, &replacement).expect("replace");
    assert_eq!(count, 2);
    assert_eq!(out, vec![0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn replace_leaves_the_input_buffer_untouched() {
    let buffer = vec![0xAA, 0x01, 0xBB];
    let original = buffer.clone();
    let pattern = parse_pattern("AA ?? BB").expect("parse");

    let _ = replace_all_copy(&buffer, &pattern, &[0x00, 0x00, 0x00]).expect("replace");
    assert_eq!(buffer, original, "the source buffer must not be mutated");
}

#[test]
fn replace_rejects_a_length_mismatch() {
    let buffer = vec![0xAA, 0xBB];
    let pattern = parse_pattern("AA BB").expect("parse");
    assert_eq!(
        replace_all_copy(&buffer, &pattern, &[0x00]),
        Err("Replacement length must match pattern length for in-place replacement")
    );
}

#[test]
fn replace_with_no_matches_returns_the_original_bytes() {
    let buffer = vec![0x01, 0x02, 0x03];
    let pattern = parse_pattern("AA BB").expect("parse");
    let (out, count) = replace_all_copy(&buffer, &pattern, &[0x00, 0x00]).expect("replace");
    assert_eq!(count, 0);
    assert_eq!(out, buffer);
}

// ---------------------------------------------------------------------------
// count_different_bytes
// ---------------------------------------------------------------------------

#[test]
fn counts_differing_bytes_in_the_common_prefix() {
    let a = [1, 2, 3, 4, 5];
    let b = [1, 9, 3, 8, 5];
    assert_eq!(count_different_bytes(&a, &b), 2);
}

#[test]
fn a_length_difference_counts_as_differing_bytes() {
    // Source uses `abs_diff`, so a trailing length mismatch contributes.
    assert_eq!(count_different_bytes(&[1, 2, 3], &[1, 2]), 1);
    assert_eq!(count_different_bytes(&[1, 2], &[1, 2, 3, 4]), 2);
}

#[test]
fn identical_buffers_have_zero_differences() {
    assert_eq!(count_different_bytes(&[7, 8, 9], &[7, 8, 9]), 0);
}

#[test]
fn empty_buffers_have_zero_differences() {
    assert_eq!(count_different_bytes(&[], &[]), 0);
}

#[test]
fn length_difference_plus_content_difference_are_additive() {
    assert_eq!(count_different_bytes(&[1, 9, 3], &[1, 2]), 1 + 1);
}
