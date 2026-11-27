/**
 * Canonical typed IPC & bridge barrel.
 * Re-exports all focused modules under `./bridge/` to keep files modular (< 1,000 lines)
 * while preserving every existing import path unchanged.
 */

export type {
  DiffRegion,
  DiffKind,
  DetectedMapCandidate,
} from '../types/kivi.ts';
export type {
  TelemetryStatusFlags,
  TelemetryFrame,
  EcuIdentity,
  ConnectionState,
  DriverDescriptor,
  RecordingState,
  SessionMetadata,
  DatalogRecorderStatus,
  PlaybackState,
  PlaybackStatus,
  TelemetrySourceType,
} from '../types/telemetry.ts';

export * from './bridge/romBridge.ts';
export * from './bridge/windowBridge.ts';
export * from './bridge/romMathBridge.ts';
export * from './bridge/diffScanBridge.ts';
export * from './bridge/telemetryBridge.ts';
export * from './bridge/datalogBridge.ts';
export * from './bridge/toolkitBridge.ts';
export * from './bridge/ecuToolsBridge.ts';
