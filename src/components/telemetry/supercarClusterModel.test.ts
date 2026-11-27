import test from 'node:test';
import assert from 'node:assert/strict';
import {
  SHIFT_LIGHT_COUNT,
  TACH_SEGMENT_COUNT,
  classifyAfr,
  computeAfrMarkerPercent,
  computeFlankArcPath,
  computeShiftLights,
  computeTachScaleLabels,
  computeTachSegments,
  formatGearLabel,
} from './supercarClusterModel.ts';

test('computeShiftLights handles zero, mid-rev progressive activation, and 10k redline alert', () => {
  const idle = computeShiftLights(900);
  assert.equal(idle.segments.length, SHIFT_LIGHT_COUNT);
  assert.equal(idle.segments.filter((s) => s.active).length, 0);
  assert.equal(idle.isShiftAlert, false);

  const mid = computeShiftLights(7000);
  const midActive = mid.segments.filter((s) => s.active).length;
  assert.ok(midActive >= 4 && midActive <= 8, `expected mid shift count, got ${midActive}`);
  assert.equal(mid.isShiftAlert, false);

  const red = computeShiftLights(11900);
  assert.equal(red.segments.filter((s) => s.active).length, SHIFT_LIGHT_COUNT);
  assert.equal(red.isShiftAlert, true);

  const invalid = computeShiftLights(Number.NaN);
  assert.equal(invalid.segments.filter((s) => s.active).length, 0);
  assert.equal(invalid.isShiftAlert, false);
});

test('computeTachSegments produces 36 calibrated segments with redline zone above 10,000 RPM', () => {
  const off = computeTachSegments(0);
  assert.equal(off.length, TACH_SEGMENT_COUNT);
  assert.equal(off.filter((s) => s.active).length, 0);

  const peak = computeTachSegments(10600);
  const activeCount = peak.filter((s) => s.active).length;
  assert.ok(activeCount >= 31 && activeCount <= 34);
  assert.equal(peak[35].zone, 'danger');
  assert.ok(peak[0].path.startsWith('M '));

  const labels = computeTachScaleLabels();
  assert.equal(labels.length, 7);
  assert.equal(labels[5].rpmThousands, 10);
  assert.equal(labels[5].isRedline, true);
});

test('computeFlankArcPath and AFR / Gear helpers format edge inputs safely', () => {
  assert.equal(computeFlankArcPath(0, 'left'), '');
  assert.ok(computeFlankArcPath(80, 'left').startsWith('M '));
  assert.ok(computeFlankArcPath(50, 'right').startsWith('M '));

  assert.equal(computeAfrMarkerPercent(14.7), 47);
  assert.equal(computeAfrMarkerPercent(5), 0);
  assert.equal(computeAfrMarkerPercent(25), 100);
  assert.equal(classifyAfr(12.8), 'rich');
  assert.equal(classifyAfr(14.7), 'stoich');
  assert.equal(classifyAfr(16.2), 'lean');

  assert.equal(formatGearLabel(0), 'N');
  assert.equal(formatGearLabel(3), '3');
  assert.equal(formatGearLabel(null), '--');
});
