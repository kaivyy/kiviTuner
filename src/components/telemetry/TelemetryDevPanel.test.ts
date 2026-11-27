import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  formatTimecode,
  getScrubberAriaAttributes,
  getDatalogToolbarStates,
  getSourceBadgeInfo,
  getRecordingBadgeInfo,
} from './telemetryPanelModel.ts';

test('formatTimecode: correctly formats milliseconds to mm:ss.t', () => {
  assert.strictEqual(formatTimecode(0), '00:00.0');
  assert.strictEqual(formatTimecode(500), '00:00.5');
  assert.strictEqual(formatTimecode(14200), '00:14.2');
  assert.strictEqual(formatTimecode(90000), '01:30.0');
  assert.strictEqual(formatTimecode(125430), '02:05.4');
  assert.strictEqual(formatTimecode(3661200), '61:01.2');
  // Clamps negative inputs to 00:00.0
  assert.strictEqual(formatTimecode(-500), '00:00.0');
});

test('getScrubberAriaAttributes: builds complete WAI-ARIA slider accessibility attributes', () => {
  const attrs = getScrubberAriaAttributes(14200, 90000);
  assert.strictEqual(attrs['aria-label'], 'Playback timeline scrubber');
  assert.strictEqual(attrs['aria-valuenow'], 14200);
  assert.strictEqual(attrs['aria-valuemin'], 0);
  assert.strictEqual(attrs['aria-valuemax'], 90000);
  assert.strictEqual(attrs['aria-valuetext'], '00:14.2 / 01:30.0');

  // Handles zero duration edge case
  const zeroAttrs = getScrubberAriaAttributes(0, 0);
  assert.strictEqual(zeroAttrs['aria-valuenow'], 0);
  assert.strictEqual(zeroAttrs['aria-valuemin'], 0);
  assert.strictEqual(zeroAttrs['aria-valuemax'], 0);
  assert.strictEqual(zeroAttrs['aria-valuetext'], '00:00.0 / 00:00.0');
});

test('getDatalogToolbarStates: enforces invalid transition disabling', () => {
  // Case 1: Inactive source ('none') cannot start recording
  const inactiveState = getDatalogToolbarStates({
    telemetrySource: 'none',
    isRecording: false,
    recordingState: 'Idle',
    droppedCount: 0,
    hasPlayback: false,
  });
  assert.strictEqual(inactiveState.canRecord, false, 'Record disabled when source is none');
  assert.strictEqual(inactiveState.canStop, false, 'Stop disabled when not recording');
  assert.strictEqual(inactiveState.canExport, false, 'Export disabled when no session ready');
  assert.strictEqual(inactiveState.canImport, true, 'Import allowed when not recording');
  assert.strictEqual(inactiveState.showDataLossWarning, false);

  // Case 2: Live source active, idle -> can record
  const liveIdleState = getDatalogToolbarStates({
    telemetrySource: 'live',
    isRecording: false,
    recordingState: 'Idle',
    droppedCount: 0,
    hasPlayback: false,
  });
  assert.strictEqual(liveIdleState.canRecord, true, 'Record enabled when live and not recording');
  assert.strictEqual(liveIdleState.canStop, false);
  assert.strictEqual(liveIdleState.canExport, false);

  // Case 3: Actively recording -> Record and Export disabled, Stop enabled, Import disabled
  const recordingState = getDatalogToolbarStates({
    telemetrySource: 'live',
    isRecording: true,
    recordingState: 'Recording',
    droppedCount: 0,
    hasPlayback: false,
  });
  assert.strictEqual(recordingState.canRecord, false, 'Record disabled while already recording');
  assert.strictEqual(recordingState.canStop, true, 'Stop enabled while recording');
  assert.strictEqual(recordingState.canExport, false, 'Export disabled during recording');
  assert.strictEqual(recordingState.canImport, false, 'Import disabled during recording');

  // Case 4: Recording finished, ReadyToSave -> Export enabled, Stop disabled
  const readyToSaveState = getDatalogToolbarStates({
    telemetrySource: 'live',
    isRecording: false,
    recordingState: 'ReadyToSave',
    droppedCount: 0,
    hasPlayback: false,
  });
  assert.strictEqual(readyToSaveState.canRecord, true);
  assert.strictEqual(readyToSaveState.canStop, false);
  assert.strictEqual(readyToSaveState.canExport, true, 'Export enabled when ReadyToSave');

  // Case 5: Interrupted with dropped samples -> Data loss warning visible, export enabled
  const interruptedState = getDatalogToolbarStates({
    telemetrySource: 'live',
    isRecording: false,
    recordingState: 'Interrupted',
    droppedCount: 14,
    hasPlayback: false,
  });
  assert.strictEqual(interruptedState.canExport, true, 'Export enabled for interrupted session');
  assert.strictEqual(interruptedState.showDataLossWarning, true, 'Warning banner shown when dropped > 0');
});

test('getSourceBadgeInfo: provides correct badge semantics for live, playback, and none', () => {
  const live = getSourceBadgeInfo('live');
  assert.ok(live.label.includes('LIVE 20 Hz'));
  assert.strictEqual(live.isLive, true);

  const playback = getSourceBadgeInfo('playback');
  assert.ok(playback.label.includes('PLAYBACK'));
  assert.strictEqual(playback.isLive, false);

  const none = getSourceBadgeInfo('none');
  assert.ok(none.label.includes('INACTIVE'));
  assert.strictEqual(none.isLive, false);
});

test('getRecordingBadgeInfo: represents recording, interrupted, ready, and none states', () => {
  const recBadge = getRecordingBadgeInfo({
    isRecording: true,
    elapsedMs: 14200,
    sampleCount: 284,
    droppedCount: 0,
    state: 'Recording',
  });
  assert.strictEqual(recBadge.badge, 'recording');
  assert.ok(recBadge.label.includes('REC'));
  assert.ok(recBadge.label.includes('00:14.2'));
  assert.ok(recBadge.label.includes('284 pts'));

  const interruptedBadge = getRecordingBadgeInfo({
    isRecording: false,
    elapsedMs: 5000,
    sampleCount: 100,
    droppedCount: 5,
    state: 'Interrupted',
  });
  assert.strictEqual(interruptedBadge.badge, 'interrupted');
  assert.ok(interruptedBadge.label.includes('INTERRUPTED'));
  assert.ok(interruptedBadge.label.includes('100 pts'));

  const readyBadge = getRecordingBadgeInfo({
    isRecording: false,
    elapsedMs: 10000,
    sampleCount: 200,
    droppedCount: 0,
    state: 'ReadyToSave',
  });
  assert.strictEqual(readyBadge.badge, 'ready');
  assert.ok(readyBadge.label.includes('READY TO EXPORT'));

  const idleBadge = getRecordingBadgeInfo({
    isRecording: false,
    elapsedMs: 0,
    sampleCount: 0,
    droppedCount: 0,
    state: 'Idle',
  });
  assert.strictEqual(idleBadge.badge, 'none');
});
