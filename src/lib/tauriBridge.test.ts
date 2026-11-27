import test from 'node:test';
import assert from 'node:assert/strict';
import {
  isDesktopApp,
  bridgeLoadBinData,
  bridgeParseXdf,
  bridgeExtractMapMatrix,
  bridgeUpdateMapMatrix,
  bridgeVerifyChecksum,
  bridgeFixChecksum,
  bridgeSaveBinData,
  bridgeCompareRomBinaries,
  bridgeVerifyRomAgainstReference,
  bridgePlanDifferentialFlash,
  bridgeDescribeFlashTarget,
  bridgeComputeChecksum,
  bridgeInterpolateMapAt,
  bridgeAnalyzeMapSmoothing,
  bridgeIngestFirmware,
  bridgeCompareRomBinariesWithAddress,
  bridgeScanRomHeuristics,
  bridgeScanRomCurves,
  bridgeScanDensoInterleaved,
  bridgeBlindAutomap,
  bridgeConvertDiffToMap,
  bridgeListDrivers,
  bridgeConnectEcu,
  bridgeDisconnectEcu,
  bridgeGetConnectionStatus,
  bridgeGetLatestTelemetry,
  bridgeSubscribeTelemetry,
  bridgeSubscribeEcuStatus,
  bridgeStartDatalogRecording,
  bridgeStopDatalogRecording,
  bridgeGetDatalogStatus,
  bridgeExportDatalogCsv,
  bridgeImportDatalogCsv,
  bridgeLoadDatalogPlayback,
  bridgeStartDatalogPlayback,
  bridgePauseDatalogPlayback,
  bridgeSeekDatalogPlayback,
  bridgeSetPlaybackSpeed,
  bridgeSetPlaybackLoop,
  bridgeStopDatalogPlayback,
  bridgeUnloadDatalogPlayback,
  bridgeGetPlaybackStatus,
  bridgeSubscribeDatalogStatus,
  bridgeSubscribePlaybackStatus,
  resetLocalDatalogState,
} from './tauriBridge.ts';
import type { MapDef } from '../types/kivi';

test('isDesktopApp: returns false in Node/browser test environment', () => {
  assert.strictEqual(isDesktopApp(), false);
});

test('bridgeLoadBinData: computes real RFC MD5/SHA-1/SHA-256 digests in browser mode', async () => {
  const data = new TextEncoder().encode('abc');
  const hashes = await bridgeLoadBinData(data);
  assert.strictEqual(hashes.md5, '900150983cd24fb0d6963f7d28e17f72');
  assert.strictEqual(hashes.sha1, 'a9993e364706816aba3e25717850c26c9cd0d89d');
  assert.strictEqual(
    hashes.sha256,
    'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
  );
});

test('bridgeParseXdf: returns fallback definitions in browser mode', async () => {
  const defs = await bridgeParseXdf('<XDF></XDF>', []);
  assert.deepStrictEqual(defs, []);
});

test('bridgeExtractMapMatrix and bridgeUpdateMapMatrix: fallback round-trips data', async () => {
  const rom = new Uint8Array(64);
  rom[10] = 50;
  rom[11] = 100;

  const mapDef: MapDef = {
    id: 'm1',
    name: 'Map 1',
    category: 'Fuel',
    address: 10,
    rows: 1,
    cols: 2,
    dtype: 'u8',
    endian: 'little',
    scaling: { equation: 'X', unit: 'ms', decimals: 0 },
  };

  const mat = await bridgeExtractMapMatrix(mapDef, rom);
  assert.deepStrictEqual(mat, [[50, 100]]);

  const updatedRom = await bridgeUpdateMapMatrix(mapDef, [[60, 120]], rom);
  assert.ok(updatedRom);
  assert.strictEqual(updatedRom[10], 60);
  assert.strictEqual(updatedRom[11], 120);
});

test('bridgeVerifyChecksum and bridgeFixChecksum: Keihin 8-bit checksum fallback', async () => {
  const rom = new Uint8Array(1024);
  rom.fill(1); // 1024 % 256 = 0, so valid
  const isValidInit = await bridgeVerifyChecksum(rom);
  assert.strictEqual(isValidInit, true);

  // Corrupt checksum
  rom[0] = 5;
  const isInvalid = await bridgeVerifyChecksum(rom);
  assert.strictEqual(isInvalid, false);

  // Fix checksum
  const fixRes = await bridgeFixChecksum(rom);
  assert.strictEqual(fixRes.ok, true);
  assert.ok(fixRes.data);

  // Verify repaired
  const isValidAfter = await bridgeVerifyChecksum(fixRes.data);
  assert.strictEqual(isValidAfter, true);
});

test('bridgeSaveBinData: preserves buffer', async () => {
  const rom = new Uint8Array([10, 20, 30]);
  const saved = await bridgeSaveBinData(rom);
  assert.deepStrictEqual(saved, rom);
});

test('bridgeCompareRomBinaries and bridgeConvertDiffToMap: fallback diff and convert', async () => {
  const base = new Uint8Array(128);
  const target = new Uint8Array(128);
  // Modify 16 contiguous bytes at 0x20
  for (let i = 0x20; i < 0x30; i++) {
    target[i] = 100;
  }

  const diffs = await bridgeCompareRomBinaries(base, target, 4);
  assert.strictEqual(diffs.length, 1);
  assert.strictEqual(diffs[0].start_addr, 0x20);
  assert.strictEqual(diffs[0].end_addr, 0x30);
  assert.strictEqual(diffs[0].byte_len, 16);
  assert.strictEqual(diffs[0].max_abs_delta, 100);

  const mapDef = await bridgeConvertDiffToMap(diffs[0]);
  assert.strictEqual(mapDef.address, 0x20);
  assert.strictEqual(mapDef.cols, 16);
  assert.ok(mapDef.name.includes('0x20'));
});

test('bridgeBlindAutomap: returns null outside the desktop app', async () => {
  const rom = new Uint8Array(256);
  const tables = await bridgeBlindAutomap(rom);
  assert.strictEqual(tables, null);
});

test('bridgeScanDensoInterleaved: returns null outside the desktop app', async () => {
  const rom = new Uint8Array(256);
  const maps = await bridgeScanDensoInterleaved(rom, [4, 8], 4, 16);
  assert.strictEqual(maps, null);
});

test('bridgeScanRomCurves: returns null outside the desktop app', async () => {
  const rom = new Uint8Array(256);
  const curves = await bridgeScanRomCurves(rom, 8, 32, false);
  assert.strictEqual(curves, null);
});

test('bridgeScanRomHeuristics: returns detected candidate tables in fallback mode', async () => {
  const rom = new Uint8Array(256);
  // Fill sample table values
  for (let i = 0x20; i < 0x80; i++) {
    rom[i] = (i * 3) % 255;
  }

  const candidates = await bridgeScanRomHeuristics(rom, 60);
  assert.ok(candidates.length > 0);
  assert.ok(candidates[0].confidence >= 60);
  assert.strictEqual(candidates[0].cols, 16);
});

test('telemetry bridge: lists zero simulated drivers in browser mode', async () => {
  resetLocalDatalogState();
  const drivers = await bridgeListDrivers();
  assert.ok(Array.isArray(drivers));
  assert.strictEqual(drivers.length, 0);
});

test('telemetry bridge: hardware connect and recording require desktop backend', async () => {
  resetLocalDatalogState();
  const initialStatus = await bridgeGetConnectionStatus();
  assert.strictEqual(initialStatus.state, 'Disconnected');
  assert.strictEqual(await bridgeGetLatestTelemetry(), null);

  await assert.rejects(async () => {
    await bridgeConnectEcu('obd2:/dev/ttyUSB0');
  }, /desktop application/);

  await assert.rejects(async () => {
    await bridgeStartDatalogRecording('test');
  }, /desktop application/);

  await assert.rejects(async () => {
    await bridgeStopDatalogRecording();
  }, /desktop application/);

  const statuses: string[] = [];
  const unsubStatus = bridgeSubscribeEcuStatus((state) => statuses.push(state.state));
  await bridgeDisconnectEcu();
  unsubStatus();
  assert.ok(statuses.includes('Disconnected'));
  const unsubDatalog = bridgeSubscribeDatalogStatus(() => {});
  unsubDatalog();
});

test('datalog bridge: CSV import and export roundtrip produces valid v1 schema without simulators', async () => {
  resetLocalDatalogState();
  const initStatus = await bridgeGetDatalogStatus();
  assert.strictEqual(initStatus.state.state, 'Idle');
  assert.strictEqual(initStatus.sample_count, 0);

  const rawCsv = [
    '# kiviTuner Datalog Session v1.0',
    '# session_id: ses-roundtrip-01',
    '# started_at: 2026-10-24T12:00:00.000Z',
    '# ended_at: 2026-10-24T12:00:00.100Z',
    '# ecu_id: BOSCH-EDC16',
    '# protocol: OBD2-CAN',
    '# sample_rate_hz: 20.0',
    '# total_samples: 2',
    '# dropped_samples: 0',
    '# notes: Roundtrip test',
    'timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags',
    '1000,0,1500.0,10.0,45.0,85.0,25.0,14.10,14.0,2.10,14.70,0.0,1,0x00',
    '1050,1,2200.0,25.0,65.0,85.0,25.0,14.10,18.0,3.10,14.10,20.0,2,0x00',
  ].join('\n');

  const imported = await bridgeImportDatalogCsv(rawCsv);
  assert.strictEqual(imported.session_id, 'ses-roundtrip-01');
  assert.strictEqual(imported.notes, 'Roundtrip test');
  assert.strictEqual(imported.total_samples_recorded, 2);
  assert.strictEqual(imported.ecu_identity?.ecu_id, 'BOSCH-EDC16');
  assert.strictEqual(imported.ecu_identity?.is_simulated, false);

  const exported = await bridgeExportDatalogCsv();
  assert.ok(exported.startsWith('# kiviTuner Datalog Session v1.0\n'));
  assert.ok(exported.includes('# ecu_id: BOSCH-EDC16'));
  assert.ok(exported.includes('# notes: Roundtrip test'));

  await assert.rejects(async () => {
    await bridgeImportDatalogCsv('# invalid header\n1,2,3');
  }, /Unsupported CSV schema version/);
});

test('datalog bridge: playback engine transport controls (play, pause, seek, speed, loop, stop, unload)', async () => {
  resetLocalDatalogState();

  const sampleCsv = [
    '# kiviTuner Datalog Session v1.0',
    '# session_id: ses-playback-test-01',
    '# started_at: 2026-10-24T12:00:00.000Z',
    '# ended_at: 2026-10-24T12:00:00.200Z',
    '# ecu_id: SIM-BENCH-01',
    '# protocol: VirtualBench',
    '# sample_rate_hz: 20.0',
    '# total_samples: 5',
    '# dropped_samples: 0',
    '# notes: Playback test',
    'timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags',
    '1000,0,1500.0,0.0,35.0,85.0,25.0,14.10,14.0,2.10,14.70,0.0,0,0x00',
    '1050,1,2000.0,10.0,40.0,85.0,25.0,14.10,16.0,2.50,14.20,10.0,1,0x00',
    '1100,2,3000.0,30.0,60.0,85.0,25.0,14.10,20.0,3.50,13.50,25.0,2,0x00',
    '1150,3,4500.0,60.0,80.0,86.0,26.0,14.10,24.0,5.00,12.80,45.0,2,0x00',
    '1200,4,6000.0,100.0,100.0,86.0,26.0,14.10,28.0,7.50,12.50,70.0,3,0x01',
  ].join('\n');

  const meta = await bridgeLoadDatalogPlayback(sampleCsv);
  assert.strictEqual(meta.session_id, 'ses-playback-test-01');
  assert.strictEqual(meta.total_samples_recorded, 5);

  const initialStatus = await bridgeGetPlaybackStatus();
  assert.strictEqual(initialStatus.state, 'Stopped');
  assert.strictEqual(initialStatus.current_time_ms, 1000);
  assert.strictEqual(initialStatus.total_duration_ms, 200);
  assert.strictEqual(initialStatus.total_samples, 5);
  assert.strictEqual(initialStatus.current_index, 0);

  // Playback subscription
  const playbackStatuses: string[] = [];
  const unsubPlaybackStatus = bridgeSubscribePlaybackStatus((status) => {
    playbackStatuses.push(status.state);
  });

  const framesEmitted: number[] = [];
  const unsubTelemetry = bridgeSubscribeTelemetry((frame) => {
    framesEmitted.push(frame.sequence);
  });

  // Start playback
  await bridgeStartDatalogPlayback();
  const playingStatus = await bridgeGetPlaybackStatus();
  assert.strictEqual(playingStatus.state, 'Playing');

  // Seek
  await bridgeSeekDatalogPlayback(1150);
  const seekStatus = await bridgeGetPlaybackStatus();
  assert.ok(
    Math.abs(seekStatus.current_time_ms - 1150) <= 5,
    `Seek time ${seekStatus.current_time_ms} close to 1150`
  );
  assert.strictEqual(seekStatus.current_index, 3);

  // Pause
  await bridgePauseDatalogPlayback();
  const pausedStatus = await bridgeGetPlaybackStatus();
  assert.strictEqual(pausedStatus.state, 'Paused');

  // Change speed & loop
  await bridgeSetPlaybackSpeed(2.0);
  const speedStatus = await bridgeGetPlaybackStatus();
  assert.strictEqual(speedStatus.speed, 2.0);

  await bridgeSetPlaybackLoop(true);
  const loopStatus = await bridgeGetPlaybackStatus();
  assert.strictEqual(loopStatus.loop_playback, true);

  // Stop playback
  await bridgeStopDatalogPlayback();
  const stoppedStatus = await bridgeGetPlaybackStatus();
  assert.strictEqual(stoppedStatus.state, 'Stopped');
  assert.strictEqual(stoppedStatus.current_index, 0);
  assert.strictEqual(stoppedStatus.current_time_ms, 1000);

  // Unload playback
  await bridgeUnloadDatalogPlayback();
  const unloadedStatus = await bridgeGetPlaybackStatus();
  assert.strictEqual(unloadedStatus.total_samples, 0);

  unsubPlaybackStatus();
  unsubTelemetry();
  assert.ok(playbackStatuses.includes('Playing'));
  assert.ok(playbackStatuses.includes('Paused'));
  assert.ok(playbackStatuses.includes('Stopped'));
});


test('bridgeVerifyRomAgainstReference: reports identical and differing buffers', async () => {
  const rom = new Uint8Array([0x10, 0x20, 0x30, 0x40]);

  const identical = await bridgeVerifyRomAgainstReference(rom, rom, 16);
  assert.ok(identical);
  assert.strictEqual(identical!.matches, true);
  assert.strictEqual(identical!.difference_count, 0);
  assert.strictEqual(identical!.active_len, 4);
  assert.strictEqual(identical!.reference_len, 4);
  assert.strictEqual(identical!.reference_is_blank, false);

  const reference = new Uint8Array([0x10, 0x20, 0x00, 0x40]);
  const differing = await bridgeVerifyRomAgainstReference(reference, rom, 16);
  assert.ok(differing);
  assert.strictEqual(differing!.matches, false);
  assert.strictEqual(differing!.difference_count, 1);
  assert.strictEqual(differing!.differences[0].offset, 2);
  assert.strictEqual(differing!.differences[0].expected_local_byte, 0x30);
  assert.strictEqual(differing!.differences[0].actual_chip_byte, 0x00);
});

test('bridgeVerifyRomAgainstReference: blank reference and difference cap', async () => {
  const rom = new Uint8Array([0xff, 0xff, 0xff, 0xff]);
  const blank = await bridgeVerifyRomAgainstReference(rom, rom, 16);
  assert.ok(blank);
  assert.strictEqual(blank!.matches, true);
  assert.strictEqual(blank!.reference_is_blank, true);

  const active = new Uint8Array([0x00, 0x00, 0x00, 0x00]);
  const capped = await bridgeVerifyRomAgainstReference(rom, active, 2);
  assert.ok(capped);
  assert.strictEqual(capped!.matches, false);
  assert.strictEqual(capped!.difference_count, 2);
});

test('bridgeVerifyRomAgainstReference: length mismatch yields synthetic difference', async () => {
  const active = new Uint8Array([0x01, 0x02]);
  const reference = new Uint8Array([0x01, 0x02, 0x03]);
  const report = await bridgeVerifyRomAgainstReference(reference, active, 16);
  assert.ok(report);
  assert.strictEqual(report!.matches, false);
  assert.strictEqual(report!.difference_count, 1);
  assert.strictEqual(report!.differences[0].offset, 2);
  assert.strictEqual(report!.active_len, 2);
  assert.strictEqual(report!.reference_len, 3);
});

test('bridgePlanDifferentialFlash: fallback sector plan matches Rust semantics', async () => {
  const stock = new Uint8Array(16).fill(0xaa);
  const tuned = new Uint8Array(stock);
  tuned[12] = 0xbb;

  const report = await bridgePlanDifferentialFlash(tuned, stock, [8, 8]);
  assert.ok(report);
  assert.strictEqual(report!.rom_len, 16);
  assert.strictEqual(report!.sectors.length, 2);
  assert.strictEqual(report!.sectors[0].is_modified, false);
  assert.strictEqual(report!.sectors[0].start_address, 0);
  assert.strictEqual(report!.sectors[1].is_modified, true);
  assert.strictEqual(report!.sectors[1].start_address, 8);
  assert.strictEqual(report!.sectors[1].modified_bytes_count, 1);
  assert.strictEqual(report!.modified_sector_count, 1);
  assert.strictEqual(report!.total_bytes_to_flash, 8);
});

test('bridgePlanDifferentialFlash: stops when sector list exceeds ROM', async () => {
  const rom = new Uint8Array([1, 2, 3, 4]);
  const report = await bridgePlanDifferentialFlash(rom, rom, [2, 2, 2, 2]);
  assert.ok(report);
  assert.strictEqual(report!.sectors.length, 2);
  assert.strictEqual(report!.total_bytes_to_flash, 0);
});

test('bridgePlanDifferentialFlash: length mismatch returns null', async () => {
  const report = await bridgePlanDifferentialFlash(
    new Uint8Array([1, 2, 3]),
    new Uint8Array([1, 2, 3, 4]),
    [4]
  );
  assert.strictEqual(report, null);
});

test('bridgeDescribeFlashTarget: fallback describes known and rejects unknown', async () => {
  const known = await bridgeDescribeFlashTarget(0x00894471);
  assert.ok(known);
  assert.strictEqual(known!.model_name, 'Intel 28F400B 512KiB');
  assert.strictEqual(known!.total_size_bytes, 524288);
  assert.strictEqual(known!.blocks.length, 7);
  assert.strictEqual(known!.boot_block_count, 1);
  assert.strictEqual(known!.write_safe_block_count, 6);

  const unknown = await bridgeDescribeFlashTarget(0xdeadbeef);
  assert.strictEqual(unknown, null);
});

test('bridgeComputeChecksum: oracle-derived vectors for all supported algorithms', async () => {
  const data = new Uint8Array([0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39]); // "123456789"

  const castagnoli = await bridgeComputeChecksum('crc32_castagnoli', data);
  assert.ok(castagnoli);
  assert.strictEqual(castagnoli!.width_bits, 32);
  assert.strictEqual(castagnoli!.value, 0xe3069283);
  assert.strictEqual(castagnoli!.input_len, 9);

  const ccitt = await bridgeComputeChecksum('crc16_ccitt', data);
  assert.ok(ccitt);
  assert.strictEqual(ccitt!.width_bits, 16);
  assert.strictEqual(ccitt!.value, 0x29b1);

  const simos = await bridgeComputeChecksum('crc32_simos', data);
  assert.ok(simos);
  assert.strictEqual(simos!.value, 0x89a1897f);

  const ieee = await bridgeComputeChecksum('crc32_ieee', data);
  assert.ok(ieee);
  assert.strictEqual(ieee!.value, 0xcbf43926);
});

test('bridgeComputeChecksum: empty input and unsupported algorithm', async () => {
  const emptyCastagnoli = await bridgeComputeChecksum('crc32_castagnoli', new Uint8Array(0));
  assert.ok(emptyCastagnoli);
  assert.strictEqual(emptyCastagnoli!.value, 0);

  const emptyCcitt = await bridgeComputeChecksum('crc16_ccitt', new Uint8Array(0));
  assert.ok(emptyCcitt);
  assert.strictEqual(emptyCcitt!.value, 0xffff);

  // crc16_modbus is not implemented in the source engine and must be rejected.
  const unsupported = await bridgeComputeChecksum('crc16_modbus', new Uint8Array([1]));
  assert.strictEqual(unsupported, null);
});

test('bridgeInterpolateMapAt: fallback bilinear lookup matches Rust semantics', async () => {
  // Index-space axes (parser currently discards LABEL values).
  const map = [
    [10, 15, 20, 25],
    [20, 25, 30, 35],
    [30, 35, 40, 45],
  ];
  const xAxis = [0, 1, 2, 3];
  const yAxis = [0, 1, 2];

  const exact = await bridgeInterpolateMapAt('map_4000', map, xAxis, yAxis, 1, 1);
  assert.ok(exact);
  assert.strictEqual(exact!.value, 25);

  const mid = await bridgeInterpolateMapAt('map_4000', map, xAxis, yAxis, 1.5, 1.5);
  assert.ok(mid);
  assert.strictEqual(mid!.value, 32.5);
  assert.strictEqual(mid!.x_t, 0.5);
  assert.strictEqual(mid!.y_t, 0.5);
});

test('bridgeInterpolateMapAt: clamps out-of-range coordinates', async () => {
  const map = [
    [10, 15],
    [20, 25],
  ];
  const xAxis = [0, 1];
  const yAxis = [0, 1];

  const below = await bridgeInterpolateMapAt('map_4000', map, xAxis, yAxis, -10, -10);
  assert.ok(below);
  assert.strictEqual(below!.value, 10);
  assert.strictEqual(below!.x_clamped, true);
  assert.strictEqual(below!.y_clamped, true);

  const above = await bridgeInterpolateMapAt('map_4000', map, xAxis, yAxis, 10, 10);
  assert.ok(above);
  assert.strictEqual(above!.value, 25);
});

test('bridgeInterpolateMapAt: rejects malformed input', async () => {
  // Row count does not match Y axis length.
  const bad = await bridgeInterpolateMapAt('m', [[1, 2]], [0, 1], [0, 1, 2], 0, 0);
  assert.strictEqual(bad, null);

  // Axis shorter than two points.
  const shortAxis = await bridgeInterpolateMapAt('m', [[1], [2]], [0], [0, 1], 0, 0);
  assert.strictEqual(shortAxis, null);
});

test('bridgeAnalyzeMapSmoothing: fallback reports laplacian spike and smoothed preview', async () => {
  const map = [
    [10, 10, 10],
    [10, 50, 10],
    [10, 10, 10],
  ];

  const r = await bridgeAnalyzeMapSmoothing('map_4000', map, 0.5);
  assert.ok(r);
  assert.strictEqual(r!.rows, 3);
  assert.strictEqual(r!.cols, 3);
  // Centre spike: 10+10+10+10 - 4*50 = -160
  assert.strictEqual(r!.laplacian[1][1], -160);
  assert.strictEqual(r!.max_abs_curvature, 160);
  assert.strictEqual(r!.max_curvature_row, 1);
  assert.strictEqual(r!.max_curvature_col, 1);
  // Preview centre: 0.5*50 + 0.5*10 = 30
  assert.strictEqual(r!.smoothed[1][1], 30);
  // Original preserved
  assert.strictEqual(r!.original[1][1], 50);
});

test('bridgeAnalyzeMapSmoothing: rejects invalid alpha and missing map', async () => {
  const map = [[1, 2], [3, 4], [5, 6]];
  assert.strictEqual(await bridgeAnalyzeMapSmoothing('m', map, 0), null);
  assert.strictEqual(await bridgeAnalyzeMapSmoothing('m', map, 1), null);
  assert.strictEqual(await bridgeAnalyzeMapSmoothing('m', null, 0.5), null);
});

test('bridgeAnalyzeMapSmoothing: does not mutate the supplied matrix', async () => {
  const map = [
    [10, 10, 10],
    [10, 50, 10],
    [10, 10, 10],
  ];
  const snapshot = JSON.stringify(map);
  await bridgeAnalyzeMapSmoothing('map_4000', map, 0.5);
  assert.strictEqual(JSON.stringify(map), snapshot);
});

test('bridgeIngestFirmware: parses Intel HEX into firmware bytes, not ASCII', async () => {
  const hexText = ':0400000001020304F2\n:00000001FF\n';
  const data = new TextEncoder().encode(hexText);

  const r = await bridgeIngestFirmware(data);
  assert.ok(r);
  assert.strictEqual(r!.format, 'IntelHex');
  assert.deepStrictEqual(Array.from(r!.bytes), [0x01, 0x02, 0x03, 0x04]);
  // Must NOT be the ASCII bytes of the file itself.
  assert.notDeepStrictEqual(Array.from(r!.bytes), Array.from(data));
});

test('bridgeIngestFirmware: parses S-Record', async () => {
  // S1, count=05, addr=0000, data=AABB, checksum
  const srec = 'S1050000AABB95\n';
  const data = new TextEncoder().encode(srec);
  const r = await bridgeIngestFirmware(data);
  assert.ok(r);
  assert.strictEqual(r!.format, 'SRecord');
  assert.deepStrictEqual(Array.from(r!.bytes), [0xaa, 0xbb]);
});

test('bridgeIngestFirmware: raw binary passes through unchanged', async () => {
  const bin = new Uint8Array([0x00, 0xff, 0x12, 0x34]);
  const r = await bridgeIngestFirmware(bin);
  assert.ok(r);
  assert.strictEqual(r!.format, 'RawBinary');
  assert.deepStrictEqual(Array.from(r!.bytes), [0x00, 0xff, 0x12, 0x34]);
});

test('bridgeIngestFirmware: malformed text fails instead of returning ASCII', async () => {
  const bad = new TextEncoder().encode(':ZZZZ\n');
  const r = await bridgeIngestFirmware(bad);
  assert.strictEqual(r, null);
});

test('bridgeCompareRomBinariesWithAddress: BIN-to-BIN still compares', async () => {
  const base = new Uint8Array([0x00, 0x11, 0x22, 0x33]);
  const target = new Uint8Array([0x00, 0x11, 0xff, 0x33]);

  const r = await bridgeCompareRomBinariesWithAddress(base, null, target, null, 4);
  assert.ok(r.ok);
  assert.strictEqual(r.ok!.compatibility, 'UnknownBases');
  assert.strictEqual(r.ok!.regions.length, 1);
});

test('bridgeCompareRomBinariesWithAddress: conflicting bases are rejected with a message', async () => {
  const a = new Uint8Array([1, 2, 3]);
  const b = new Uint8Array([1, 2, 3]);

  const r = await bridgeCompareRomBinariesWithAddress(a, 0x8000, b, 0x9000, 4);
  assert.strictEqual(r.ok, null);
  assert.ok(r.error);
  assert.ok(r.error!.includes('0x8000'), `error: ${r.error}`);
  assert.ok(r.error!.includes('0x9000'), `error: ${r.error}`);
});

test('bridgeCompareRomBinariesWithAddress: mixed known/unknown is rejected', async () => {
  const a = new Uint8Array([1, 2]);
  const b = new Uint8Array([1, 2]);

  const r = await bridgeCompareRomBinariesWithAddress(a, 0x8000, b, null, 4);
  assert.strictEqual(r.ok, null);
  assert.ok(r.error);
});

test('bridgeCompareRomBinariesWithAddress: matching bases are compared', async () => {
  const a = new Uint8Array([1, 2, 3]);
  const b = new Uint8Array([1, 0xff, 3]);

  const r = await bridgeCompareRomBinariesWithAddress(a, 0x8000, b, 0x8000, 4);
  assert.ok(r.ok);
  assert.ok(r.ok!.compatibility.startsWith('MatchingBases'));
  assert.strictEqual(r.ok!.regions.length, 1);
});

test('bridgeCompareRomBinariesWithAddress: empty input is rejected', async () => {
  const r = await bridgeCompareRomBinariesWithAddress(
    new Uint8Array([1]),
    null,
    new Uint8Array(0),
    null,
    4
  );
  assert.strictEqual(r.ok, null);
  assert.ok(r.error);
});
