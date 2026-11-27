/**
 * 2D Matrix Interpolation and Adjustment Library for ECU Calibration
 */

/**
 * Performs 2D Bilinear Interpolation across the specified rectangular bounds of a matrix.
 * Interpolates smoothly using the 4 corner values (v00, v01, v10, v11).
 * If the selection is 1D (single row or single column), it gracefully degenerates to linear interpolation.
 */
export function interpolate2D(
  matrix: number[][],
  minR: number,
  maxR: number,
  minC: number,
  maxC: number
): number[][] {
  if (!matrix.length || !matrix[0]?.length) return matrix;

  const result = matrix.map((row) => [...row]);

  const r1 = Math.max(0, Math.min(minR, maxR));
  const r2 = Math.min(matrix.length - 1, Math.max(minR, maxR));
  const c1 = Math.max(0, Math.min(minC, maxC));
  const c2 = Math.min(matrix[0].length - 1, Math.max(minC, maxC));

  if (r1 === r2 && c1 === c2) return result;

  const v00 = matrix[r1][c1];
  const v01 = matrix[r1][c2];
  const v10 = matrix[r2][c1];
  const v11 = matrix[r2][c2];

  for (let r = r1; r <= r2; r++) {
    const tr = r2 === r1 ? 0 : (r - r1) / (r2 - r1);
    for (let c = c1; c <= c2; c++) {
      const tc = c2 === c1 ? 0 : (c - c1) / (c2 - c1);
      const top = v00 * (1 - tc) + v01 * tc;
      const bottom = v10 * (1 - tc) + v11 * tc;
      result[r][c] = top * (1 - tr) + bottom * tr;
    }
  }

  return result;
}

/**
 * Performs Horizontal Linear Interpolation row-by-row across the selected columns.
 * For each row r in [minR, maxR], linearly blends between matrix[r][minC] and matrix[r][maxC].
 */
export function interpolateHorizontal(
  matrix: number[][],
  minR: number,
  maxR: number,
  minC: number,
  maxC: number
): number[][] {
  if (!matrix.length || !matrix[0]?.length) return matrix;

  const result = matrix.map((row) => [...row]);

  const r1 = Math.max(0, Math.min(minR, maxR));
  const r2 = Math.min(matrix.length - 1, Math.max(minR, maxR));
  const c1 = Math.max(0, Math.min(minC, maxC));
  const c2 = Math.min(matrix[0].length - 1, Math.max(minC, maxC));

  if (c1 === c2) return result;

  for (let r = r1; r <= r2; r++) {
    const vStart = matrix[r][c1];
    const vEnd = matrix[r][c2];
    for (let c = c1; c <= c2; c++) {
      const tc = (c - c1) / (c2 - c1);
      result[r][c] = vStart * (1 - tc) + vEnd * tc;
    }
  }

  return result;
}

/**
 * Performs Vertical Linear Interpolation column-by-column across the selected rows.
 * For each column c in [minC, maxC], linearly blends between matrix[minR][c] and matrix[maxR][c].
 */
export function interpolateVertical(
  matrix: number[][],
  minR: number,
  maxR: number,
  minC: number,
  maxC: number
): number[][] {
  if (!matrix.length || !matrix[0]?.length) return matrix;

  const result = matrix.map((row) => [...row]);

  const r1 = Math.max(0, Math.min(minR, maxR));
  const r2 = Math.min(matrix.length - 1, Math.max(minR, maxR));
  const c1 = Math.max(0, Math.min(minC, maxC));
  const c2 = Math.min(matrix[0].length - 1, Math.max(minC, maxC));

  if (r1 === r2) return result;

  for (let c = c1; c <= c2; c++) {
    const vStart = matrix[r1][c];
    const vEnd = matrix[r2][c];
    for (let r = r1; r <= r2; r++) {
      const tr = (r - r1) / (r2 - r1);
      result[r][c] = vStart * (1 - tr) + vEnd * tr;
    }
  }

  return result;
}

/**
 * Adjusts all cells within selection bounds by adding delta.
 */
export function adjustSelection(
  matrix: number[][],
  minR: number,
  maxR: number,
  minC: number,
  maxC: number,
  delta: number,
  decimals?: number
): number[][] {
  if (!matrix.length || !matrix[0]?.length) return matrix;

  const result = matrix.map((row) => [...row]);

  const r1 = Math.max(0, Math.min(minR, maxR));
  const r2 = Math.min(matrix.length - 1, Math.max(minR, maxR));
  const c1 = Math.max(0, Math.min(minC, maxC));
  const c2 = Math.min(matrix[0].length - 1, Math.max(minC, maxC));

  for (let r = r1; r <= r2; r++) {
    for (let c = c1; c <= c2; c++) {
      const val = result[r][c] + delta;
      result[r][c] = decimals !== undefined ? Number(val.toFixed(decimals)) : Math.round(val * 1e6) / 1e6;
    }
  }

  return result;
}

/**
 * Scales all cells within selection bounds by percentage (e.g. +10% => 1.10, -5% => 0.95).
 */
export function scaleSelection(
  matrix: number[][],
  minR: number,
  maxR: number,
  minC: number,
  maxC: number,
  percentage: number,
  decimals?: number
): number[][] {
  if (!matrix.length || !matrix[0]?.length) return matrix;

  const factor = 1 + percentage / 100;
  const result = matrix.map((row) => [...row]);

  const r1 = Math.max(0, Math.min(minR, maxR));
  const r2 = Math.min(matrix.length - 1, Math.max(minR, maxR));
  const c1 = Math.max(0, Math.min(minC, maxC));
  const c2 = Math.min(matrix[0].length - 1, Math.max(minC, maxC));

  for (let r = r1; r <= r2; r++) {
    for (let c = c1; c <= c2; c++) {
      const val = result[r][c] * factor;
      result[r][c] = decimals !== undefined ? Number(val.toFixed(decimals)) : Math.round(val * 1e6) / 1e6;
    }
  }

  return result;
}

/**
 * Sets all cells within selection bounds to a constant value.
 */
export function setSelectionValue(
  matrix: number[][],
  minR: number,
  maxR: number,
  minC: number,
  maxC: number,
  value: number
): number[][] {
  if (!matrix.length || !matrix[0]?.length) return matrix;

  const result = matrix.map((row) => [...row]);

  const r1 = Math.max(0, Math.min(minR, maxR));
  const r2 = Math.min(matrix.length - 1, Math.max(minR, maxR));
  const c1 = Math.max(0, Math.min(minC, maxC));
  const c2 = Math.min(matrix[0].length - 1, Math.max(minC, maxC));

  for (let r = r1; r <= r2; r++) {
    for (let c = c1; c <= c2; c++) {
      result[r][c] = value;
    }
  }

  return result;
}

/**
 * Performs a 3x3 weighted Gaussian-like smoothing filter on the selected sub-rectangle.
 * Kernel weights:
 * [1, 2, 1]
 * [2, 4, 2]
 * [1, 2, 1]
 * With boundary normalization (omitting off-grid neighbors and dividing by actual sum of weights).
 */
export function smooth3x3(
  matrix: number[][],
  rMin: number,
  rMax: number,
  cMin: number,
  cMax: number,
  decimals?: number
): number[][] {
  if (!matrix.length || !matrix[0]?.length) return matrix;

  const numRows = matrix.length;
  const numCols = matrix[0].length;
  const result = matrix.map((row) => [...row]);

  const r1 = Math.max(0, Math.min(rMin, rMax));
  const r2 = Math.min(numRows - 1, Math.max(rMin, rMax));
  const c1 = Math.max(0, Math.min(cMin, cMax));
  const c2 = Math.min(numCols - 1, Math.max(cMin, cMax));

  for (let r = r1; r <= r2; r++) {
    for (let c = c1; c <= c2; c++) {
      let sumWeighted = 0;
      let sumWeights = 0;

      for (let dr = -1; dr <= 1; dr++) {
        const nr = r + dr;
        if (nr < 0 || nr >= numRows) continue;
        for (let dc = -1; dc <= 1; dc++) {
          const nc = c + dc;
          if (nc < 0 || nc >= numCols) continue;

          const w = dr === 0 && dc === 0 ? 4 : dr === 0 || dc === 0 ? 2 : 1;
          sumWeighted += matrix[nr][nc] * w;
          sumWeights += w;
        }
      }

      if (sumWeights > 0) {
        const smoothed = sumWeighted / sumWeights;
        result[r][c] =
          decimals !== undefined
            ? Number(smoothed.toFixed(decimals))
            : Math.round(smoothed * 1e6) / 1e6;
      }
    }
  }

  return result;
}
