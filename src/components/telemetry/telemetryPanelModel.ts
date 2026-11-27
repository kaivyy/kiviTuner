import type { RecordingState, TelemetrySourceType } from '../../types/telemetry.ts';

/**
 * Formats a duration or timestamp in milliseconds to mm:ss.t (minutes, seconds, tenths of a second).
 * Example: 14200 -> "00:14.2", 90000 -> "01:30.0"
 */
export function formatTimecode(ms: number): string {
  const clamped = Math.max(0, Math.floor(ms));
  const totalSeconds = Math.floor(clamped / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  const tenths = Math.floor((clamped % 1000) / 100);

  const mm = String(minutes).padStart(2, '0');
  const ss = String(seconds).padStart(2, '0');
  return `${mm}:${ss}.${tenths}`;
}

export interface ScrubberAriaAttributes {
  'aria-label': string;
  'aria-valuenow': number;
  'aria-valuemin': number;
  'aria-valuemax': number;
  'aria-valuetext': string;
}

/**
 * Builds WAI-ARIA compliant slider attributes for the playback scrubber.
 */
export function getScrubberAriaAttributes(
  currentTimeMs: number,
  totalDurationMs: number
): ScrubberAriaAttributes {
  const current = Math.max(0, Math.round(currentTimeMs));
  const total = Math.max(0, Math.round(totalDurationMs));
  return {
    'aria-label': 'Playback timeline scrubber',
    'aria-valuenow': current,
    'aria-valuemin': 0,
    'aria-valuemax': total,
    'aria-valuetext': `${formatTimecode(current)} / ${formatTimecode(total)}`,
  };
}

export interface DatalogToolbarStateParams {
  telemetrySource: TelemetrySourceType;
  isRecording: boolean;
  recordingState: RecordingState['state'];
  droppedCount: number;
  hasPlayback: boolean;
}

export interface DatalogToolbarStates {
  canRecord: boolean;
  canStop: boolean;
  canExport: boolean;
  canImport: boolean;
  showDataLossWarning: boolean;
}

/**
 * Determines enabled/disabled states for datalogger recording and transport controls.
 * Prevents invalid state transitions (e.g. exporting during recording or recording with inactive stream).
 */
export function getDatalogToolbarStates(
  params: DatalogToolbarStateParams
): DatalogToolbarStates {
  const canRecord = params.telemetrySource !== 'none' && !params.isRecording;
  const canStop = params.isRecording;
  const hasExportableData =
    params.recordingState === 'ReadyToSave' ||
    params.recordingState === 'Saved' ||
    params.recordingState === 'Interrupted' ||
    params.hasPlayback;
  const canExport = !params.isRecording && hasExportableData;
  const canImport = !params.isRecording;
  const showDataLossWarning = params.droppedCount > 0;

  return {
    canRecord,
    canStop,
    canExport,
    canImport,
    showDataLossWarning,
  };
}

export interface SourceBadgeInfo {
  label: string;
  color: 'emerald' | 'sky' | 'slate';
  isLive: boolean;
}

/**
 * Computes telemetry stream source badge presentation.
 */
export function getSourceBadgeInfo(source: TelemetrySourceType): SourceBadgeInfo {
  switch (source) {
    case 'live':
      return {
        label: 'LIVE 20 Hz',
        color: 'emerald',
        isLive: true,
      };
    case 'playback':
      return {
        label: 'PLAYBACK',
        color: 'sky',
        isLive: false,
      };
    case 'none':
    default:
      return {
        label: 'INACTIVE',
        color: 'slate',
        isLive: false,
      };
  }
}

export interface RecordingBadgeParams {
  isRecording: boolean;
  elapsedMs: number;
  sampleCount: number;
  droppedCount: number;
  state: RecordingState['state'];
}

export interface RecordingBadgeInfo {
  badge: 'recording' | 'interrupted' | 'ready' | 'none';
  label: string;
}

/**
 * Computes recording status badge text and visual level.
 */
export function getRecordingBadgeInfo(params: RecordingBadgeParams): RecordingBadgeInfo {
  if (params.isRecording) {
    return {
      badge: 'recording',
      label: `REC [${formatTimecode(params.elapsedMs)} | ${params.sampleCount} pts]`,
    };
  }

  if (params.state === 'Interrupted' || (params.droppedCount > 0 && params.sampleCount > 0)) {
    return {
      badge: 'interrupted',
      label: `INTERRUPTED (${params.sampleCount} pts)`,
    };
  }

  if (params.state === 'ReadyToSave' || params.state === 'Saved') {
    return {
      badge: 'ready',
      label: 'READY TO EXPORT',
    };
  }

  return {
    badge: 'none',
    label: '',
  };
}
