import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  ConnectionState,
  DriverDescriptor,
  EcuIdentity,
  TelemetryFrame,
} from '../../types/telemetry.ts';
import { isDesktopApp } from './romBridge.ts';

const telemetrySubscribers = new Set<(frame: TelemetryFrame) => void>();
const statusSubscribers = new Set<(state: ConnectionState) => void>();
let lastPlaybackFrame: TelemetryFrame | null = null;

/** Internal dispatcher used by CSV datalog playback in browser/Node mode. */
export function emitLocalPlaybackFrame(frame: TelemetryFrame): void {
  lastPlaybackFrame = frame;
  for (const cb of telemetrySubscribers) {
    cb(frame);
  }
}

export function clearLocalTelemetryState(): void {
  lastPlaybackFrame = null;
}

/**
 * Enumerates real hardware ECU communication drivers from the desktop backend.
 * Returns an empty list in browser mode (no simulated drivers).
 */
export async function bridgeListDrivers(): Promise<DriverDescriptor[]> {
  if (isDesktopApp()) {
    return await invoke<DriverDescriptor[]>('list_ecu_drivers');
  }
  return [];
}

/**
 * Connects to a real hardware ECU driver (`obd2:<port>` or `ini_serial:<port>`).
 */
export async function bridgeConnectEcu(driverId: string): Promise<EcuIdentity> {
  if (isDesktopApp()) {
    return await invoke<EcuIdentity>('connect_ecu', { driverId });
  }
  throw new Error(
    `Hardware ECU connection ('${driverId}') requires the kiviTuner desktop application.`
  );
}

/**
 * Disconnects from the currently active ECU session.
 */
export async function bridgeDisconnectEcu(): Promise<void> {
  if (isDesktopApp()) {
    await invoke<void>('disconnect_ecu');
    return;
  }
  const disconnected: ConnectionState = { state: 'Disconnected' };
  for (const cb of statusSubscribers) {
    cb(disconnected);
  }
}

/**
 * Retrieves the current ECU connection state.
 */
export async function bridgeGetConnectionStatus(): Promise<ConnectionState> {
  if (isDesktopApp()) {
    return await invoke<ConnectionState>('get_connection_status');
  }
  return { state: 'Disconnected' };
}

/**
 * Retrieves the most recent telemetry frame.
 */
export async function bridgeGetLatestTelemetry(): Promise<TelemetryFrame | null> {
  if (isDesktopApp()) {
    return await invoke<TelemetryFrame | null>('get_latest_telemetry');
  }
  return lastPlaybackFrame;
}

/**
 * Subscribes to live telemetry frames emitted by the backend or CSV playback.
 */
export function bridgeSubscribeTelemetry(
  callback: (frame: TelemetryFrame) => void
): () => void {
  if (isDesktopApp()) {
    let unlistenFn: (() => void) | null = null;
    let unsubscribed = false;

    listen<TelemetryFrame>('kivi://telemetry', (event) => {
      callback(event.payload);
    })
      .then((unlisten) => {
        if (unsubscribed) {
          unlisten();
        } else {
          unlistenFn = unlisten;
        }
      })
      .catch((err) => {
        console.warn('Failed to listen to kivi://telemetry:', err);
      });

    return () => {
      unsubscribed = true;
      if (unlistenFn) unlistenFn();
    };
  }

  telemetrySubscribers.add(callback);
  return () => {
    telemetrySubscribers.delete(callback);
  };
}

/**
 * Subscribes to ECU connection state transitions.
 */
export function bridgeSubscribeEcuStatus(
  callback: (state: ConnectionState) => void
): () => void {
  if (isDesktopApp()) {
    let unlistenFn: (() => void) | null = null;
    let unsubscribed = false;

    listen<ConnectionState>('kivi://ecu-status', (event) => {
      callback(event.payload);
    })
      .then((unlisten) => {
        if (unsubscribed) {
          unlisten();
        } else {
          unlistenFn = unlisten;
        }
      })
      .catch((err) => {
        console.warn('Failed to listen to kivi://ecu-status:', err);
      });

    return () => {
      unsubscribed = true;
      if (unlistenFn) unlistenFn();
    };
  }

  statusSubscribers.add(callback);
  return () => {
    statusSubscribers.delete(callback);
  };
}
