//! Similar maps, parallel map clustering, and cross-ROM migration/delta transfer.
//!
//! Scans ROM images for maps matching a source 2-D template by shape-preserving
//! Pearson correlation (`r`), normalized RMSE (`NRMSE`), and optional max relative
//! difference, with strict non-overlapping NMS and cross-revision delta transfer.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Specification of a table within a ROM buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarMapSpec {
    pub address: usize,
    pub rows: usize,
    pub cols: usize,
    pub cell_bits: u16,
    pub little_endian: bool,
    pub is_signed: bool,
}

impl SimilarMapSpec {
    /// Byte span occupied by the table cells, or `None` if width/dimensions are invalid.
    pub fn byte_len(&self) -> Option<usize> {
        if self.rows == 0 || self.cols == 0 || !matches!(self.cell_bits, 8 | 16 | 32) {
            return None;
        }
        let elem_bytes = (self.cell_bits / 8) as usize;
        self.rows
            .checked_mul(self.cols)?
            .checked_mul(elem_bytes)
    }
}

/// Options controlling similarity search in [`find_similar_maps_in_rom`] and
/// [`find_migrated_map_across_roms`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarMapOptions {
    pub min_pearson_r: f64,
    pub max_nrmse: f64,
    pub max_rel_diff: Option<f64>,
    /// Scan alignment stride in bytes; `0` selects `cell_bits / 8`.
    pub stride_bytes: usize,
    pub exclude_overlapping_source: bool,
    pub max_results: usize,
}

impl Default for SimilarMapOptions {
    fn default() -> Self {
        Self {
            min_pearson_r: 0.90,
            max_nrmse: 0.20,
            max_rel_diff: None,
            stride_bytes: 0,
            exclude_overlapping_source: true,
            max_results: 64,
        }
    }
}

/// One candidate match found by similarity search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarMapHit {
    pub address: usize,
    pub rows: usize,
    pub cols: usize,
    pub cell_bits: u16,
    pub little_endian: bool,
    pub pearson_r: f64,
    pub nrmse: f64,
    pub mean_abs_diff: f64,
    pub max_rel_diff: f64,
    pub is_exact_copy: bool,
}

/// A cluster of structurally similar (twin / parallel) maps sharing geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParallelMapGroup {
    pub group_id: usize,
    pub anchor_address: usize,
    pub member_addresses: Vec<usize>,
    pub rows: usize,
    pub cols: usize,
    pub cell_bits: u16,
    pub min_pearson_r: f64,
}

/// Mode used by [`transfer_map_delta`] when porting calibration changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeltaTransferMode {
    Absolute,
    AdditiveDelta,
    PercentageDelta,
}

#[inline]
fn read_elem_at(
    rom: &[u8],
    off: usize,
    cell_bits: u16,
    little_endian: bool,
    is_signed: bool,
) -> Option<f64> {
    match (cell_bits, is_signed) {
        (8, false) => rom.get(off).map(|&b| b as f64),
        (8, true) => rom.get(off).map(|&b| (b as i8) as f64),
        (16, false) => {
            let b: [u8; 2] = rom.get(off..off + 2)?.try_into().ok()?;
            Some(if little_endian {
                u16::from_le_bytes(b)
            } else {
                u16::from_be_bytes(b)
            } as f64)
        }
        (16, true) => {
            let b: [u8; 2] = rom.get(off..off + 2)?.try_into().ok()?;
            Some(if little_endian {
                i16::from_le_bytes(b)
            } else {
                i16::from_be_bytes(b)
            } as f64)
        }
        (32, false) => {
            let b: [u8; 4] = rom.get(off..off + 4)?.try_into().ok()?;
            Some(if little_endian {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            } as f64)
        }
        (32, true) => {
            let b: [u8; 4] = rom.get(off..off + 4)?.try_into().ok()?;
            Some(if little_endian {
                i32::from_le_bytes(b)
            } else {
                i32::from_be_bytes(b)
            } as f64)
        }
        _ => None,
    }
}

/// Decodes a map described by `spec` into a row-major flat vector of `f64` cells.
pub fn decode_map_flat(rom: &[u8], spec: &SimilarMapSpec) -> Option<Vec<f64>> {
    let span_bytes = spec.byte_len()?;
    let end = spec.address.checked_add(span_bytes)?;
    if end > rom.len() {
        return None;
    }
    let n = spec.rows * spec.cols;
    let elem_bytes = (spec.cell_bits / 8) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let off = spec.address + i * elem_bytes;
        out.push(read_elem_at(
            rom,
            off,
            spec.cell_bits,
            spec.little_endian,
            spec.is_signed,
        )?);
    }
    Some(out)
}

#[derive(Debug, Clone, Copy)]
struct PairMetrics {
    pearson_r: f64,
    nrmse: f64,
}

fn compare_flat_vectors(a: &[f64], b: &[f64], norm_span: f64) -> Option<PairMetrics> {
    if a.len() != b.len() || a.len() < 2 {
        return None;
    }
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    let mut cov = 0.0;
    let mut sum_sq_diff = 0.0;

    for i in 0..a.len() {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        var_a += da * da;
        var_b += db * db;
        cov += da * db;
        let abs_diff = (a[i] - b[i]).abs();
        sum_sq_diff += abs_diff * abs_diff;
    }

    if var_a <= 1e-12 || var_b <= 1e-12 {
        return None;
    }
    let pearson_r = (cov / (var_a * var_b).sqrt()).clamp(-1.0, 1.0);
    let rmse = (sum_sq_diff / n).sqrt();
    let nrmse = rmse / norm_span.max(1.0);
    Some(PairMetrics { pearson_r, nrmse })
}

fn search_rom_for_source_values(
    rom: &[u8],
    src_vals: &[f64],
    source: &SimilarMapSpec,
    opts: &SimilarMapOptions,
) -> Vec<SimilarMapHit> {
    let Some(span_bytes) = source.byte_len() else {
        return Vec::new();
    };
    if span_bytes > rom.len() || src_vals.len() < 2 || opts.max_results == 0 {
        return Vec::new();
    }
    let elem_bytes = (source.cell_bits / 8) as usize;
    let stride = if opts.stride_bytes == 0 {
        elem_bytes
    } else {
        opts.stride_bytes
    };
    let n = src_vals.len();
    let n_f = n as f64;

    let mut src_min = f64::INFINITY;
    let mut src_max = f64::NEG_INFINITY;
    let mut src_sum = 0.0;
    for &v in src_vals {
        if v < src_min {
            src_min = v;
        }
        if v > src_max {
            src_max = v;
        }
        src_sum += v;
    }
    let src_mean = src_sum / n_f;
    let src_var_sum: f64 = src_vals.iter().map(|&v| (v - src_mean).powi(2)).sum();
    if src_var_sum <= 1e-12 {
        return Vec::new();
    }
    let src_span = (src_max - src_min).max(1.0);
    let sq_err_limit = n_f * (opts.max_nrmse.max(0.0) * src_span).powi(2) + 1e-9;

    let src_end = source.address.saturating_add(span_bytes);
    let last_start = rom.len() - span_bytes;
    let offsets: Vec<usize> = (0..=last_start)
        .step_by(stride)
        .filter(|&off| {
            !opts.exclude_overlapping_source
                || off + span_bytes <= source.address
                || off >= src_end
        })
        .collect();

    let mut raw_hits: Vec<SimilarMapHit> = offsets
        .par_chunks(2048)
        .flat_map_iter(|chunk| {
            chunk.iter().filter_map(|&off| {
                let mut sum_y = 0.0;
                let mut sum_y2 = 0.0;
                let mut sum_xy = 0.0;
                let mut sum_sq_diff = 0.0;
                let mut sum_abs_diff = 0.0;
                let mut max_rel_diff = 0.0_f64;

                for (i, &x) in src_vals.iter().enumerate() {
                    let y = read_elem_at(
                        rom,
                        off + i * elem_bytes,
                        source.cell_bits,
                        source.little_endian,
                        source.is_signed,
                    )?;
                    let abs_diff = (y - x).abs();
                    sum_sq_diff += abs_diff * abs_diff;
                    // Monotonic partial sum: cannot decrease, so abort early once NRMSE bound fails.
                    if sum_sq_diff > sq_err_limit {
                        return None;
                    }
                    let rel = abs_diff / x.abs().max(1.0);
                    if let Some(rel_lim) = opts.max_rel_diff {
                        if rel > rel_lim {
                            return None;
                        }
                    }
                    if rel > max_rel_diff {
                        max_rel_diff = rel;
                    }
                    sum_abs_diff += abs_diff;
                    sum_y += y;
                    sum_y2 += y * y;
                    sum_xy += x * y;
                }

                let cand_var_sum = sum_y2 - (sum_y * sum_y) / n_f;
                if cand_var_sum <= 1e-12 {
                    return None;
                }
                let cov = sum_xy - (src_sum * sum_y) / n_f;
                let pearson_r = (cov / (src_var_sum * cand_var_sum).sqrt()).clamp(-1.0, 1.0);
                let nrmse = (sum_sq_diff / n_f).sqrt() / src_span;

                if pearson_r < opts.min_pearson_r || nrmse > opts.max_nrmse {
                    return None;
                }

                Some(SimilarMapHit {
                    address: off,
                    rows: source.rows,
                    cols: source.cols,
                    cell_bits: source.cell_bits,
                    little_endian: source.little_endian,
                    pearson_r,
                    nrmse,
                    mean_abs_diff: sum_abs_diff / n_f,
                    max_rel_diff,
                    is_exact_copy: sum_sq_diff == 0.0,
                })
            })
        })
        .collect();

    raw_hits.sort_by(|a, b| {
        b.is_exact_copy
            .cmp(&a.is_exact_copy)
            .then_with(|| a.nrmse.total_cmp(&b.nrmse))
            .then_with(|| b.pearson_r.total_cmp(&a.pearson_r))
            .then_with(|| a.address.cmp(&b.address))
    });

    // Strict non-overlapping NMS by hit byte span.
    let mut kept: Vec<SimilarMapHit> = Vec::with_capacity(raw_hits.len().min(opts.max_results));
    for hit in raw_hits {
        let h_start = hit.address;
        let h_end = h_start + span_bytes;
        let overlaps = kept
            .iter()
            .any(|k| h_start < k.address + span_bytes && h_end > k.address);
        if overlaps {
            continue;
        }
        kept.push(hit);
        if kept.len() >= opts.max_results {
            break;
        }
    }
    kept
}

/// Scans `rom` for maps structurally similar to the template at `source`.
pub fn find_similar_maps_in_rom(
    rom: &[u8],
    source: &SimilarMapSpec,
    opts: &SimilarMapOptions,
) -> Vec<SimilarMapHit> {
    let Some(src_vals) = decode_map_flat(rom, source) else {
        return Vec::new();
    };
    search_rom_for_source_values(rom, &src_vals, source, opts)
}

/// Extracts `source` from `source_rom` and locates corresponding (possibly shifted)
/// maps inside `target_rom`.
pub fn find_migrated_map_across_roms(
    source_rom: &[u8],
    target_rom: &[u8],
    source: &SimilarMapSpec,
    opts: &SimilarMapOptions,
) -> Vec<SimilarMapHit> {
    let Some(src_vals) = decode_map_flat(source_rom, source) else {
        return Vec::new();
    };
    let mut target_opts = opts.clone();
    target_opts.exclude_overlapping_source = false;
    search_rom_for_source_values(target_rom, &src_vals, source, &target_opts)
}

struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

/// Clusters compatible same-geometry maps into synchronized parallel-map groups.
pub fn discover_parallel_map_groups(
    rom: &[u8],
    maps: &[SimilarMapSpec],
    min_pearson_r: f64,
    max_nrmse: f64,
) -> Vec<ParallelMapGroup> {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct BucketKey {
        rows: usize,
        cols: usize,
        cell_bits: u16,
        little_endian: bool,
        is_signed: bool,
    }

    struct DecodedMap {
        spec: SimilarMapSpec,
        vals: Vec<f64>,
        span: f64,
    }

    let mut buckets: BTreeMap<BucketKey, Vec<DecodedMap>> = BTreeMap::new();
    for &spec in maps {
        let Some(vals) = decode_map_flat(rom, &spec) else {
            continue;
        };
        if vals.len() < 2 {
            continue;
        }
        let min_v = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let max_v = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span = (max_v - min_v).max(1.0);
        let key = BucketKey {
            rows: spec.rows,
            cols: spec.cols,
            cell_bits: spec.cell_bits,
            little_endian: spec.little_endian,
            is_signed: spec.is_signed,
        };
        buckets.entry(key).or_default().push(DecodedMap { spec, vals, span });
    }

    let mut raw_groups: Vec<ParallelMapGroup> = Vec::new();
    for (key, mut entries) in buckets {
        entries.sort_by_key(|e| e.spec.address);
        entries.dedup_by_key(|e| e.spec.address);
        if entries.len() < 2 {
            continue;
        }

        let m = entries.len();
        let mut dsu = DisjointSet::new(m);
        for i in 0..m {
            for j in (i + 1)..m {
                let norm = entries[i].span.min(entries[j].span);
                if let Some(pm) = compare_flat_vectors(&entries[i].vals, &entries[j].vals, norm) {
                    if pm.pearson_r >= min_pearson_r && pm.nrmse <= max_nrmse {
                        dsu.union(i, j);
                    }
                }
            }
        }

        let mut comps: HashMap<usize, Vec<usize>> = HashMap::new();
        for idx in 0..m {
            comps.entry(dsu.find(idx)).or_default().push(idx);
        }

        for (_root, member_idxs) in comps {
            if member_idxs.len() < 2 {
                continue;
            }
            let mut min_r = 1.0_f64;
            for a_pos in 0..member_idxs.len() {
                for b_pos in (a_pos + 1)..member_idxs.len() {
                    let i = member_idxs[a_pos];
                    let j = member_idxs[b_pos];
                    let norm = entries[i].span.min(entries[j].span);
                    if let Some(pm) = compare_flat_vectors(&entries[i].vals, &entries[j].vals, norm)
                    {
                        if pm.pearson_r < min_r {
                            min_r = pm.pearson_r;
                        }
                    } else {
                        min_r = min_r.min(0.0);
                    }
                }
            }
            let mut member_addresses: Vec<usize> = member_idxs
                .iter()
                .map(|&idx| entries[idx].spec.address)
                .collect();
            member_addresses.sort_unstable();
            raw_groups.push(ParallelMapGroup {
                group_id: 0,
                anchor_address: member_addresses[0],
                member_addresses,
                rows: key.rows,
                cols: key.cols,
                cell_bits: key.cell_bits,
                min_pearson_r: min_r,
            });
        }
    }

    raw_groups.sort_by_key(|g| (g.anchor_address, g.rows, g.cols, g.cell_bits));
    for (idx, g) in raw_groups.iter_mut().enumerate() {
        g.group_id = idx + 1;
    }
    raw_groups
}

/// Transfers calibration modifications from `source` onto `target_orig` using `mode`.
pub fn transfer_map_delta(
    source_orig: &[Vec<f64>],
    source_tuned: &[Vec<f64>],
    target_orig: &[Vec<f64>],
    mode: DeltaTransferMode,
) -> Result<Vec<Vec<f64>>, String> {
    let rows = source_orig.len();
    if rows == 0 {
        return Err("source_orig matrix cannot be empty".to_string());
    }
    let cols = source_orig[0].len();
    if cols == 0 || source_orig.iter().any(|r| r.len() != cols) {
        return Err("source_orig matrix must be rectangular and non-empty".to_string());
    }
    let same_shape = |m: &[Vec<f64>]| m.len() == rows && m.iter().all(|r| r.len() == cols);
    if !same_shape(source_tuned) || !same_shape(target_orig) {
        return Err(format!(
            "all matrices must share {rows}x{cols} dimensions for delta transfer"
        ));
    }

    let mut out = vec![vec![0.0; cols]; rows];
    for r in 0..rows {
        for c in 0..cols {
            let so = source_orig[r][c];
            let st = source_tuned[r][c];
            let to = target_orig[r][c];
            out[r][c] = match mode {
                DeltaTransferMode::Absolute => st,
                DeltaTransferMode::AdditiveDelta => to + (st - so),
                DeltaTransferMode::PercentageDelta => {
                    if so.abs() > 1e-9 {
                        to * (st / so)
                    } else {
                        to + (st - so)
                    }
                }
            };
        }
    }
    Ok(out)
}
