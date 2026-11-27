import test from 'node:test';
import assert from 'node:assert/strict';
import { useTelemetryStore } from './useTelemetryStore.ts';
import { resetLocalDatalogState } from '../lib/tauriBridge.ts';

test.beforeEach(() => {
  resetLocalDatalogState();
  useTelemetryStore.getState().reset();
});

test('useTelemetryStore: browser mode reports zero simulated drivers', async () => {
  const store = useTelemetryStore.getState();
  const drivers = await store.fetchDrivers();
  assert.strictEqual(drivers.length, 0);
  assert.strictEqual(useTelemetryStore.getState().availableDrivers.length, 0);
  assert.strictEqual(useTelemetryStore.getState().selectedDriverId, '');
});

test('useTelemetryStore: panel toggle switches visibility state', () => {
  const store = useTelemetryStore.getState();
  assert.strictEqual(store.isPanelOpen, false);

  store.togglePanel();
  assert.strictEqual(useTelemetryStore.getState().isPanelOpen, true);

  store.togglePanel(false);
  assert.strictEqual(useTelemetryStore.getState().isPanelOpen, false);

  store.togglePanel(true);
  assert.strictEqual(useTelemetryStore.getState().isPanelOpen, true);
});

test('useTelemetryStore: connecting without desktop hardware backend faults honestly', async () => {
  const store = useTelemetryStore.getState();
  assert.strictEqual(store.connectionState.state, 'Disconnected');

  await assert.rejects(async () => {
    await store.connect('obd2:/dev/ttyUSB0');
  }, /desktop application/);

  const state = useTelemetryStore.getState();
  assert.strictEqual(state.connectionState.state, 'Faulted');
  assert.strictEqual(state.identity, null);

  await store.disconnect();
  assert.strictEqual(useTelemetryStore.getState().connectionState.state, 'Disconnected');
});

test('useTelemetryStore: datalog CSV import and playback transport actions', async () => {
  const sampleCsv = [
    '# kiviTuner Datalog Session v1.0',
    '# session_id: ses-store-playback-01',
    '# started_at: 2026-10-24T12:00:00.000Z',
    '# ended_at: 2026-10-24T12:00:00.150Z',
    '# ecu_id: KEIHIN-K56',
    '# protocol: OBD2-Serial',
    '# sample_rate_hz: 20.0',
    '# total_samples: 4',
    '# dropped_samples: 0',
    '# notes: Store Playback Test',
    'timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags',
    '5000,0,1000.0,0.0,30.0,80.0,20.0,14.00,10.0,2.00,14.70,0.0,0,0x00',
    '5050,1,2000.0,20.0,50.0,80.0,20.0,14.00,15.0,3.00,14.00,15.0,1,0x00',
    '5100,2,3000.0,40.0,70.0,81.0,21.0,14.00,20.0,4.50,13.20,35.0,2,0x00',
    '5150,3,4000.0,80.0,90.0,81.0,21.0,14.00,25.0,6.00,12.80,60.0,3,0x00',
  ].join('\n');

  const store = useTelemetryStore.getState();
  const session = await store.importDatalogCsv(sampleCsv);

  assert.strictEqual(session.session_id, 'ses-store-playback-01');
  assert.strictEqual(session.ecu_identity?.is_simulated, false);

  const loadedState = useTelemetryStore.getState();
  assert.strictEqual(loadedState.telemetrySource, 'playback');
  assert.ok(loadedState.playbackSession !== null);
  assert.strictEqual(loadedState.playbackStatus?.state, 'Stopped');

  await store.startPlayback();
  assert.strictEqual(useTelemetryStore.getState().isPlaybackActive, true);
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.state, 'Playing');

  await store.pausePlayback();
  assert.strictEqual(useTelemetryStore.getState().isPlaybackActive, false);
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.state, 'Paused');

  await store.seekPlayback(5100);
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.current_time_ms, 5100);
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.current_index, 2);

  await store.setPlaybackSpeed(1.5);
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.speed, 1.5);

  await store.setPlaybackLoop(true);
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.loop_playback, true);

  await store.stopPlayback();
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.state, 'Stopped');
  assert.strictEqual(useTelemetryStore.getState().playbackStatus?.current_index, 0);

  await store.unloadPlayback();
  const unloaded = useTelemetryStore.getState();
  assert.strictEqual(unloaded.playbackStatus, null);
  assert.strictEqual(unloaded.playbackSession, null);
  assert.strictEqual(unloaded.telemetrySource, 'none');
});

test('useTelemetryStore: datalog CSV import rejects invalid non-numeric column data', async () => {
  const store = useTelemetryStore.getState();
  const invalidCsv = [
    '# kiviTuner Datalog Session v1.0',
    '# session_id: ses-invalid-numeric',
    'timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags',
    '5000,0,1000.0,BAD_TPS,30.0,80.0,20.0,14.00,10.0,2.00,14.70,0.0,0,0x00',
  ].join('\n');

  await assert.rejects(async () => {
    await store.importDatalogCsv(invalidCsv);
  }, /invalid numeric data for tps/);
});
