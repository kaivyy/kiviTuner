//! Wave 3.3 P0: first controlled execution of the three 2-D scanners.
//!
//! Discovery only. No XDF/A2L/DAMOS is read anywhere in this binary — grep it.
//! Output is written before any reference is touched, so the freeze is real
//! rather than procedural.
//!
//! Run: `cargo run --release --example wave3_3_p0_harness -- <out.json>`

use std::collections::BTreeMap;
use std::time::Instant;

use kivi_scanner::axis_anchored::axis_anchored_scan;
use kivi_scanner::blind_automap::default_shapes;
use kivi_scanner::heuristic_scan::scan_rom_16bit;

const DB: &str = "/root/ecu_master_database";

const ROMS: &[(&str, &str)] = &[
    (
        "CBR250RR",
        "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
    ),
    (
        "BMW_B58",
        "02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/00005D55508C05_b58o1_original.bin",
    ),
    (
        "SUZUKI_5JCZSNC0",
        "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.bin",
    ),
];

fn sha256_hex(bytes: &[u8]) -> String {
    // sha2 is not a dependency of this crate; a local implementation keeps the
    // harness dependency-free as the task requires.
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
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "P0-RAW-OUTPUT.json".to_string());

    let mut report = String::new();
    report.push_str("{\n\"wave\":\"3.3\",\"phase\":\"P0\",\n");
    report.push_str("\"note\":\"discovery only; no reference definition was read\",\n");
    report.push_str("\"scanner_source_sha256\":{");
    for (i, f) in [
        "src/axis_anchored.rs",
        "src/heuristic_scan.rs",
        "src/blind_automap.rs",
        "src/scan_metrics.rs",
    ]
    .iter()
    .enumerate()
    {
        let p = format!("crates/kivi-scanner/{f}");
        let bytes = std::fs::read(&p).unwrap_or_default();
        if i > 0 {
            report.push(',');
        }
        report.push_str(&format!("\"{f}\":\"{}\"", sha256_hex(&bytes)));
    }
    report.push_str("},\n\"totals\":{");

    let mut per_rom = Vec::new();
    let mut b_total = 0usize;
    let mut c_total = 0usize;

    for (idx, (name, rel)) in ROMS.iter().enumerate() {
        let path = format!("{DB}/{rel}");
        let Ok(rom) = std::fs::read(&path) else {
            eprintln!("SKIP {name}: cannot read {path}");
            continue;
        };
        let rom_sha = sha256_hex(&rom);

        // ---- Scanner B: shape range, no per-pair shapes ----
        let t = Instant::now();
        let b = scan_rom_16bit(&rom, &[16, 12, 8], 6, 16, true);
        let b_ms = t.elapsed().as_secs_f64() * 1000.0;
        b_total += b.len();

        // ---- Scanner C: first execution ever ----
        let t = Instant::now();
        let c = axis_anchored_scan(&rom, true, 64);
        let c_ms = t.elapsed().as_secs_f64() * 1000.0;
        c_total += c.len();

        let mut b_by_shape: BTreeMap<String, usize> = BTreeMap::new();
        for m in &b {
            *b_by_shape
                .entry(format!("{}x{}", m.rows, m.cols))
                .or_default() += 1;
        }
        let mut c_by_shape: BTreeMap<String, usize> = BTreeMap::new();
        let mut c_axis_rows = 0usize;
        let mut c_axis_cols = 0usize;
        for t in &c {
            *c_by_shape
                .entry(format!("{}x{}", t.rows, t.cols))
                .or_default() += 1;
            // A derived row count is one that candidate_y_lens could reach from
            // the axis; the fallback list is [4,5,6,8,10,12,16].
            if [4usize, 5, 6, 8, 10, 12, 16].contains(&t.rows) {
                c_axis_rows += 1;
            }
            c_axis_cols += t.cols;
        }

        // Overlap between B and C by address span.
        let b_spans: Vec<(usize, usize)> = b
            .iter()
            .map(|m| (m.offset, m.offset + m.rows * m.cols * 2))
            .collect();
        let c_spans: Vec<(usize, usize)> = c
            .iter()
            .map(|t| {
                (
                    t.address,
                    t.address + t.rows * t.cols * t.cell_bytes,
                )
            })
            .collect();
        let overlap = c_spans
            .iter()
            .filter(|(cs, ce)| {
                b_spans
                    .iter()
                    .any(|(bs, be)| cs.max(bs) < ce.min(be))
            })
            .count();

        let json = format!(
            "{{\"name\":\"{name}\",\"path\":\"{rel}\",\"bytes\":{},\"sha256\":\"{rom_sha}\",
\"scanner_a\":{{\"shapes_available\":{},\"executed\":false,\"reason\":\"excluded from P0: consumes default_shapes() which is a corpus prior\"}},
\"scanner_b\":{{\"candidates\":{},\"runtime_ms\":{b_ms:.2},\"by_shape\":{}}},
\"scanner_c\":{{\"candidates\":{},\"runtime_ms\":{c_ms:.2},\"by_shape\":{},\"rows_in_fallback_list\":{c_axis_rows}}},
\"b_c_address_overlap\":{overlap}}}",
            rom.len(),
            default_shapes().len(),
            b.len(),
            serde_json::to_string(&b_by_shape).unwrap_or_else(|_| "{}".into()),
            c.len(),
            serde_json::to_string(&c_by_shape).unwrap_or_else(|_| "{}".into()),
        );
        per_rom.push(json);
        let _ = c_axis_cols;
        if idx > 0 {
            // separator handled below
        }
        eprintln!(
            "{name}: B={} ({b_ms:.0} ms)  C={} ({c_ms:.0} ms)  b∩c={overlap}",
            b.len(),
            c.len()
        );
    }

    report.push_str(&format!("\"scanner_b_total\":{b_total},\"scanner_c_total\":{c_total}}}"));
    report.push_str(",\n\"roms\":[\n");
    report.push_str(&per_rom.join(",\n"));
    report.push_str("\n]\n}\n");

    std::fs::write(&out_path, &report).expect("write output");
    println!("wrote {out_path}");
    println!("scanner B total candidates: {b_total}");
    println!("scanner C total candidates: {c_total}");
}
