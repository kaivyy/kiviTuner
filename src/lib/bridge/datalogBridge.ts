import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  DatalogRecorderStatus,
  PlaybackState,
  PlaybackStatus,
  SessionMetadata,
  TelemetryFrame,
} from '../../types/telemetry.ts';
import { parseDatalogSessionFromCsv, serializeDatalogSessionToCsv } from './datalogCsv.ts';
import { isDesktopApp } from './romBridge.ts';
import { clearLocalTelemetryState, emitLocalPlaybackFrame } from './telemetryBridge.ts';

interface LocalDatalogSession {
  metadata: SessionMetadata;
  frames: TelemetryFrame[];
}

const localState = {
  activeSession: null as LocalDatalogSession | null,
  playbackSession: null as LocalDatalogSession | null,
  playbackState: 'Stopped' as PlaybackState,
  playbackCurrentIndex: 0,
  playbackCurrentTimeMs: 0,
  playbackSpeed: 1.0,
  playbackLoop: false,
  playbackAnchorClockMs: 0,
  playbackAnchorVirtualMs: 0,
  playbackTimer: null as ReturnType<typeof setInterval> | null,
  datalogSubscribers: new Set<(status: DatalogRecorderStatus) => void>(),
  playbackSubscribers: new Set<(status: PlaybackStatus) => void>(),
};

function stopPlaybackTimer() {
  if (localState.playbackTimer) {
    clearInterval(localState.playbackTimer);
    localState.playbackTimer = null;
  }
}

function getLocalPlaybackStatus(): PlaybackStatus {
  const frames = localState.playbackSession?.frames ?? [];
  if (frames.length === 0) {
    return {
      state: 'Stopped',
      current_time_ms: 0,
      total_duration_ms: 0,
      current_index: 0,
      total_samples: 0,
      speed: localState.playbackSpeed,
      loop_playback: localState.playbackLoop,
    };
  }
  const firstTs = frames[0].timestamp_ms;
  const lastTs = frames[frames.length - 1].timestamp_ms;
  let currentTimeMs = localState.playbackCurrentTimeMs;
  let currentIndex = localState.playbackCurrentIndex;

  if (localState.playbackState === 'Playing') {
    const elapsedWall = Date.now() - localState.playbackAnchorClockMs;
    const rawVirtual = localState.playbackAnchorVirtualMs + elapsedWall * localState.playbackSpeed;
    if (rawVirtual >= lastTs) {
      if (localState.playbackLoop && lastTs > firstTs) {
        const span = lastTs - firstTs;
        currentTimeMs = firstTs + ((rawVirtual - firstTs) % span);
      } else {
        currentTimeMs = lastTs;
      }
    } else {
      currentTimeMs = Math.max(firstTs, rawVirtual);
    }
    let low = 0;
    let high = frames.length - 1;
    let idx = 0;
    while (low <= high) {
      const mid = Math.floor((low + high) / 2);
      if (frames[mid].timestamp_ms <= currentTimeMs) {
        idx = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    currentIndex = idx;
  }

  return {
    state: localState.playbackState,
    current_time_ms: Math.round(currentTimeMs),
    total_duration_ms: Math.max(0, lastTs - firstTs),
    current_index: currentIndex,
    total_samples: frames.length,
    speed: localState.playbackSpeed,
    loop_playback: localState.playbackLoop,
  };
}

function notifyPlaybackSubscribers() {
  const status = getLocalPlaybackStatus();
  for (const cb of localState.playbackSubscribers) {
    cb(status);
  }
}

function startPlaybackTimer() {
  stopPlaybackTimer();
  localState.playbackTimer = setInterval(() => {
    if (localState.playbackState !== 'Playing' || !localState.playbackSession) {
      stopPlaybackTimer();
      return;
    }
    const frames = localState.playbackSession.frames;
    const status = getLocalPlaybackStatus();
    localState.playbackCurrentTimeMs = status.current_time_ms;
    localState.playbackCurrentIndex = status.current_index;
    const frame = frames[status.current_index];
    if (frame) emitLocalPlaybackFrame(frame);

    const lastTs = frames[frames.length - 1].timestamp_ms;
    if (!localState.playbackLoop && status.current_time_ms >= lastTs) {
      localState.playbackState = 'Stopped';
      stopPlaybackTimer();
    }
    notifyPlaybackSubscribers();
  }, 50);
}

export function resetLocalDatalogState(): void {
  stopPlaybackTimer();
  localState.activeSession = null;
  localState.playbackSession = null;
  localState.playbackState = 'Stopped';
  localState.playbackCurrentIndex = 0;
  localState.playbackCurrentTimeMs = 0;
  localState.playbackSpeed = 1.0;
  localState.playbackLoop = false;
  localState.playbackAnchorClockMs = 0;
  localState.playbackAnchorVirtualMs = 0;
  clearLocalTelemetryState();
}

export async function bridgeStartDatalogRecording(notes?: string): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('start_datalog_recording', { notes: notes ?? null });
    return;
  }
  throw new Error('Live ECU datalog recording requires the kiviTuner desktop application.');
}

export async function bridgeStopDatalogRecording(): Promise<SessionMetadata> {
  if (isDesktopApp()) {
    return await invoke<SessionMetadata>('stop_datalog_recording');
  }
  throw new Error('Live ECU datalog recording requires the kiviTuner desktop application.');
}

export async function bridgeGetDatalogStatus(): Promise<DatalogRecorderStatus> {
  if (isDesktopApp()) {
    return await invoke<DatalogRecorderStatus>('get_datalog_status');
  }
  return {
    state: { state: 'Idle' },
    elapsed_ms: 0,
    sample_count: 0,
    dropped_count: 0,
  };
}

export async function bridgeExportDatalogCsv(): Promise<string> {
  if (isDesktopApp()) {
    return await invoke<string>('export_datalog_csv');
  }
  if (localState.activeSession) {
    return serializeDatalogSessionToCsv(localState.activeSession);
  }
  throw new Error('No datalog session available to export');
}

export async function bridgeImportDatalogCsv(csvContent: string): Promise<SessionMetadata> {
  if (isDesktopApp()) {
    return await invoke<SessionMetadata>('import_datalog_csv', { csvContent });
  }
  const parsed = parseDatalogSessionFromCsv(csvContent);
  localState.activeSession = parsed;
  return parsed.metadata;
}

export async function bridgeLoadDatalogPlayback(csvContent: string): Promise<SessionMetadata> {
  if (isDesktopApp()) {
    return await invoke<SessionMetadata>('load_datalog_playback', { csvContent });
  }
  stopPlaybackTimer();
  const parsed = parseDatalogSessionFromCsv(csvContent);
  if (parsed.frames.length === 0) {
    throw new Error('Cannot load playback for empty datalog session');
  }
  localState.playbackSession = parsed;
  localState.activeSession = parsed;
  localState.playbackState = 'Stopped';
  localState.playbackCurrentIndex = 0;
  localState.playbackCurrentTimeMs = parsed.frames[0].timestamp_ms;
  localState.playbackAnchorClockMs = Date.now();
  localState.playbackAnchorVirtualMs = localState.playbackCurrentTimeMs;
  notifyPlaybackSubscribers();
  return parsed.metadata;
}

export async function bridgeStartDatalogPlayback(): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('start_datalog_playback');
    return;
  }
  if (!localState.playbackSession || localState.playbackSession.frames.length === 0) {
    throw new Error('Cannot start playback on an empty session');
  }
  if (localState.playbackState === 'Playing') return;
  const now = Date.now();
  if (localState.playbackState === 'Stopped') {
    localState.playbackCurrentIndex = 0;
    localState.playbackCurrentTimeMs = localState.playbackSession.frames[0].timestamp_ms;
    emitLocalPlaybackFrame(localState.playbackSession.frames[0]);
  }
  localState.playbackAnchorClockMs = now;
  localState.playbackAnchorVirtualMs = localState.playbackCurrentTimeMs;
  localState.playbackState = 'Playing';
  startPlaybackTimer();
  notifyPlaybackSubscribers();
}

export async function bridgePauseDatalogPlayback(): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('pause_datalog_playback');
    return;
  }
  stopPlaybackTimer();
  if (localState.playbackState === 'Playing') {
    const status = getLocalPlaybackStatus();
    localState.playbackCurrentTimeMs = status.current_time_ms;
    localState.playbackCurrentIndex = status.current_index;
    localState.playbackAnchorVirtualMs = status.current_time_ms;
    localState.playbackAnchorClockMs = Date.now();
    localState.playbackState = 'Paused';
    notifyPlaybackSubscribers();
  }
}

export async function bridgeSeekDatalogPlayback(targetMs: number): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('seek_datalog_playback', { targetMs });
    return;
  }
  if (!localState.playbackSession || localState.playbackSession.frames.length === 0) return;
  const frames = localState.playbackSession.frames;
  const firstTs = frames[0].timestamp_ms;
  const lastTs = frames[frames.length - 1].timestamp_ms;
  const targetClamped = Math.max(firstTs, Math.min(lastTs, targetMs));

  let low = 0;
  let high = frames.length - 1;
  let idx = 0;
  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (frames[mid].timestamp_ms <= targetClamped) {
      idx = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  localState.playbackCurrentIndex = idx;
  localState.playbackCurrentTimeMs = targetClamped;
  localState.playbackAnchorClockMs = Date.now();
  localState.playbackAnchorVirtualMs = targetClamped;
  emitLocalPlaybackFrame(frames[idx]);
  notifyPlaybackSubscribers();
}

export async function bridgeSetPlaybackSpeed(speed: number): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('set_playback_speed', { speed });
    return;
  }
  const clamped = Math.max(0.1, Math.min(10.0, speed));
  if (localState.playbackState === 'Playing') {
    const status = getLocalPlaybackStatus();
    localState.playbackAnchorVirtualMs = status.current_time_ms;
    localState.playbackAnchorClockMs = Date.now();
    localState.playbackCurrentTimeMs = status.current_time_ms;
    localState.playbackCurrentIndex = status.current_index;
  }
  localState.playbackSpeed = clamped;
  notifyPlaybackSubscribers();
}

export async function bridgeSetPlaybackLoop(loopEnabled: boolean): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('set_playback_loop', { loopEnabled });
    return;
  }
  localState.playbackLoop = loopEnabled;
  notifyPlaybackSubscribers();
}

export async function bridgeStopDatalogPlayback(): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('stop_datalog_playback');
    return;
  }
  stopPlaybackTimer();
  localState.playbackState = 'Stopped';
  localState.playbackCurrentIndex = 0;
  localState.playbackCurrentTimeMs =
    localState.playbackSession && localState.playbackSession.frames.length > 0
      ? localState.playbackSession.frames[0].timestamp_ms
      : 0;
  localState.playbackAnchorClockMs = Date.now();
  localState.playbackAnchorVirtualMs = localState.playbackCurrentTimeMs;
  notifyPlaybackSubscribers();
}

export async function bridgeUnloadDatalogPlayback(): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('unload_datalog_playback');
    return;
  }
  stopPlaybackTimer();
  localState.playbackSession = null;
  localState.playbackState = 'Stopped';
  localState.playbackCurrentIndex = 0;
  localState.playbackCurrentTimeMs = 0;
  localState.playbackAnchorClockMs = Date.now();
  localState.playbackAnchorVirtualMs = 0;
  notifyPlaybackSubscribers();
}

export async function bridgeGetPlaybackStatus(): Promise<PlaybackStatus> {
  if (isDesktopApp()) {
    return await invoke<PlaybackStatus>('get_playback_status');
  }
  return getLocalPlaybackStatus();
}

export function bridgeSubscribeDatalogStatus(
  callback: (status: DatalogRecorderStatus) => void
): () => void {
  if (isDesktopApp()) {
    let unlistenFn: (() => void) | null = null;
    let unsubscribed = false;

    listen<DatalogRecorderStatus>('kivi://datalog-status', (event) => {
      callback(event.payload);
    })
      .then((unlisten) => {
        if (unsubscribed) unlisten();
        else unlistenFn = unlisten;
      })
      .catch((err) => console.warn('Failed to listen to kivi://datalog-status:', err));

    return () => {
      unsubscribed = true;
      if (unlistenFn) unlistenFn();
    };
  }

  localState.datalogSubscribers.add(callback);
  return () => {
    localState.datalogSubscribers.delete(callback);
  };
}

export function bridgeSubscribePlaybackStatus(
  callback: (status: PlaybackStatus) => void
): () => void {
  if (isDesktopApp()) {
    let unlistenFn: (() => void) | null = null;
    let unsubscribed = false;

    listen<PlaybackStatus>('kivi://playback-status', (event) => {
      callback(event.payload);
    })
      .then((unlisten) => {
        if (unsubscribed) unlisten();
        else unlistenFn = unlisten;
      })
      .catch((err) => console.warn('Failed to listen to kivi://playback-status:', err));

    return () => {
      unsubscribed = true;
      if (unlistenFn) unlistenFn();
    };
  }

  localState.playbackSubscribers.add(callback);
  return () => {
    localState.playbackSubscribers.delete(callback);
  };
}
