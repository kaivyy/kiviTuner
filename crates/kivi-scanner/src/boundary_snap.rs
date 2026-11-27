//! Boundary snapping: recover the true start of a 1-D run from a bleeding one.
//!
//! ## The defect this fixes
//!
//! A curve detector anchored on rise/fall starts wherever the run begins, and in
//! a real ROM the bytes before a table are live data. Measured on the CBR250RR
//! ROM, the declared 25-point TPS axis at `0x04FF80` is reported starting at
//! `0x04FF78` — 8 bytes early — because this prefix reads as part of the run:
//!
//! ```text
//! 0x04FF70:  33
//! 0x04FF72:  4000
//! 0x04FF74:  33
//! 0x04FF76:  3000
//! 0x04FF78:  33     <-- detector starts here
//! 0x04FF7A:  1500
//! 0x04FF7C:  33
//! 0x04FF7E:  0
//! 0x04FF80:  0      <-- oracle start
//! 0x04FF82:  3
//! 0x04FF84:  8
//! ```
//!
//! ## The rule
//!
//! 1. **Tail split** ([`split_at_gradient_anomaly`]): a hard gradient jump ends
//!    the run, so garbage after the last real cell is discarded before anything
//!    else is measured.
//! 2. **Head trim**: drop leading values that violate the run's own direction,
//!    or that jump far relative to the body's median step.
//! 3. **Duplicate advance**: step past a run of equal leading values.
//!
//! The direction is taken from the **body** (the longer of the two monotonic
//! halves), not from the first step, because the first step is exactly what the
//! bleeding corrupted.
//!
//! ## Limits
//!
//! Only 1-D runs are snapped. A 2-D table's start cannot be recovered this way:
//! measured on the CBR 35x25 table at `0x050700`, the exact offset never appears
//! among the 71 covering candidates, so there is nothing to snap.
//!
//! A window that starts *inside* the table cannot be recovered at all — the
//! missing cells are simply not in the input. Measured: the CBR map axis at
//! `0x04FFB2` is reported at `0x04FFC2` (+16, eight cells late), and no
//! head-trimming rule can move a start backwards.

/// Steps examined when estimating the local gradient before a candidate split.
pub const LOCAL_STEP_WINDOW: usize = 5;

/// A step this many times the local median step is a hard jump.
///
/// Measured on the BMW B58 ROM at `0x700F82`: the six real cells step by
/// `300, 1000, 1000, 500, 1000`, but the cell after them steps by `59736`
/// (`5500 -> 65236`). That is 59x the local median, so any multiplier between
/// about 2 and 59 separates them. 10 keeps the rule well clear of both real
/// calibration ramps and ordinary SD-card-style noise.
pub const HARD_JUMP_MULTIPLIER: f64 = 10.0;

/// A step larger than this is a hard jump even when the local median is itself
/// large, so a garbage tail cannot protect itself by making the median huge.
///
/// This is the "jumps from a calibration range into the top of `u16`" case:
/// `65236`, `65386` and `65426` are all within 300 of `u16::MAX`.
pub const HARD_JUMP_FLOOR: i64 = 5000;

/// Equal leading cells before they count as padding (CBR TPS starts `0, 0, 3`).
pub const MIN_PADDING_RUN: usize = 4;

/// Shortest run [`snap_monotonic`] will accept.
pub const MIN_POINTS: usize = 3;

/// Cells dropped from one 1-D run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Snapped {
    /// How many leading cells were dropped.
    pub trimmed_head: usize,
    /// How many trailing cells were dropped.
    pub trimmed_tail: usize,
    /// Number of cells after trimming.
    pub points: usize,
}

/// Cuts a run at a hard gradient jump that follows an established ramp.
///
/// Three conditions, each forced by a measured failure: an established ramp
/// (3 same-direction steps), a local ratio jump, and a half-range
/// discontinuity (B58 `0x700FD4` needs the last one — its own steps are large
/// enough to hide the `65 535 -> 0` reset).
pub fn split_at_gradient_anomaly(values: &[u16], max_allowed_step_multiplier: f64) -> &[u16] {
    if values.len() < 3 {
        return values;
    }
    let run_max = values.iter().copied().max().unwrap_or(0) as i64;
    let half_range = run_max / 2;

    // Length of the run of same-direction steps ending at each step index.
    let n_steps = values.len() - 1;
    let mut consecutive = vec![1usize; n_steps];
    for j in 1..n_steps {
        let prev = values[j] as i64 - values[j - 1] as i64;
        let cur = values[j + 1] as i64 - values[j] as i64;
        if (prev > 0) == (cur > 0) && prev != 0 && cur != 0 {
            consecutive[j] = consecutive[j - 1] + 1;
        }
    }

    for i in 2..values.len() {
        // Ramp must be established: three same-direction steps ending at i-2.
        if consecutive[i - 2] < 3 {
            continue;
        }
        let step_prev = (values[i - 1] as i64 - values[i - 2] as i64).abs();
        let step_curr = (values[i] as i64 - values[i - 1] as i64).abs();
        let limit = ((step_prev as f64) * max_allowed_step_multiplier).max(HARD_JUMP_FLOOR as f64);
        if step_curr as f64 > limit || step_curr > half_range {
            return &values[..i];
        }
    }
    values
}

/// Step jump over this many times the local median marks a seam.
pub const SEAM_STEP_MULTIPLIER: f64 = 3.0;

/// Length of the run starting at `values[0]`, cut at the first seam.
///
/// Measured on CBR250RR: `661 -> 118` is a reversal, `594 -> 700` (+106) is a
/// step jump where local steps were +30, +28.
pub fn seam_len(values: &[u16]) -> usize {
    if values.len() < 3 {
        return values.len();
    }
    let mut dir: i8 = 0;
    let mut recent = [0i64; 5];
    let mut recent_len = 0usize;
    let mut scratch = [0i64; 5];
    for j in 0..values.len() - 1 {
        let step = values[j + 1] as i64 - values[j] as i64;
        let sign: i8 = if step > 0 { 1 } else if step < 0 { -1 } else { 0 };
        if j >= 2 && dir != 0 && sign != 0 && sign != dir {
            return j + 1;
        }
        if recent_len >= 3 {
            for (dst, &v) in scratch[..recent_len].iter_mut().zip(recent[..recent_len].iter()) {
                *dst = v.abs();
            }
            scratch[..recent_len].sort_unstable();
            let med = (scratch[recent_len / 2] as f64).max(1.0);
            if (step as f64).abs() > med * SEAM_STEP_MULTIPLIER {
                return j + 1;
            }
        }
        if sign != 0 {
            dir = sign;
        }
        if recent_len < 5 {
            recent[recent_len] = step;
            recent_len += 1;
        } else {
            recent.copy_within(1..5, 0);
            recent[4] = step;
        }
    }
    values.len()
}

/// Snaps a monotonic run back to its true start and end.
///
/// Returns `None` when the run is too short after trimming to be a calibration
/// row, or when no consistent direction could be established.
pub fn snap_monotonic(values: &[u16]) -> Option<Snapped> {
    // Discard a hard-jump tail first: leaving garbage in the run corrupts both
    // the median step and the direction vote.
    let body = split_at_gradient_anomaly(values, HARD_JUMP_MULTIPLIER);
    let tail_cut = values.len() - body.len();
    snap_head_only(body).map(|mut s| {
        s.trimmed_tail = tail_cut;
        s
    })
}

/// Head trimming alone, on a body already known to be free of a hard-jump tail.
fn snap_head_only(values: &[u16]) -> Option<Snapped> {
    if values.len() < 4 {
        return None;
    }

    // Direction from the longest strictly-monotonic half, so the bleeding prefix
    // cannot set it.
    let mid = values.len() / 2;
    let up_first = values[..mid].windows(2).filter(|w| w[1] > w[0]).count();
    let down_first = values[..mid].windows(2).filter(|w| w[1] < w[0]).count();
    let up_second = values[mid..].windows(2).filter(|w| w[1] > w[0]).count();
    let down_second = values[mid..].windows(2).filter(|w| w[1] < w[0]).count();
    let up = (up_first + up_second) >= (down_first + down_second);
    let consistent = |a: u16, b: u16| if up { b >= a } else { b <= a };

    // Trim the head while the step violates the direction, or is a large jump
    // relative to the median body step.
    let steps: Vec<i64> = values.windows(2).map(|w| w[1] as i64 - w[0] as i64).collect();
    let mut sorted_steps: Vec<i64> = steps.iter().map(|s| s.abs()).collect();
    sorted_steps.sort_unstable();
    let median_step = sorted_steps[sorted_steps.len() / 2].max(1);
    let jump_limit = median_step * 20;

    let mut head = 0usize;
    while head + 1 < values.len() {
        let a = values[head];
        let b = values[head + 1];
        let step = (b as i64 - a as i64).abs();
        let violates = !consistent(a, b);
        // A leading zero followed by a rise is padding, not a violation: keep
        // trimming only until the run settles into its direction.
        if violates || step > jump_limit {
            head += 1;
        } else {
            break;
        }
    }

    // The tail is already clean: `split_at_gradient_anomaly` cut it. Trimming it
    // again here would double-count the cut and, worse, a second rule can cut a
    // real monotonic ramp whose last step is legitimately large.
    let tail = values.len();

    // Leading duplicates: a run of equal values at the head is padding before
    // the first real cell; advance to the LAST of them.
    while head + 1 < tail && values[head] == values[head + 1] {
        head += 1;
    }

    let points = tail.saturating_sub(head);
    if points < 3 {
        return None;
    }
    Some(Snapped {
        trimmed_head: head,
        trimmed_tail: values.len() - tail,
        points,
    })
}
