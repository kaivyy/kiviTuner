//! Tests for the vehicle/ECU identity catalogue controller.
//!
//! The controller is where the "no backend" / "no results" / "results"
//! distinction is enforced. These tests pin that distinction and the minimum
//! query length.

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  MIN_QUERY_LEN,
  describeCatalogueScope,
  describeSuperCatalogueScope,
  parseHexUint,
  runCatalogueStats,
  runChipSearch,
  runEcuSearch,
  runFlasherCapabilitySearch,
  runJedecLookup,
  runReferenceModuleSearch,
  runSuperCatalogueStats,
  runVehicleSearch,
  summariseSearch,
} from './catalogueController.ts';

const stats = { ecuFamilyCount: 874, vehicleModelCount: 7311 };

test('stats report no-backend when the bridge returns null', async () => {
  const s = await runCatalogueStats(async () => null);
  assert.equal(s.kind, 'no-backend');
});

test('stats report ok with the counts', async () => {
  const s = await runCatalogueStats(async () => stats);
  assert.equal(s.kind, 'ok');
  if (s.kind === 'ok') {
    assert.equal(s.data.ecuFamilyCount, 874);
    assert.equal(s.data.vehicleModelCount, 7311);
  }
});

test('an ECU search below the minimum length is refused before the bridge', async () => {
  let called = false;
  const s = await runEcuSearch('b', async () => {
    called = true;
    return [];
  });
  assert.equal(s.kind, 'error');
  assert.equal(called, false);
});

test('a vehicle search below the minimum length is refused before the bridge', async () => {
  let called = false;
  const s = await runVehicleSearch('b', async () => {
    called = true;
    return [];
  });
  assert.equal(called, false);
  assert.equal(s.kind, 'error');
});

test('the minimum length is at least two', () => {
  assert.ok(MIN_QUERY_LEN >= 2);
});

test('a valid ECU search reports ok', async () => {
  const s = await runEcuSearch('BMS46', async () => [
    { category: 'Engine', manufacturer: 'Bosch', family: 'BMS', variant: 'BMS46' },
  ]);
  assert.equal(s.kind, 'ok');
  if (s.kind === 'ok') assert.equal(s.data.length, 1);
});

test('an ECU search with no matches reports ok with an empty list, not no-backend', async () => {
  const s = await runEcuSearch('ZZZZ', async () => []);
  assert.equal(s.kind, 'ok');
  if (s.kind === 'ok') assert.equal(s.data.length, 0);
});

test('an ECU search reports no-backend distinctly from an empty result', async () => {
  const s = await runEcuSearch('Bosch', async () => null);
  assert.equal(s.kind, 'no-backend');
});

test('a valid vehicle search reports ok', async () => {
  const s = await runVehicleSearch('BMW', async () => [
    { class: 'PKW', make: 'BMW', model: 'E46', variant: '', engine: '3.0L' },
  ]);
  assert.equal(s.kind, 'ok');
});

test('the scope description names the counts and says it is not a capability list', () => {
  const text = describeCatalogueScope(stats);
  assert.match(text, /874/);
  assert.match(text, /7311/);
  assert.match(text, /not what can be flashed/i);
});

test('an empty result is summarised as no match', () => {
  assert.match(summariseSearch(0, 'ecu'), /no matching/i);
  assert.match(summariseSearch(0, 'vehicle'), /no matching/i);
});

test('a non-empty result reports the count', () => {
  assert.match(summariseSearch(12, 'ecu'), /12/);
  assert.match(summariseSearch(7, 'vehicle'), /7/);
});

const superStats = {
  ecu_family_count: 874,
  vehicle_model_count: 7311,
  chip_catalogue_count: 6742,
  flasher_capability_count: 726,
  reference_module_count: 99,
  total_entries: 15752,
};

test('super-catalogue stats handle no-backend and ok with all 15,752 entries', async () => {
  const missing = await runSuperCatalogueStats(async () => null);
  assert.equal(missing.kind, 'no-backend');

  const ok = await runSuperCatalogueStats(async () => superStats);
  assert.equal(ok.kind, 'ok');
  if (ok.kind === 'ok') {
    assert.equal(ok.data.total_entries, 15752);
    const scope = describeSuperCatalogueScope(ok.data);
    assert.match(scope, /15,752/);
    assert.match(scope, /6742 chips/);
    assert.match(scope, /726 reference capabilities/);
  }
});

test('chip search validates min length and returns matching chips', async () => {
  const short = await runChipSearch('2', async () => []);
  assert.equal(short.kind, 'error');

  const ok = await runChipSearch('29F400', async () => [
    {
      manufacturer: 'AMD',
      chip_name: 'AM29F400BB',
      package: 'PSOP44',
      programmer_id: 0x22ab,
      jedec_ids: [[0x01, 0x22ab]],
    },
  ]);
  assert.equal(ok.kind, 'ok');
  if (ok.kind === 'ok') {
    assert.equal(ok.data[0].chip_name, 'AM29F400BB');
  }
});

test('parseHexUint and JEDEC lookup validate hex before calling bridge', () => {
  assert.equal(parseHexUint('EF'), 0xef);
  assert.equal(parseHexUint('0x4018'), 0x4018);
  assert.equal(parseHexUint(''), null);
  assert.equal(parseHexUint('ZZ'), null);
});

test('JEDEC lookup distinguishes validation error, no-backend, and matches', async () => {
  let called = false;
  const bad = await runJedecLookup('GG', '4018', async () => {
    called = true;
    return [];
  });
  assert.equal(bad.kind, 'error');
  assert.equal(called, false);

  const missing = await runJedecLookup('EF', '4018', async () => null);
  assert.equal(missing.kind, 'no-backend');

  const ok = await runJedecLookup('0xEF', '0x4018', async (mfr, dev) => {
    assert.equal(mfr, 0xef);
    assert.equal(dev, 0x4018);
    return [
      {
        manufacturer: 'Winbond',
        chip_name: 'W25Q128FV',
        package: 'SOIC8',
        programmer_id: null,
        jedec_ids: [[0xef, 0x4018]],
      },
    ];
  });
  assert.equal(ok.kind, 'ok');
});

test('flasher capabilities and reference modules enforce search contract', async () => {
  const capNoBackend = await runFlasherCapabilitySearch('Honda', async () => null);
  assert.equal(capNoBackend.kind, 'no-backend');

  const modOk = await runReferenceModuleSearch('Bosch', async () => [
    {
      file_index: 71,
      vendor_number: 71,
      coverage: 'Bosch MEDC17 Bench',
      file_size: 409600,
      encrypted_blobs: 3,
    },
  ]);
  assert.equal(modOk.kind, 'ok');
  assert.match(summariseSearch(2, 'module'), /2 reference modules/);
  assert.match(summariseSearch(0, 'capability'), /No matching capability entry/);
});
