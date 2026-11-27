//! 16-bit-aware structural pre-filter for Blind V2 calibration range discovery.
//!
//! Legacy `analyze_rom_blocks` prunes 4 KiB blocks whenever 8-bit entropy > 6.95 and raw byte delta > 85.0.
//! High-entropy 16-bit LE/BE calibration tables commonly alternate smoothly in high-order bytes while varying rapidly
//! in low-order bytes, causing false byte-delta pruning. Here we use finer 512 B blocks and only classify a dense
//! high-entropy block as machine code when BOTH LE and BE 16-bit adjacent word deltas exceed noise-like thresholds.

const BLOCK_SIZE: usize = 512;

fn block_word_mean_delta(chunk: &[u8], little_endian: bool) -> f64 {
    let (word_chunks, _) = chunk.as_chunks::<2>();
    let mut words = word_chunks.iter();
    let Some(&first_b) = words.next() else {
        return 0.0;
    };
    let read_w = |b: [u8; 2]| -> i32 {
        if little_endian {
            u16::from_le_bytes(b) as i32
        } else {
            u16::from_be_bytes(b) as i32
        }
    };
    let mut prev = read_w(first_b);
    let mut sum_d = 0u64;
    let mut pairs = 0usize;
    for &b in words {
        let cur = read_w(b);
        sum_d += cur.abs_diff(prev) as u64;
        prev = cur;
        pairs += 1;
    }
    if pairs == 0 {
        0.0
    } else {
        sum_d as f64 / pairs as f64
    }
}

fn should_prune_v2(chunk: &[u8]) -> bool {
    let n = chunk.len();
    if n == 0 {
        return true;
    }

    let mut counts = [0u16; 256];
    let mut max_c = 0u16;
    for &b in chunk {
        let idx = b as usize;
        counts[idx] += 1;
        if counts[idx] > max_c {
            max_c = counts[idx];
        }
    }

    // 1. Blank / filler padding: single byte > 92% of the 512 B block.
    if (max_c as usize) * 100 > n * 92 {
        return true;
    }

    // 2. Low Shannon entropy: constant / sparse repetition.
    let len_f = n as f64;
    let mut ent = 0.0f64;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / len_f;
            ent -= p * p.log2();
        }
    }
    if ent < 0.75 {
        return true;
    }

    // 3. Machine code: high entropy AND volatile across BOTH 8-bit bytes and 16-bit LE/BE words.
    if ent > 6.95 && n > 1 {
        let mut sum_d8 = 0u64;
        for j in 0..n - 1 {
            sum_d8 += chunk[j + 1].abs_diff(chunk[j]) as u64;
        }
        let mean_d8 = sum_d8 as f64 / (n - 1) as f64;
        if mean_d8 > 72.0 {
            let le_d16 = block_word_mean_delta(chunk, true);
            let be_d16 = block_word_mean_delta(chunk, false);
            if le_d16 > 6500.0 && be_d16 > 6500.0 {
                return true;
            }
        }
    }

    false
}

/// Returns coalesced non-pruned calibration byte ranges `[start, end)` using 512 B 16-bit-aware blocks.
pub(super) fn calibration_ranges_v2(rom: &[u8]) -> Vec<(usize, usize)> {
    if rom.len() < 1024 {
        return if rom.is_empty() {
            Vec::new()
        } else {
            vec![(0, rom.len())]
        };
    }

    let total_blocks = rom.len().div_ceil(BLOCK_SIZE);
    let mut kept = Vec::new();
    let mut in_run = false;
    let mut run_start = 0usize;

    for i in 0..total_blocks {
        let s = i * BLOCK_SIZE;
        let e = (s + BLOCK_SIZE).min(rom.len());
        if should_prune_v2(&rom[s..e]) {
            if in_run {
                in_run = false;
                kept.push((run_start, s));
            }
        } else if !in_run {
            in_run = true;
            run_start = s;
        }
    }
    if in_run {
        kept.push((run_start, rom.len()));
    }
    kept
}
