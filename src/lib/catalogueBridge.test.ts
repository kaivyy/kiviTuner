//! Tests for the vehicle/ECU identity catalogue bridge.
//!
//! A browser has no catalogue backend, so every call returns `null`. The UI must
//! treat that as "not available here", never as "no results".

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  bridgeCatalogueStats,
  bridgeListEcuFamilies,
  bridgeListVehicleModels,
} from './catalogueBridge.ts';

test('stats return null in a browser', async () => {
  assert.equal(await bridgeCatalogueStats(), null);
});

test('an ECU search returns null in a browser', async () => {
  assert.equal(await bridgeListEcuFamilies('Bosch'), null);
});

test('a vehicle search returns null in a browser', async () => {
  assert.equal(await bridgeListVehicleModels('BMW'), null);
});

test('an empty query never throws', async () => {
  assert.equal(await bridgeListEcuFamilies(''), null);
});

test('a nonsense query never throws', async () => {
  assert.equal(await bridgeListVehicleModels('ZZZZ-NOT-A-VEHICLE'), null);
});
