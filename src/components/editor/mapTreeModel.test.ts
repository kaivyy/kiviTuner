//! Tests for the sidebar map tree model.

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  UNCATEGORISED,
  classifyMapParamType,
  countMaps,
  countMapsByParamType,
  filterMapTree,
  filterMapTreeByParamType,
  groupMapsByCategory,
} from './mapTreeModel.ts';
import type { MapDef } from '../../types/kivi';

function map(name: string, category: string): MapDef {
  return {
    id: name,
    name,
    category,
    address: 0,
    rows: 1,
    cols: 1,
    dataType: 'u8',
    endianness: 'le',
    units: '',
    scaling: { kind: 'linear', factor: 1, offset: 0 },
  } as unknown as MapDef;
}

test('maps are grouped by their category', () => {
  const groups = groupMapsByCategory([
    map('Fuel Main', 'Fuel'),
    map('Ignition Main', 'Ignition'),
    map('Fuel Idle', 'Fuel'),
  ]);
  assert.equal(groups.length, 2);
  const fuel = groups.find((g) => g.category === 'Fuel');
  assert.equal(fuel?.maps.length, 2);
});

test('an empty category becomes Uncategorised', () => {
  const groups = groupMapsByCategory([map('Loose', ''), map('Blank', '   ')]);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].category, UNCATEGORISED);
  assert.equal(groups[0].maps.length, 2);
});

test('Uncategorised sorts last', () => {
  const groups = groupMapsByCategory([
    map('A', ''),
    map('B', 'Zebra'),
    map('C', 'Alpha'),
  ]);
  assert.deepEqual(
    groups.map((g) => g.category),
    ['Alpha', 'Zebra', UNCATEGORISED]
  );
});

test('categories sort alphabetically', () => {
  const groups = groupMapsByCategory([
    map('x', 'Zulu'),
    map('y', 'Alpha'),
    map('z', 'Mike'),
  ]);
  assert.deepEqual(
    groups.map((g) => g.category),
    ['Alpha', 'Mike', 'Zulu']
  );
});

test('maps sort by name within a category', () => {
  const groups = groupMapsByCategory([
    map('Charlie', 'F'),
    map('Alpha', 'F'),
    map('Bravo', 'F'),
  ]);
  assert.deepEqual(
    groups[0].maps.map((m) => m.name),
    ['Alpha', 'Bravo', 'Charlie']
  );
});

test('the input array is not mutated', () => {
  const input = [map('B', 'F'), map('A', 'F')];
  const before = input.map((m) => m.name);
  groupMapsByCategory(input);
  assert.deepEqual(input.map((m) => m.name), before);
});

test('an empty input yields no groups', () => {
  assert.deepEqual(groupMapsByCategory([]), []);
});

test('filtering matches names case-insensitively', () => {
  const groups = groupMapsByCategory([
    map('Fuel Main', 'Fuel'),
    map('Ignition Main', 'Ignition'),
  ]);
  const hits = filterMapTree(groups, 'FUEL');
  assert.equal(hits.length, 1);
  assert.equal(hits[0].maps.length, 1);
});

test('filtering drops groups with no match', () => {
  const groups = groupMapsByCategory([
    map('Fuel Main', 'Fuel'),
    map('Ignition Main', 'Ignition'),
  ]);
  const hits = filterMapTree(groups, 'zzz');
  assert.equal(hits.length, 0);
});

test('an empty query returns every group unchanged', () => {
  const groups = groupMapsByCategory([map('A', 'F'), map('B', 'I')]);
  assert.equal(filterMapTree(groups, '   ').length, 2);
});

test('the map count adds up across groups', () => {
  const groups = groupMapsByCategory([
    map('A', 'F'),
    map('B', 'F'),
    map('C', 'I'),
  ]);
  assert.equal(countMaps(groups), 3);
});

test('countMaps of an empty tree is zero', () => {
  assert.equal(countMaps([]), 0);
});

test('classifyMapParamType, countMapsByParamType, and filterMapTreeByParamType separate 2D, 1D, and 1x1 scalars', () => {
  const s = { ...map('RevLimit', 'Limiters'), rows: 1, cols: 1 };
  const c = { ...map('WOTSpark', 'Spark'), rows: 1, cols: 12 };
  const t = { ...map('MainFuel', 'Fuel'), rows: 16, cols: 16 };
  assert.equal(classifyMapParamType(s), 'scalar');
  assert.equal(classifyMapParamType(c), '1d');
  assert.equal(classifyMapParamType(t), '2d');

  const counts = countMapsByParamType([s, c, t]);
  assert.deepEqual(counts, { all: 3, '2d': 1, '1d': 1, scalar: 1 });

  const tree = groupMapsByCategory([s, c, t]);
  assert.equal(filterMapTreeByParamType(tree, 'scalar')[0].maps[0].name, 'RevLimit');
  assert.equal(filterMapTreeByParamType(tree, '1d')[0].maps[0].name, 'WOTSpark');
  assert.equal(filterMapTreeByParamType(tree, '2d')[0].maps[0].name, 'MainFuel');
});
