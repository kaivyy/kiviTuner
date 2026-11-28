import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { DiffRegion } from './tauriBridge';
import { isDesktopApp } from './tauriBridge';
import type { MapDef } from '../types/kivi';

const WORKSPACE_SYNC_EVENT = 'kivituner:workspace-sync';
const WORKSPACE_SYNC_CHANNEL = 'kivituner-workspace-sync';
const SENDER_ID =
  typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `win-${Math.random().toString(36).slice(2)}`;

export type WorkspaceSyncEvent =
  | { type: 'add-maps'; maps: MapDef[]; selectFirst?: boolean }
  | { type: 'select-map'; mapId: string }
  | { type: 'update-map-scaling'; mapId: string; equation: string }
  | { type: 'jump-waveform'; startAddr: number; endAddr: number }
  | { type: 'diffs-updated'; diffs: DiffRegion[] }
  | { type: 'rom-updated'; checksumOk?: boolean; activeDocId?: string | null; revision?: number }
  | { type: 'request-snapshot' }
  | { type: 'workspace-snapshot'; maps: MapDef[]; selectedMapId: string | null; diffs: DiffRegion[] };

interface SyncEnvelope {
  senderId: string;
  payload: WorkspaceSyncEvent;
}

function unwrapSyncEnvelope(raw: unknown): WorkspaceSyncEvent | null {
  if (!raw || typeof raw !== 'object') return null;
  const obj = raw as Record<string, unknown>;
  if (typeof obj.senderId === 'string' && obj.payload && typeof obj.payload === 'object') {
    if (obj.senderId === SENDER_ID) return null;
    const payload = obj.payload as Record<string, unknown>;
    return typeof payload.type === 'string' ? (payload as unknown as WorkspaceSyncEvent) : null;
  }
  return typeof obj.type === 'string' ? (raw as WorkspaceSyncEvent) : null;
}

export async function emitWorkspaceSyncEvent(event: WorkspaceSyncEvent): Promise<void> {
  const envelope: SyncEnvelope = { senderId: SENDER_ID, payload: event };

  if (isDesktopApp()) {
    try {
      await invoke('broadcast_workspace_event', { event: envelope });
    } catch {
      // Ignore if backend listener is tearing down.
    }
    return;
  }

  if (typeof window !== 'undefined' && typeof BroadcastChannel !== 'undefined') {
    try {
      const ch = new BroadcastChannel(WORKSPACE_SYNC_CHANNEL);
      ch.postMessage(envelope);
      ch.close();
    } catch {
      // Ignore BroadcastChannel errors in restricted webviews.
    }
  }
}

export function subscribeWorkspaceSyncEvents(
  handler: (event: WorkspaceSyncEvent) => void
): () => void {
  let active = true;
  let unlistenTauri: (() => void) | null = null;
  let channel: BroadcastChannel | null = null;

  if (isDesktopApp()) {
    void listen<unknown>(WORKSPACE_SYNC_EVENT, (ev) => {
      if (!active) return;
      const parsed = unwrapSyncEnvelope(ev?.payload);
      if (parsed) handler(parsed);
    }).then((unsub) => {
      if (!active) {
        unsub();
      } else {
        unlistenTauri = unsub;
      }
    });
  } else if (typeof window !== 'undefined' && typeof BroadcastChannel !== 'undefined') {
    try {
      channel = new BroadcastChannel(WORKSPACE_SYNC_CHANNEL);
      channel.onmessage = (msg) => {
        if (!active) return;
        const parsed = unwrapSyncEnvelope(msg?.data);
        if (parsed) handler(parsed);
      };
    } catch {
      channel = null;
    }
  }

  return () => {
    active = false;
    if (channel) {
      try {
        channel.close();
      } catch {
        // Ignore close error.
      }
    }
    if (unlistenTauri) {
      unlistenTauri();
    }
  };
}
