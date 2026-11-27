//! Pointer-guided snapping: shift an already-detected candidate onto a
//! pointer target found in the ROM itself.
//!
//! The set of anchors is derived from ROM structure only (constant-stride
//! pointer arrays and literal pools). It never adds a candidate — it can only
//! move one that the detector already produced, so the candidate count cannot
//! grow.

use std::collections::HashMap;

/// Pointer-derived anchors: `target -> stride in bytes`, when known.
#[derive(Debug, Clone, Default)]
pub struct Anchors {
    pub by_target: HashMap<usize, Option<usize>>,
    /// Targets sorted, so `nearest` bisects instead of scanning. A ROM with a
    /// dense literal pool yields thousands of anchors; a linear scan per
    /// candidate turns snapping into the slowest step of a scan.
    sorted: Vec<usize>,
}

impl Anchors {
    fn extract_constant_stride_arrays(
        rom: &[u8],
        big_endian: bool,
        min_run: usize,
        min_addr: usize,
        max_stride: i64,
        require_even_inbounds: bool,
        by_target: &mut HashMap<usize, Option<usize>>,
    ) {
        let n = rom.len();
        let word = |a: usize| -> u32 {
            let b = [rom[a], rom[a + 1], rom[a + 2], rom[a + 3]];
            if big_endian {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            }
        };
        let in_rom = |v: u32| (v as usize) >= min_addr && (v as usize) < n;

        let mut a = 0usize;
        while a + 4 <= n {
            if !in_rom(word(a)) {
                a += 2;
                continue;
            }
            let mut seq: Vec<usize> = Vec::new();
            let mut p = a;
            while p + 4 <= n && in_rom(word(p)) {
                seq.push(word(p) as usize);
                p += 4;
                if seq.len() > 128 {
                    break;
                }
            }
            let mut i = 0usize;
            while i + 1 < seq.len() {
                let d = seq[i + 1] as i64 - seq[i] as i64;
                let mut j = i + 1;
                while j + 1 < seq.len()
                    && seq[j + 1] as i64 - seq[j] as i64 == d
                {
                    j += 1;
                }
                if j - i + 1 >= min_run && d >= 64 && d <= max_stride {
                    let stride_u = d as usize;
                    for &t in &seq[i..=j] {
                        if !require_even_inbounds
                            || (t >= min_addr && t.is_multiple_of(2) && t.saturating_add(stride_u) <= n)
                        {
                            by_target.entry(t).or_insert(Some(stride_u));
                        }
                    }
                }
                i = if j == i { i + 1 } else { j };
            }
            a = p;
        }
    }

    /// Builds constant-stride pointer-array anchors without the slower literal-pool scan.
    pub fn constant_stride_only(rom: &[u8], big_endian: bool) -> Self {
        let mut by_target: HashMap<usize, Option<usize>> = HashMap::new();
        Self::extract_constant_stride_arrays(
            rom,
            big_endian,
            3,
            0x400,
            16_384,
            true,
            &mut by_target,
        );
        let mut sorted: Vec<usize> = by_target.keys().copied().collect();
        sorted.sort_unstable();
        Self { by_target, sorted }
    }

    /// Sorted `(target_address, stride_bytes)` pairs discovered from constant-stride pointer arrays.
    pub fn strong_targets(&self) -> Vec<(usize, usize)> {
        self.sorted
            .iter()
            .filter_map(|&t| {
                self.by_target
                    .get(&t)
                    .and_then(|s| s.map(|stride| (t, stride)))
            })
            .collect()
    }

    /// Builds the anchor set from raw ROM bytes only.
    ///
    /// Two structural shapes are accepted, both measured on CBR250RR:
    ///
    /// * **Constant-stride array** — at least four consecutive 32-bit big-endian
    ///   words that all point inside the ROM and are separated by the same
    ///   distance of at least 64 bytes. Measured at `0x04BC14`: eight entries
    ///   with stride 1750 (`35 * 25 * 2`).
    /// * **Literal pool** — a word that points inside the ROM and has a
    ///   `0xFFFF....` word within 16 bytes on either side. Measured at
    ///   `0x00D040`: the axis pointers sit among `0xFFFF2xxx` entries.
    pub fn from_rom(rom: &[u8], big_endian: bool) -> Self {
        let n = rom.len();
        let word = |a: usize| -> u32 {
            let b = [rom[a], rom[a + 1], rom[a + 2], rom[a + 3]];
            if big_endian {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            }
        };
        let in_rom = |v: u32| (v as usize) >= 0x1000 && (v as usize) < n;

        let mut by_target: HashMap<usize, Option<usize>> = HashMap::new();
        Self::extract_constant_stride_arrays(
            rom,
            big_endian,
            4,
            0x1000,
            i64::MAX,
            false,
            &mut by_target,
        );

        // Literal pools: an in-ROM pointer with a 0xFFFF.... neighbour.
        let mut a = 0usize;
        while a + 4 <= n {
            let v = word(a);
            if in_rom(v) {
                let lo = a.saturating_sub(16);
                let hi = (a + 20).min(n - 3);
                let mut pooled = false;
                let mut c = lo;
                while c <= hi {
                    if word(c) >= 0xFFFF_0000 {
                        pooled = true;
                        break;
                    }
                    c += 4;
                }
                if pooled {
                    by_target.entry(v as usize).or_insert(None);
                }
            }
            a += 2;
        }

        let mut sorted: Vec<usize> = by_target.keys().copied().collect();
        sorted.sort_unstable();
        Self { by_target, sorted }
    }

    pub fn len(&self) -> usize {
        self.by_target.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_target.is_empty()
    }

    /// Nearest anchor that came from a constant-stride array.
    ///
    /// A literal pool proves only that some code references an address; a
    /// constant-stride array proves the addresses are a table boundary. On
    /// ROMs dense with code (BMW), the literal pools are close enough to pull
    /// candidates onto themselves and lose real tables, so the weak anchors can
    /// be excluded.
    pub fn nearest_strong(&self, address: usize, radius: usize) -> Option<(usize, Option<usize>)> {
        let lo = address.saturating_sub(radius);
        let hi = address.saturating_add(radius);
        let start = self.sorted.partition_point(|&t| t < lo);
        let end = self.sorted.partition_point(|&t| t <= hi);
        let mut best: Option<(usize, Option<usize>)> = None;
        for &t in &self.sorted[start..end] {
            let Some(stride) = self.by_target[&t] else {
                continue;
            };
            let d = t.abs_diff(address);
            match best {
                None => best = Some((t, Some(stride))),
                Some((bt, _)) => {
                    let bd = bt.abs_diff(address);
                    if d < bd || (d == bd && t > bt) {
                        best = Some((t, Some(stride)));
                    }
                }
            }
        }
        best
    }

    /// Nearest anchor within `radius` bytes of `address`.
    ///
    /// Ties break toward the smaller absolute distance, then toward the higher
    /// address, because a table that starts one cell before a pointer is far
    /// rarer than one whose detector window opened a cell early.
    pub fn nearest(&self, address: usize, radius: usize) -> Option<(usize, Option<usize>)> {
        let lo = address.saturating_sub(radius);
        let hi = address.saturating_add(radius);
        let start = self.sorted.partition_point(|&t| t < lo);
        let end = self.sorted.partition_point(|&t| t <= hi);
        let mut best: Option<(usize, Option<usize>)> = None;
        for &t in &self.sorted[start..end] {
            let d = t.abs_diff(address);
            match best {
                None => best = Some((t, self.by_target[&t])),
                Some((bt, _)) => {
                    let bd = bt.abs_diff(address);
                    if d < bd || (d == bd && t > bt) {
                        best = Some((t, self.by_target[&t]));
                    }
                }
            }
        }
        best
    }
}
