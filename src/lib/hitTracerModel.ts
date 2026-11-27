//! Pure state model for live heatmap cell hit-tracing and dwell trail visualization.

export const HIT_TRAIL_MAX_LENGTH = 12;

export interface HitTracerState {
  counts: Record<string, number>;
  totalHits: number;
  maxHits: number;
  /** Most-recent-first unique `"r:c"` cell coordinates, capped at `HIT_TRAIL_MAX_LENGTH`. */
  recentTrail: string[];
}

export interface CellHitVisual {
  count: number;
  /** Normalized dwell intensity `[0, 1]` relative to the hottest cell in this table. */
  intensity: number;
  /** `0` for newest trail cell up to `HIT_TRAIL_MAX_LENGTH - 1`, or `null` if not in recent trail. */
  trailRank: number | null;
}

export function hitCellKey(r: number, c: number): string {
  return `${r}:${c}`;
}

export function createHitTracerState(): HitTracerState {
  return {
    counts: {},
    totalHits: 0,
    maxHits: 0,
    recentTrail: [],
  };
}

export function recordCellHit(
  state: HitTracerState,
  r: number,
  c: number
): HitTracerState {
  if (!Number.isInteger(r) || !Number.isInteger(c) || r < 0 || c < 0) {
    return state;
  }
  const key = hitCellKey(r, c);
  const nextCount = (state.counts[key] ?? 0) + 1;
  const counts = {
    ...state.counts,
    [key]: nextCount,
  };
  const nextTrail = [key, ...state.recentTrail.filter((k) => k !== key)].slice(
    0,
    HIT_TRAIL_MAX_LENGTH
  );

  return {
    counts,
    totalHits: state.totalHits + 1,
    maxHits: Math.max(state.maxHits, nextCount),
    recentTrail: nextTrail,
  };
}

export function getCellHitVisual(
  state: HitTracerState,
  r: number,
  c: number
): CellHitVisual {
  const key = hitCellKey(r, c);
  const count = state.counts[key] ?? 0;
  const intensity =
    count > 0 && state.maxHits > 0 ? Math.min(1, count / state.maxHits) : 0;
  const rankIdx = state.recentTrail.indexOf(key);
  return {
    count,
    intensity,
    trailRank: rankIdx >= 0 ? rankIdx : null,
  };
}
