import { create } from 'zustand';
import type { MapDef } from '../types/kivi.ts';
import type { TelemetryFrame } from '../types/telemetry.ts';
import type {
  CorrelateTelemetryRequest,
  CorrelateTelemetryResponse,
  CorrelationCommandError,
  CorrelationResult,
  TableBindingConfig,
  AutoTuneConfig,
  AutoTuneRequest,
  AutoTuneResponse,
  AutoTuneCommandError,
  RecommendationReport,
} from '../types/correlation.ts';
import { DEFAULT_AUTOTUNE_CONFIG } from '../types/correlation.ts';
import {
  bridgeCorrelateTelemetry,
  bridgeComputeAutoTuneRecommendations,
  normalizeCorrelationError,
  normalizeAutoTuneError,
} from '../lib/correlationBridge.ts';

export type CorrelationStoreStatus = 'idle' | 'loading' | 'success' | 'unavailable' | 'error';
export type AutoTuneStoreStatus = 'idle' | 'loading' | 'success' | 'error';

export interface CorrelationStoreState {
  // Target Calibration Map
  targetTableId: string | null;
  targetMapDef: MapDef | null;
  bindingConfig: TableBindingConfig | null;

  // Lifecycle & Generation Counters
  sourceEpoch: number;
  requestGeneration: number;
  autoTuneGeneration: number;

  // Correlation State
  correlationStatus: CorrelationStoreStatus;
  activeCorrelation: CorrelationResult | null;
  activeCorrelationResponse: CorrelateTelemetryResponse | null;
  correlationError: CorrelationCommandError | null;

  // Auto-Tune State
  autoTuneStatus: AutoTuneStoreStatus;
  autoTuneConfig: AutoTuneConfig;
  activeReport: RecommendationReport | null;
  activeReportResponse: AutoTuneResponse | null;
  autoTuneError: AutoTuneCommandError | null;

  // Pluggable dispatch functions (allows mock injection in tests)
  correlateFrameWithBackend: (
    req: CorrelateTelemetryRequest
  ) => Promise<CorrelateTelemetryResponse>;
  computeAutoTuneWithBackend: (req: AutoTuneRequest) => Promise<AutoTuneResponse>;

  // Actions
  setTargetTable: (tableId: string | null, mapDef: MapDef | null) => void;
  setBindingConfig: (config: TableBindingConfig | null) => void;
  setSourceEpoch: (epoch: number) => void;
  setAutoTuneConfig: (config: AutoTuneConfig) => void;

  correlateFrame: (frame?: TelemetryFrame) => Promise<void>;
  computeAutoTuneRecommendations: (params?: {
    config?: AutoTuneConfig;
    frames?: TelemetryFrame[];
  }) => Promise<void>;

  invalidateOnSourceChange: (newSource?: string) => void;
  invalidateOnSeek: () => void;
  invalidateOnDisconnect: () => void;
  reset: () => void;
}

export const useCorrelationStore = create<CorrelationStoreState>((set, get) => ({
  targetTableId: null,
  targetMapDef: null,
  bindingConfig: null,

  sourceEpoch: 0,
  requestGeneration: 0,
  autoTuneGeneration: 0,

  correlationStatus: 'idle',
  activeCorrelation: null,
  activeCorrelationResponse: null,
  correlationError: null,

  autoTuneStatus: 'idle',
  autoTuneConfig: DEFAULT_AUTOTUNE_CONFIG,
  activeReport: null,
  activeReportResponse: null,
  autoTuneError: null,

  correlateFrameWithBackend: bridgeCorrelateTelemetry,
  computeAutoTuneWithBackend: bridgeComputeAutoTuneRecommendations,

  setTargetTable: (tableId, mapDef) => {
    set((state) => ({
      targetTableId: tableId,
      targetMapDef: mapDef,
      requestGeneration: state.requestGeneration + 1,
      autoTuneGeneration: state.autoTuneGeneration + 1,
      activeCorrelation: null,
      activeCorrelationResponse: null,
      correlationStatus: 'idle',
      correlationError: null,
      activeReport: null,
      activeReportResponse: null,
      autoTuneStatus: 'idle',
      autoTuneError: null,
    }));
  },

  setBindingConfig: (config) => {
    set((state) => ({
      bindingConfig: config,
      requestGeneration: state.requestGeneration + 1,
      activeCorrelation: null,
      activeCorrelationResponse: null,
      correlationStatus: 'idle',
      correlationError: null,
    }));
  },

  setSourceEpoch: (epoch) => {
    set((state) => {
      if (state.sourceEpoch === epoch) return {};
      return {
        sourceEpoch: epoch,
        requestGeneration: state.requestGeneration + 1,
        activeCorrelation: null,
        activeCorrelationResponse: null,
        correlationStatus: 'idle',
        correlationError: null,
      };
    });
  },

  setAutoTuneConfig: (config) => {
    set((state) => ({
      autoTuneConfig: config,
      autoTuneGeneration: state.autoTuneGeneration + 1,
      activeReport: null,
      activeReportResponse: null,
      autoTuneStatus: 'idle',
      autoTuneError: null,
    }));
  },

  correlateFrame: async (frame) => {
    const nextGeneration = get().requestGeneration + 1;
    set({
      requestGeneration: nextGeneration,
      correlationStatus: 'loading',
      correlationError: null,
    });

    const requestId = `corr-${nextGeneration}-${Date.now()}`;
    const req: CorrelateTelemetryRequest = {
      table_id: get().targetTableId ?? undefined,
      map_def: get().targetMapDef ?? undefined,
      frame,
      binding_config: get().bindingConfig ?? undefined,
      source_epoch: get().sourceEpoch,
      request_id: requestId,
    };

    try {
      const resp = await get().correlateFrameWithBackend(req);

      // Verify request generation before committing asynchronous result
      if (get().requestGeneration !== nextGeneration) {
        return; // Discard stale response
      }

      // Authoritative epoch verification: discard response if source epoch changed
      if (
        resp.source_epoch !== undefined &&
        resp.source_epoch !== null &&
        resp.source_epoch !== get().sourceEpoch
      ) {
        return; // Discard response from obsolete source epoch
      }

      set({
        correlationStatus: 'success',
        activeCorrelation: resp.result,
        activeCorrelationResponse: resp,
        correlationError: null,
      });
    } catch (err) {
      if (get().requestGeneration !== nextGeneration) {
        return; // Discard stale error
      }

      const normalized = normalizeCorrelationError(err);
      set({
        correlationStatus: 'error',
        activeCorrelation: null,
        activeCorrelationResponse: null,
        correlationError: normalized,
      });
    }
  },

  computeAutoTuneRecommendations: async (params) => {
    const nextGeneration = get().autoTuneGeneration + 1;
    set({
      autoTuneGeneration: nextGeneration,
      autoTuneStatus: 'loading',
      autoTuneError: null,
    });

    const cfg = params?.config ?? get().autoTuneConfig;
    const requestId = `tune-${nextGeneration}-${Date.now()}`;

    const req: AutoTuneRequest = {
      table_id: get().targetTableId ?? undefined,
      map_def: get().targetMapDef ?? undefined,
      config: cfg,
      frames: params?.frames,
      request_id: requestId,
    };

    try {
      const resp = await get().computeAutoTuneWithBackend(req);

      if (get().autoTuneGeneration !== nextGeneration) {
        return; // Discard stale response
      }

      set({
        autoTuneStatus: 'success',
        activeReport: resp.report,
        activeReportResponse: resp,
        autoTuneError: null,
      });
    } catch (err) {
      if (get().autoTuneGeneration !== nextGeneration) {
        return; // Discard stale error
      }

      const normalized = normalizeAutoTuneError(err);
      set({
        autoTuneStatus: 'error',
        activeReport: null,
        activeReportResponse: null,
        autoTuneError: normalized,
      });
    }
  },

  invalidateOnSourceChange: () => {
    set((state) => ({
      requestGeneration: state.requestGeneration + 1,
      activeCorrelation: null,
      activeCorrelationResponse: null,
      correlationStatus: 'idle',
      correlationError: null,
    }));
  },

  invalidateOnSeek: () => {
    set((state) => ({
      requestGeneration: state.requestGeneration + 1,
      activeCorrelation: null,
      activeCorrelationResponse: null,
      correlationStatus: 'idle',
      correlationError: null,
    }));
  },

  invalidateOnDisconnect: () => {
    set((state) => ({
      requestGeneration: state.requestGeneration + 1,
      activeCorrelation: null,
      activeCorrelationResponse: null,
      correlationStatus: 'unavailable',
      correlationError: null,
    }));
  },

  reset: () => {
    set({
      targetTableId: null,
      targetMapDef: null,
      bindingConfig: null,
      sourceEpoch: 0,
      requestGeneration: 0,
      autoTuneGeneration: 0,
      correlationStatus: 'idle',
      activeCorrelation: null,
      activeCorrelationResponse: null,
      correlationError: null,
      autoTuneStatus: 'idle',
      autoTuneConfig: DEFAULT_AUTOTUNE_CONFIG,
      activeReport: null,
      activeReportResponse: null,
      autoTuneError: null,
      correlateFrameWithBackend: bridgeCorrelateTelemetry,
      computeAutoTuneWithBackend: bridgeComputeAutoTuneRecommendations,
    });
  },
}));
