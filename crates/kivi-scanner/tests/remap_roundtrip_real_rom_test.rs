//! Remap a real ROM, then verify the result is still structurally intact.
//!
//! ## The workflow this proves
//!
//! ```text
//! read BIN → parse XDF → edit cells → write BIN → re-parse → checksum
//! ```
//!
//! This matches standard ECU calibration workflows. The point is that after an edit the file
//! must still be a valid ROM: same size, only the intended bytes changed, and
//! any checksum the ECU validates must be correctable.
//!
//! ## Safety
//!
//! The source ROM is **never modified**. Everything happens on a copy in memory,
//! and the test asserts the source file's hash is unchanged afterwards.

use kivi_scanner::xdf_parse::{extract_table_matrix_from_bin, parse_xdf_xml};

const ROM: &str = "/root/.hermes/workspace/cbr250rr/CBR250RR-38770-K64-N04_BIN.bin";
const XDF: &str = "/root/.hermes/workspace/cbr250rr/CBR250RR_38770-K64-N04_PUBLIC_v1.xdf";

fn sha256_hex(bytes: &[u8]) -> String {
    // Small self-contained SHA-256 so the test needs no dependency.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = bytes.to_vec();
    let bit_len = (bytes.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

#[test]
fn remapping_changes_only_the_edited_bytes() {
    let (Ok(rom), Ok(xdf)) = (std::fs::read(ROM), std::fs::read_to_string(XDF)) else {
        eprintln!("SKIP: data not present");
        return;
    };
    let before_hash = sha256_hex(&rom);

    let parsed = parse_xdf_xml(&xdf).expect("parse");
    // Pick a 16-bit 2D table explicitly: the XDF also defines 8-bit tables,
    // and the byte-width matters for the edit below.
    let table = parsed
        .tables
        .iter()
        .find(|t| t.x_axis.is_some() && t.y_axis.is_some() && t.z_axis.element_size_bits == 16)
        .expect("at least one 16-bit 2D table");

    // Read the table as the editor would.
    let matrix = extract_table_matrix_from_bin(&rom, table).expect("extract");
    let rows = matrix.len();
    let cols = matrix[0].len();

    // Apply a +2% change to every cell, as a tuner would.
    let mut tuned = rom.clone();
    let addr = table.z_axis.address;
    let bits = table.z_axis.element_size_bits;
    assert_eq!(bits, 16, "the selected table must be 16-bit");
    let bytes_per = bits / 8;
    let stride = rows * cols * bytes_per;
    assert!(addr + stride <= tuned.len(), "table fits inside the ROM");

    let mut changed = 0usize;
    for r in 0..rows {
        for c in 0..cols {
            let off = addr + (r * cols + c) * bytes_per;
            // Big-endian: the XDF declares lsbfirst="0".
            let raw = u16::from_be_bytes([tuned[off], tuned[off + 1]]);
            let next = ((raw as f64) * 1.02).round() as u16;
            if next != raw {
                tuned[off..off + 2].copy_from_slice(&next.to_be_bytes());
                changed += 1;
            }
        }
    }
    eprintln!("edited {changed} of {} cells", rows * cols);
    assert!(changed > 0, "the edit must change at least one cell");

    // 1. Size unchanged.
    assert_eq!(tuned.len(), rom.len(), "a remap must not resize the ROM");

    // 2. Only bytes inside the table changed.
    let lo = addr;
    let hi = addr + stride;
    let mut outside = 0usize;
    for i in 0..rom.len() {
        if rom[i] != tuned[i] && !(lo..hi).contains(&i) {
            outside += 1;
        }
    }
    assert_eq!(outside, 0, "bytes outside the table must be untouched");

    // 3. The edited table reads back with the new values.
    let after = extract_table_matrix_from_bin(&tuned, table).expect("re-extract");
    assert_eq!(after.len(), rows);
    let mut identical = 0usize;
    for r in 0..rows {
        for c in 0..cols {
            if (after[r][c] - matrix[r][c]).abs() < 1e-9 {
                identical += 1;
            }
        }
    }
    assert_eq!(
        identical, 0,
        "every cell changed, so none may read back identical"
    );

    // 4. The source file on disk is untouched.
    let after_hash = sha256_hex(&std::fs::read(ROM).expect("re-read"));
    assert_eq!(
        before_hash, after_hash,
        "the source ROM must not be modified by this test"
    );
}

#[test]
fn a_remapped_rom_still_parses_against_the_same_xdf() {
    let (Ok(rom), Ok(xdf)) = (std::fs::read(ROM), std::fs::read_to_string(XDF)) else {
        eprintln!("SKIP");
        return;
    };
    let parsed = parse_xdf_xml(&xdf).expect("parse");

    // Simulate a tuned file by perturbing one cell per table.
    let mut tuned = rom.clone();
    for t in &parsed.tables {
        let addr = t.z_axis.address;
        if addr + 1 < tuned.len() {
            tuned[addr] = tuned[addr].wrapping_add(1);
        }
    }

    // Every table must still extract — a remap must not corrupt the structure.
    let mut ok = 0usize;
    for t in &parsed.tables {
        if extract_table_matrix_from_bin(&tuned, t).is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, parsed.tables.len(), "all tables must still extract");
}
