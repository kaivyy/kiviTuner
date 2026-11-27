import { create } from 'zustand';
import {
  APP_VERSION,
  bridgeCheckForUpdate,
  bridgeDownloadAndInstallUpdate,
  bridgeExitForInstaller,
  bridgeRestartApp,
  type AppUpdateInfo,
  type UpdateDownloadProgress,
  type UpdateInstallOutcome,
} from '../lib/updaterBridge';

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'available'
  | 'downloading'
  | 'ready-to-restart'
  | 'error';

export interface UpdateStoreState {
  status: UpdateStatus;
  info: AppUpdateInfo | null;
  progress: UpdateDownloadProgress | null;
  installOutcome: UpdateInstallOutcome | null;
  error: string | null;
  isModalOpen: boolean;
  bannerDismissed: boolean;
  lastCheckedAt: number | null;

  openModal: () => void;
  closeModal: () => void;
  dismissBanner: () => void;
  checkForUpdates: (silent?: boolean) => Promise<void>;
  downloadAndInstall: () => Promise<void>;
  restartToApply: () => Promise<void>;
}

export const useUpdateStore = create<UpdateStoreState>((set, get) => ({
  status: 'idle',
  info: null,
  progress: null,
  installOutcome: null,
  error: null,
  isModalOpen: false,
  bannerDismissed: false,
  lastCheckedAt: null,

  openModal: () => set({ isModalOpen: true }),
  closeModal: () => set({ isModalOpen: false }),
  dismissBanner: () => set({ bannerDismissed: true }),

  checkForUpdates: async (silent = false) => {
    const currentStatus = get().status;
    if (currentStatus === 'checking' || currentStatus === 'downloading') return;
    set({
      status: 'checking',
      error: null,
      ...(!silent ? { isModalOpen: true } : {}),
    });
    try {
      const info = await bridgeCheckForUpdate();
      set({
        info,
        lastCheckedAt: Date.now(),
        status: info.updateAvailable ? 'available' : 'up-to-date',
        ...(info.updateAvailable ? { bannerDismissed: false } : {}),
      });
    } catch (err) {
      if (silent) {
        // Silent background check failures must not disrupt the user with noisy errors.
        set({ status: 'idle', error: null });
      } else {
        set({
          status: 'error',
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
  },

  downloadAndInstall: async () => {
    const { info, status } = get();
    if (!info || status === 'downloading') return;
    set({
      status: 'downloading',
      error: null,
      progress: { downloaded: 0, total: info.assetSize, percent: 0, phase: 'Starting download...' },
    });
    try {
      const outcome = await bridgeDownloadAndInstallUpdate(info, (prog) => {
        set({ progress: prog });
      });
      set({
        installOutcome: outcome,
        status: 'ready-to-restart',
        progress: {
          downloaded: info.assetSize ?? 1,
          total: info.assetSize ?? 1,
          percent: 100,
          phase: outcome.message,
        },
      });
    } catch (err) {
      set({
        status: 'error',
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  restartToApply: async () => {
    const outcome = get().installOutcome;
    if (outcome?.shouldExit) {
      await bridgeExitForInstaller();
    } else {
      await bridgeRestartApp();
    }
  },
}));

export function getInstalledVersionLabel(): string {
  return `v${APP_VERSION}`;
}
