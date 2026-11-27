import { create } from 'zustand';
import type {
  ConnectionState,
  EcuIdentity,
  TelemetryFrame,
  DriverDescriptor,
  SessionMetadata,
  DatalogRecorderStatus,
  PlaybackStatus,
  TelemetrySourceType,
} from '../types/telemetry.ts';
import {
  bridgeListDrivers,
  bridgeConnectEcu,
  bridgeDisconnectEcu,
  bridgeSubscribeTelemetry,
  bridgeSubscribeEcuStatus,
  bridgeStartDatalogRecording,
  bridgeStopDatalogRecording,
  bridgeGetDatalogStatus,
  bridgeExportDatalogCsv,
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
} from '../lib/tauriBridge.ts';

export interface TelemetryState {
  connectionState: ConnectionState;
  identity: EcuIdentity | null;
  latestFrame: TelemetryFrame | null;
  selectedDriverId: string;
  isPanelOpen: boolean;
  history: TelemetryFrame[];
  availableDrivers: DriverDescriptor[];

  // Datalogger recording & playback state slices
  telemetrySource: TelemetrySourceType;
  sourceEpoch: number;
  recordingStatus: DatalogRecorderStatus;
  playbackStatus: PlaybackStatus | null;
  playbackSession: SessionMetadata | null;
  isRecording: boolean;
  isPlaybackActive: boolean;

  // Actions
  fetchDrivers: () => Promise<DriverDescriptor[]>;
  selectDriver: (driverId: string) => void;
  connect: (driverId?: string) => Promise<void>;
  disconnect: () => Promise<void>;
  togglePanel: (open?: boolean) => void;
  clearHistory: () => void;
  reset: () => void;

  startRecording: (notes?: string) => Promise<void>;
  stopRecording: () => Promise<SessionMetadata | null>;
  exportDatalogCsv: () => Promise<string>;
  importDatalogCsv: (csvContent: string) => Promise<SessionMetadata>;
  startPlayback: () => Promise<void>;
  pausePlayback: () => Promise<void>;
  seekPlayback: (targetMs: number) => Promise<void>;
  setPlaybackSpeed: (speed: number) => Promise<void>;
  setPlaybackLoop: (loopEnabled: boolean) => Promise<void>;
  stopPlayback: () => Promise<void>;
  unloadPlayback: () => Promise<void>;
  setTelemetrySource: (source: TelemetrySourceType) => void;
}

let unlistenTelemetry: (() => void) | null = null;
let unlistenStatus: (() => void) | null = null;
let unlistenDatalogStatus: (() => void) | null = null;
let unlistenPlaybackStatus: (() => void) | null = null;
let sourceEpoch = 0;

function bumpEpoch(): number {
  sourceEpoch++;
  try {
    useTelemetryStore.setState({ sourceEpoch });
  } catch {
    // If called before store initialization
  }
  return sourceEpoch;
}

function cleanupSubscriptions() {
  bumpEpoch();
  if (unlistenTelemetry) {
    unlistenTelemetry();
    unlistenTelemetry = null;
  }
  if (unlistenStatus) {
    unlistenStatus();
    unlistenStatus = null;
  }
  if (unlistenDatalogStatus) {
    unlistenDatalogStatus();
    unlistenDatalogStatus = null;
  }
  if (unlistenPlaybackStatus) {
    unlistenPlaybackStatus();
    unlistenPlaybackStatus = null;
  }
}

function subscribeLiveTelemetry() {
  if (unlistenTelemetry) {
    unlistenTelemetry();
    unlistenTelemetry = null;
  }
  const currentEpoch = sourceEpoch;
  unlistenTelemetry = bridgeSubscribeTelemetry((frame) => {
    if (currentEpoch !== sourceEpoch) return;
    if (useTelemetryStore.getState().telemetrySource !== 'live') return;
    useTelemetryStore.setState((state) => {
      const nextHistory =
        state.history.length >= 100
          ? [...state.history.slice(state.history.length - 99), frame]
          : [...state.history, frame];
      return {
        latestFrame: frame,
        history: nextHistory,
      };
    });
  });
}

function subscribePlaybackTelemetry() {
  if (unlistenTelemetry) {
    unlistenTelemetry();
    unlistenTelemetry = null;
  }
  const currentEpoch = sourceEpoch;
  unlistenTelemetry = bridgeSubscribeTelemetry((frame) => {
    if (currentEpoch !== sourceEpoch) return;
    if (useTelemetryStore.getState().telemetrySource !== 'playback') return;
    useTelemetryStore.setState((state) => {
      const nextHistory =
        state.history.length >= 100
          ? [...state.history.slice(state.history.length - 99), frame]
          : [...state.history, frame];
      return {
        latestFrame: frame,
        history: nextHistory,
      };
    });
  });
}

const initialRecordingStatus: DatalogRecorderStatus = {
  state: { state: 'Idle' },
  elapsed_ms: 0,
  sample_count: 0,
  dropped_count: 0,
};

const initialState = {
  connectionState: { state: 'Disconnected' } as ConnectionState,
  identity: null as EcuIdentity | null,
  latestFrame: null as TelemetryFrame | null,
  selectedDriverId: '',
  isPanelOpen: false,
  history: [] as TelemetryFrame[],
  availableDrivers: [] as DriverDescriptor[],
  telemetrySource: 'none' as TelemetrySourceType,
  sourceEpoch: 0,
  recordingStatus: initialRecordingStatus,
  playbackStatus: null as PlaybackStatus | null,
  playbackSession: null as SessionMetadata | null,
  isRecording: false,
  isPlaybackActive: false,
};

export const useTelemetryStore = create<TelemetryState>((set, get) => ({
  ...initialState,

  fetchDrivers: async () => {
    try {
      const drivers = await bridgeListDrivers();
      set({ availableDrivers: drivers });
      if (drivers.length > 0 && !get().selectedDriverId) {
        set({ selectedDriverId: drivers[0].id });
      }
      return drivers;
    } catch (err) {
      console.warn('Failed to fetch ECU drivers:', err);
      return [];
    }
  },

  selectDriver: (driverId: string) => {
    set({ selectedDriverId: driverId });
  },

  connect: async (driverId?: string) => {
    cleanupSubscriptions();
    const currentEpoch = bumpEpoch();

    const targetDriver = driverId || get().selectedDriverId;
    if (!targetDriver) {
      const message = 'No hardware ECU driver selected.';
      set({
        identity: null,
        telemetrySource: 'none',
        connectionState: { state: 'Faulted', details: { error: message } },
      });
      throw new Error(message);
    }
    set({
      selectedDriverId: targetDriver,
      telemetrySource: 'live',
      connectionState: { state: 'Connecting', details: { attempt: 1 } },
    });

    // Wire up status listener
    unlistenStatus = bridgeSubscribeEcuStatus((status) => {
      set((state) => {
        let identity = state.identity;
        if (status.state === 'Connected') {
          identity = status.details.identity;
        } else if (status.state === 'Disconnected') {
          identity = null;
        }
        return { connectionState: status, identity };
      });
    });

    // Wire up telemetry frame listener (20 Hz high-frequency pipeline)
    subscribeLiveTelemetry();

    // Wire up datalog recorder status listener
    unlistenDatalogStatus = bridgeSubscribeDatalogStatus((status) => {
      set({
        recordingStatus: status,
        isRecording: status.state.state === 'Recording',
      });
    });

    try {
      const identity = await bridgeConnectEcu(targetDriver);
      if (currentEpoch !== sourceEpoch) return;
      set({
        identity,
        connectionState: { state: 'Connected', details: { identity } },
      });
    } catch (err) {
      cleanupSubscriptions();
      const message = err instanceof Error ? err.message : String(err);
      set({
        identity: null,
        telemetrySource: 'none',
        connectionState: { state: 'Faulted', details: { error: message } },
      });
      throw err;
    }
  },

  disconnect: async () => {
    cleanupSubscriptions();
    try {
      await bridgeDisconnectEcu();
    } finally {
      set({
        connectionState: { state: 'Disconnected' },
        identity: null,
        telemetrySource: 'none',
        isRecording: false,
      });
    }
  },

  togglePanel: (open?: boolean) => {
    set((state) => ({
      isPanelOpen: open !== undefined ? open : !state.isPanelOpen,
    }));
  },

  clearHistory: () => {
    set({ history: [] });
  },

  reset: () => {
    cleanupSubscriptions();
    set({ ...initialState });
  },

  startRecording: async (notes?: string) => {
    await bridgeStartDatalogRecording(notes);
    const status = await bridgeGetDatalogStatus();
    set({
      recordingStatus: status,
      isRecording: status.state.state === 'Recording',
    });
  },

  stopRecording: async () => {
    const session = await bridgeStopDatalogRecording();
    const status = await bridgeGetDatalogStatus();
    set({
      recordingStatus: status,
      isRecording: false,
    });
    return session;
  },

  exportDatalogCsv: async () => {
    return await bridgeExportDatalogCsv();
  },

  importDatalogCsv: async (csvContent: string) => {
    if (unlistenPlaybackStatus) {
      unlistenPlaybackStatus();
      unlistenPlaybackStatus = null;
    }
    const currentEpoch = bumpEpoch();

    const metadata = await bridgeLoadDatalogPlayback(csvContent);
    const status = await bridgeGetPlaybackStatus();

    unlistenPlaybackStatus = bridgeSubscribePlaybackStatus((newStatus) => {
      if (currentEpoch !== sourceEpoch) return;
      set({
        playbackStatus: newStatus,
        isPlaybackActive: newStatus.state === 'Playing',
      });
    });

    subscribePlaybackTelemetry();

    set({
      telemetrySource: 'playback',
      playbackSession: metadata,
      playbackStatus: status,
      isPlaybackActive: false,
      history: [],
    });

    return metadata;
  },

  startPlayback: async () => {
    await bridgeStartDatalogPlayback();
    const status = await bridgeGetPlaybackStatus();
    set({
      playbackStatus: status,
      isPlaybackActive: true,
      telemetrySource: 'playback',
    });
  },

  pausePlayback: async () => {
    await bridgePauseDatalogPlayback();
    const status = await bridgeGetPlaybackStatus();
    set({
      playbackStatus: status,
      isPlaybackActive: false,
    });
  },

  seekPlayback: async (targetMs: number) => {
    await bridgeSeekDatalogPlayback(targetMs);
    const status = await bridgeGetPlaybackStatus();
    set({ playbackStatus: status });
  },

  setPlaybackSpeed: async (speed: number) => {
    await bridgeSetPlaybackSpeed(speed);
    const status = await bridgeGetPlaybackStatus();
    set({ playbackStatus: status });
  },

  setPlaybackLoop: async (loopEnabled: boolean) => {
    await bridgeSetPlaybackLoop(loopEnabled);
    const status = await bridgeGetPlaybackStatus();
    set({ playbackStatus: status });
  },

  stopPlayback: async () => {
    await bridgeStopDatalogPlayback();
    const status = await bridgeGetPlaybackStatus();
    set({
      playbackStatus: status,
      isPlaybackActive: false,
    });
  },

  unloadPlayback: async () => {
    await bridgeUnloadDatalogPlayback();
    if (unlistenPlaybackStatus) {
      unlistenPlaybackStatus();
      unlistenPlaybackStatus = null;
    }
    if (get().connectionState.state === 'Connected') {
      bumpEpoch();
      set({
        playbackStatus: null,
        playbackSession: null,
        isPlaybackActive: false,
        telemetrySource: 'live',
      });
      subscribeLiveTelemetry();
    } else {
      bumpEpoch();
      if (unlistenTelemetry) {
        unlistenTelemetry();
        unlistenTelemetry = null;
      }
      set({
        playbackStatus: null,
        playbackSession: null,
        isPlaybackActive: false,
        telemetrySource: 'none',
      });
    }
  },

  setTelemetrySource: (source: TelemetrySourceType) => {
    bumpEpoch();
    set({ telemetrySource: source });

    if (source === 'live') {
      if (get().connectionState.state === 'Connected') {
        subscribeLiveTelemetry();
      } else {
        if (unlistenTelemetry) {
          unlistenTelemetry();
          unlistenTelemetry = null;
        }
      }
    } else if (source === 'playback') {
      if (get().playbackSession) {
        subscribePlaybackTelemetry();
      } else {
        if (unlistenTelemetry) {
          unlistenTelemetry();
          unlistenTelemetry = null;
        }
      }
    } else if (source === 'none') {
      if (unlistenTelemetry) {
        unlistenTelemetry();
        unlistenTelemetry = null;
      }
    }
  },
}));
