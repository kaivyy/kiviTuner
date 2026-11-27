import test from 'node:test';
import assert from 'node:assert/strict';
import {
  a2lEntryToMapDef,
  blindTableToMapDef,
  damosEntryToMapDef,
  parseAddressInput,
  runAutoDetectChecksums,
  runBlindV2WithClassification,
  runFactorOffsetSolve,
  runImportA2l,
  runImportDamos,
  runImportXdfFull,
  runRegionInterpolation,
  runRepairChecksum,
  runSelectionEdit,
  runSimilaritySearch,
  runXdfPatchToggle,
} from './calibrationWorkbenchController.ts';
import type { BlindTable, MapClassification } from '../../lib/workbenchBridge.ts';

const sampleTable: BlindTable = {
  address: 0x4200,
  rows: 8,
  cols: 12,
  cell_bits: 16,
  little_endian: true,
  major_stride_bits: 0,
  minor_stride_bits: 0,
  raw_min: 100,
  raw_max: 3200,
  matrix: [[100]],
  x_axis: { address: 0x4100, points: 12, is_16bit: true, little_endian: true, values: [1000] },
  y_axis: null,
  suggested_scaling: {
    equation: 'X*0.001',
    unit: 'ms',
    decimals: 3,
    rationale: 'range match',
  },
  label: 'Blind_8x12_0x4200',
};

const sampleCls: MapClassification = {
  rule_id: 'fuel_pulse_16bit',
  group_name: 'Fuel',
  role_name: 'Suspected Fuel / Injection Map',
  category: 'Fuel & Injection',
  unit: 'ms',
  equation: 'X*0.001',
  factor: 0.001,
  offset: 0,
  decimals: 3,
  min_region: 'TopLeft',
  max_region: 'BottomRight',
  is_proposal: true,
  rationale: '16-bit surface',
};

test('parseAddressInput accepts hex and decimal integers cleanly', () => {
  assert.equal(parseAddressInput('0x80000'), 0x80000);
  assert.equal(parseAddressInput('16384'), 16384);
  assert.equal(parseAddressInput(''), null);
  assert.equal(parseAddressInput('-5'), null);
  assert.equal(parseAddressInput('0xZZ'), null);
});

test('runBlindV2WithClassification validates ROM, distinguishes no-backend, and pairs classifier hints', async () => {
  const noRom = await runBlindV2WithClassification(null, {
    includeCurves: true,
    include8bit: true,
    include32bit: true,
    maxTables: 64,
  });
  assert.equal(noRom.kind, 'error');

  const noBackend = await runBlindV2WithClassification(
    new Uint8Array(64),
    { includeCurves: true, include8bit: true, include32bit: true, maxTables: 64 },
    async () => null
  );
  assert.equal(noBackend.kind, 'no-backend');

  const ok = await runBlindV2WithClassification(
    new Uint8Array(256),
    { includeCurves: true, include8bit: true, include32bit: true, maxTables: 64 },
    async () => [sampleTable],
    async () => [sampleCls]
  );
  assert.equal(ok.kind, 'ok');
  if (ok.kind === 'ok') {
    assert.equal(ok.data.length, 1);
    const mapDef = blindTableToMapDef(ok.data[0], 0);
    assert.equal(mapDef.address, 0x4200);
    assert.equal(mapDef.dtype, 'u16');
    assert.equal(mapDef.endian, 'little');
    assert.match(mapDef.name, /Suspected Fuel/);
  }
});

test('definition importers apply base offset cleanly and surface errors on blank text', async () => {
  const emptyA2l = await runImportA2l('   ');
  assert.equal(emptyA2l.kind, 'error');

  const a2lMap = a2lEntryToMapDef(
    {
      name: 'kfzw',
      description: 'spark',
      kind: 'MAP',
      address: 0x812000,
      rows: 12,
      cols: 16,
      record_layout: 'Map_U16',
      conversion: 'CONV_1',
      unit: 'deg',
    },
    0x800000
  );
  assert.equal(a2lMap.address, 0x12000);
  assert.equal(a2lMap.scaling.unit, 'deg');

  const damosMap = damosEntryToMapDef(
    {
      source: 'SPZ_TYPE_1',
      name: 'FUEL_BASE',
      description: '',
      address: 0x42000,
      rows: 16,
      cols: 16,
      elem_bytes: 2,
      is_signed: true,
      is_2d_map: true,
      is_1d_curve: false,
    },
    0x40000
  );
  assert.equal(damosMap.address, 0x2000);
  assert.equal(damosMap.dtype, 'i16');

  const noBackendDamos = await runImportDamos('ECU TEST', async () => null);
  assert.equal(noBackendDamos.kind, 'no-backend');

  const okXdf = await runImportXdfFull('<XDFFORMAT/>', async () => ({
    maps: [],
    constants: [],
    flags: [],
    patches: [],
    checksums: [],
  }));
  assert.equal(okXdf.kind, 'ok');
});

test('similarity search requires target ROM only in cross-rom mode', async () => {
  const rom = new Uint8Array(128);
  const spec = {
    address: 0x10,
    rows: 4,
    cols: 4,
    cell_bits: 16,
    little_endian: true,
    is_signed: false,
  };
  const opts = {
    min_pearson_r: 0.9,
    max_nrmse: 0.2,
    max_rel_diff: null,
    stride_bytes: 0,
    exclude_overlapping_source: true,
    max_results: 32,
  };

  const missingTarget = await runSimilaritySearch('cross-rom', rom, null, spec, opts);
  assert.equal(missingTarget.kind, 'error');

  const inRomOk = await runSimilaritySearch('in-rom', rom, null, spec, opts, async () => [
    {
      address: 0x40,
      rows: 4,
      cols: 4,
      cell_bits: 16,
      little_endian: true,
      pearson_r: 0.99,
      nrmse: 0.01,
      mean_abs_diff: 2,
      max_rel_diff: 0.02,
      is_exact_copy: false,
    },
  ]);
  assert.equal(inRomOk.kind, 'ok');
  if (inRomOk.kind === 'ok') assert.equal(inRomOk.data[0].address, 0x40);
});

test('checksum detect, repair (Keep-CRC), patch toggle, matrix edit, interp, and solver validate and delegate', async () => {
  const rom = new Uint8Array(64);
  const detected = await runAutoDetectChecksums(rom, async () => []);
  assert.equal(detected.kind, 'ok');

  const repaired = await runRepairChecksum(rom, 'bosch_mg1', 'CompensateKeepCrc', async () => ({
    rom: Array.from(rom),
    match_info: {
      algorithm_id: 'bosch_mg1',
      algorithm_name: 'Bosch MG1',
      status: 'confirmed',
      notes: 'GF(2) keep-CRC ok',
      supports_keep_crc_mode: true,
      blocks: [],
      all_ok: true,
    },
  }));
  assert.equal(repaired.kind, 'ok');

  const patchErr = await runXdfPatchToggle(
    null,
    { id: '1', name: 'EWS', category: 'Immo', description: '', entries: [] },
    false
  );
  assert.equal(patchErr.kind, 'error');

  const noMatrix = await runSelectionEdit(
    null,
    { row_start: 0, row_end: 0, col_start: 0, col_end: 0 },
    { AddDelta: 1 }
  );
  assert.equal(noMatrix.kind, 'error');

  const interpOk = await runRegionInterpolation(
    [[1, 2], [3, 4]],
    { row_start: 0, row_end: 1, col_start: 0, col_end: 1 },
    { mode: 'Linear', axes: 'AllFourSides', allow_extrapolation: false, fallback_all_sides: true },
    async (m) => ({
      matrix: m,
      applied_mode: 'Linear',
      applied_axes: 'AllFourSides',
      used_extrapolation: false,
    })
  );
  assert.equal(interpOk.kind, 'ok');

  const nanSolver = await runFactorOffsetSolve([Number.NaN, 0], [100, 1], null, false);
  assert.equal(nanSolver.kind, 'error');

  const okSolver = await runFactorOffsetSolve([0, 0], [100, 10], null, false, async () => ({
    factor: 0.1,
    offset: 0,
    reciprocal: false,
    reciprocal_num: 0,
    reciprocal_den_offset: 0,
    equation: 'X*0.1',
  }));
  assert.equal(okSolver.kind, 'ok');
});

test('blindTableToMapDef maps signed, float32, and 1D curves cleanly', () => {
  const f32Curve = blindTableToMapDef(
    {
      table: {
        ...sampleTable,
        rows: 1,
        cols: 8,
        cell_bits: 32,
        is_float: true,
        is_signed: true,
        is_curve: true,
        detector_source: 'curve_v2',
      },
      classification: null,
    },
    0
  );
  assert.equal(f32Curve.dtype, 'f32');
  assert.equal(f32Curve.category, 'Blind 1D Curves');

  const signed16 = blindTableToMapDef(
    {
      table: {
        ...sampleTable,
        cell_bits: 16,
        is_signed: true,
        is_curve: false,
      },
      classification: null,
    },
    1
  );
  assert.equal(signed16.dtype, 'i16');
  assert.equal(signed16.category, 'Blind 2D Maps');

  const unsigned32 = blindTableToMapDef(
    {
      table: {
        ...sampleTable,
        cell_bits: 32,
        is_signed: false,
        is_float: false,
      },
      classification: null,
    },
    2
  );
  assert.equal(unsigned32.dtype, 'u32');
});
