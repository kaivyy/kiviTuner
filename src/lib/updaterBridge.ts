import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { check as checkSignedUpdater } from '@tauri-apps/plugin-updater';
import { exit as exitProcess, relaunch as relaunchProcess } from '@tauri-apps/plugin-process';
import { isDesktopApp } from './tauriBridge.ts';

export const APP_VERSION = '0.1.2';
export const GITHUB_RELEASE_PAGE = 'https://github.com/kaivyy/kiviTuner/releases/latest';
const GITHUB_API_LATEST = 'https://api.github.com/repos/kaivyy/kiviTuner/releases/latest';
const UPDATE_PROGRESS_EVENT = 'kivituner:update-progress';

export interface AppUpdateInfo {
  updateAvailable: boolean;
  currentVersion: string;
  latestVersion: string;
  releaseName: string | null;
  releaseNotes: string;
  publishedAt: string | null;
  releaseUrl: string;
  assetName: string | null;
  assetUrl: string | null;
  assetSize: number | null;
  sha256Url: string | null;
  signedUpdaterReady: boolean;
}

export interface UpdateDownloadProgress {
  downloaded: number;
  total: number | null;
  percent: number;
  phase: string;
}

export interface UpdateInstallOutcome {
  mode: string;
  shouldExit: boolean;
  downloadedPath: string | null;
  message: string;
}

interface RustUpdateCheckResult {
  update_available: boolean;
  current_version: string;
  latest_version: string;
  release_name: string | null;
  release_notes: string;
  published_at: string | null;
  release_url: string;
  asset_name: string | null;
  asset_url: string | null;
  asset_size: number | null;
  sha256_url: string | null;
  signed_updater_ready: boolean;
}

interface RustUpdateInstallResult {
  mode: string;
  should_exit: boolean;
  downloaded_path: string;
  message: string;
}

interface GithubReleaseAssetJson {
  name: string;
  browser_download_url: string;
  size: number;
}

interface GithubReleaseJson {
  tag_name: string;
  name?: string;
  body?: string;
  published_at?: string;
  html_url?: string;
  draft?: boolean;
  prerelease?: boolean;
  assets?: GithubReleaseAssetJson[];
}

export function parseSemverTuple(raw: string): [number, number, number, boolean] | null {
  const cleaned = raw.trim().replace(/^[vV]/, '');
  if (!cleaned) return null;
  const dashIdx = cleaned.indexOf('-');
  const hasPre = dashIdx >= 0;
  const core = (hasPre ? cleaned.slice(0, dashIdx) : cleaned).split('+')[0];
  const parts = core.split('.');
  if (parts.length < 3) return null;
  const major = Number.parseInt(parts[0], 10);
  const minor = Number.parseInt(parts[1], 10);
  const patch = Number.parseInt(parts[2], 10);
  if (!Number.isFinite(major) || !Number.isFinite(minor) || !Number.isFinite(patch)) {
    return null;
  }
  return [major, minor, patch, hasPre];
}

export function isNewerVersion(current: string, latest: string): boolean {
  const cur = parseSemverTuple(current);
  const lat = parseSemverTuple(latest);
  if (!cur || !lat) return false;
  for (let i = 0; i < 3; i++) {
    if (lat[i] !== cur[i]) return lat[i] > cur[i];
  }
  return cur[3] && !lat[3];
}

export interface UpdaterBridgeDeps {
  isDesktop?: () => boolean;
  invokeFn?: typeof invoke;
  fetchFn?: typeof fetch;
}

export async function bridgeCheckForUpdate(deps?: UpdaterBridgeDeps): Promise<AppUpdateInfo> {
  const desktop = (deps?.isDesktop ?? isDesktopApp)();
  if (desktop) {
    const call = deps?.invokeFn ?? invoke;
    const raw = await call<RustUpdateCheckResult>('check_app_update');
    return {
      updateAvailable: raw.update_available,
      currentVersion: raw.current_version,
      latestVersion: raw.latest_version,
      releaseName: raw.release_name,
      releaseNotes: raw.release_notes,
      publishedAt: raw.published_at,
      releaseUrl: raw.release_url || GITHUB_RELEASE_PAGE,
      assetName: raw.asset_name,
      assetUrl: raw.asset_url,
      assetSize: raw.asset_size,
      sha256Url: raw.sha256_url,
      signedUpdaterReady: raw.signed_updater_ready,
    };
  }

  const doFetch = deps?.fetchFn ?? fetch;
  const res = await doFetch(GITHUB_API_LATEST, {
    headers: { Accept: 'application/vnd.github+json' },
  });
  if (!res.ok) {
    throw new Error(`Release check failed (${res.status})`);
  }
  const payload = (await res.json()) as GithubReleaseJson;
  if (payload.draft || payload.prerelease) {
    return {
      updateAvailable: false,
      currentVersion: APP_VERSION,
      latestVersion: APP_VERSION,
      releaseName: payload.name ?? null,
      releaseNotes: '',
      publishedAt: payload.published_at ?? null,
      releaseUrl: payload.html_url ?? GITHUB_RELEASE_PAGE,
      assetName: null,
      assetUrl: null,
      assetSize: null,
      sha256Url: null,
      signedUpdaterReady: false,
    };
  }
  const latestVersion = (payload.tag_name ?? '').trim().replace(/^[vV]/, '') || APP_VERSION;
  const updateAvailable = isNewerVersion(APP_VERSION, latestVersion);
  return {
    updateAvailable,
    currentVersion: APP_VERSION,
    latestVersion,
    releaseName: payload.name ?? null,
    releaseNotes: payload.body ?? '',
    publishedAt: payload.published_at ?? null,
    releaseUrl: payload.html_url ?? GITHUB_RELEASE_PAGE,
    assetName: null,
    assetUrl: null,
    assetSize: null,
    sha256Url: null,
    signedUpdaterReady: false,
  };
}

export async function bridgeDownloadAndInstallUpdate(
  info: AppUpdateInfo,
  onProgress?: (progress: UpdateDownloadProgress) => void
): Promise<UpdateInstallOutcome> {
  if (!isDesktopApp()) {
    throw new Error('In-app update installation requires the desktop application.');
  }

  if (info.signedUpdaterReady) {
    const update = await checkSignedUpdater();
    if (!update) {
      return {
        mode: 'up_to_date',
        shouldExit: false,
        downloadedPath: null,
        message: 'Application is already running the latest version.',
      };
    }
    let downloaded = 0;
    let total: number | null = null;
    await update.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        total = event.data.contentLength ?? null;
        onProgress?.({ downloaded: 0, total, percent: 0, phase: 'Downloading signed update...' });
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength;
        const percent = total && total > 0 ? Math.min(100, Math.round((downloaded * 100) / total)) : 0;
        onProgress?.({ downloaded, total, percent, phase: 'Downloading signed update...' });
      } else if (event.event === 'Finished') {
        onProgress?.({ downloaded, total: total ?? downloaded, percent: 100, phase: 'Finalizing...' });
      }
    });
    return {
      mode: 'restart_ready',
      shouldExit: false,
      downloadedPath: null,
      message: `Update v${info.latestVersion} verified and staged. Click Restart Now to finish.`,
    };
  }

  if (!info.assetUrl || !info.assetName) {
    throw new Error('No downloadable installer package found for this operating system.');
  }

  let unlisten: (() => void) | null = null;
  try {
    unlisten = await listen<UpdateDownloadProgress>(UPDATE_PROGRESS_EVENT, (ev) => {
      if (ev.payload) onProgress?.(ev.payload);
    });
  } catch {
    unlisten = null;
  }

  try {
    const res = await invoke<RustUpdateInstallResult>('download_and_install_update', {
      assetUrl: info.assetUrl,
      assetName: info.assetName,
      sha256Url: info.sha256Url,
    });
    return {
      mode: res.mode,
      shouldExit: res.should_exit,
      downloadedPath: res.downloaded_path,
      message: res.message,
    };
  } finally {
    if (unlisten) unlisten();
  }
}

export async function bridgeRestartApp(): Promise<void> {
  if (!isDesktopApp()) {
    if (typeof window !== 'undefined') window.location.reload();
    return;
  }
  try {
    await relaunchProcess();
  } catch {
    await invoke('restart_app');
  }
}

export async function bridgeExitForInstaller(): Promise<void> {
  if (!isDesktopApp()) return;
  try {
    await exitProcess(0);
  } catch {
    await invoke('restart_app');
  }
}
