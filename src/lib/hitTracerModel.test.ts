import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  HIT_TRAIL_MAX_LENGTH,
  createHitTracerState,
  getCellHitVisual,
  recordCellHit,
} from './hitTracerModel.ts';

test('hitTracerModel accumulates dwell hits and computes normalized intensity', () => {
  let state = createHitTracerState();
  state = recordCellHit(state, 2, 4);
  state = recordCellHit(state, 2, 4);
  state = recordCellHit(state, 3, 5);

  assert.equal(state.totalHits, 3);
  assert.equal(state.maxHits, 2);

  const hot = getCellHitVisual(state, 2, 4);
  assert.equal(hot.count, 2);
  assert.equal(hot.intensity, 1);
  assert.equal(hot.trailRank, 1);

  const warm = getCellHitVisual(state, 3, 5);
  assert.equal(warm.count, 1);
  assert.equal(warm.intensity, 0.5);
  assert.equal(warm.trailRank, 0);

  const cold = getCellHitVisual(state, 0, 0);
  assert.equal(cold.count, 0);
  assert.equal(cold.intensity, 0);
  assert.equal(cold.trailRank, null);
});

test('hitTracerModel caps recent trail at HIT_TRAIL_MAX_LENGTH and deduplicates', () => {
  let state = createHitTracerState();
  for (let i = 0; i < HIT_TRAIL_MAX_LENGTH + 5; i += 1) {
    state = recordCellHit(state, 0, i);
  }
  assert.equal(state.recentTrail.length, HIT_TRAIL_MAX_LENGTH);
  assert.equal(state.recentTrail[0], `0:${HIT_TRAIL_MAX_LENGTH + 4}`);

  state = recordCellHit(state, 0, 10);
  assert.equal(state.recentTrail[0], '0:10');
  assert.equal(state.recentTrail.filter((k) => k === '0:10').length, 1);
});

test('hitTracerModel ignores negative or non-integer coordinates cleanly', () => {
  const initial = createHitTracerState();
  assert.equal(recordCellHit(initial, -1, 2), initial);
  assert.equal(recordCellHit(initial, 1.5, 2), initial);
});
