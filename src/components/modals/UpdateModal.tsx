import React from 'react';
import { CheckCircle2, Download, ExternalLink, RefreshCw, RotateCw, ShieldCheck } from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { useUpdateStore } from '../../stores/useUpdateStore';
import { APP_VERSION, GITHUB_RELEASE_PAGE } from '../../lib/updaterBridge';

function formatBytesMb(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return '';
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export const UpdateModal: React.FC = () => {
  const {
    isModalOpen,
    closeModal,
    status,
    info,
    progress,
    installOutcome,
    error,
    checkForUpdates,
    downloadAndInstall,
    restartToApply,
  } = useUpdateStore();

  const currentVersion = info?.currentVersion ?? APP_VERSION;
  const latestVersion = info?.latestVersion ?? currentVersion;

  return (
    <FloatingWindow
      windowId="app-updater"
      isOpen={isModalOpen}
      onClose={closeModal}
      title="Software Updates"
      titleId="updater-modal-title"
      badge={`v${currentVersion}`}
      icon={<Download className="h-4 w-4" />}
      defaultWidth={520}
      defaultHeight={460}
      minWidth={340}
      minHeight={340}
      dock="center"
      closeLabel="Close Software Updates dialog"
      footer={
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-kiviBorder bg-kiviElevated px-4 py-2.5">
          <a
            href={info?.releaseUrl ?? GITHUB_RELEASE_PAGE}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-[11px] text-kiviSecondary hover:text-kiviInk hover:underline"
          >
            <ExternalLink className="h-3 w-3 shrink-0" />
            <span>GitHub Releases</span>
          </a>

          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={status === 'checking' || status === 'downloading'}
              onClick={() => void checkForUpdates(false)}
              className="inline-flex items-center gap-1.5 rounded border border-kiviBorder/70 bg-kiviSurface px-3 py-1.5 text-xs font-medium text-kiviInk hover:bg-kiviBg disabled:opacity-40"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${status === 'checking' ? 'animate-spin' : ''}`} />
              <span>Check Again</span>
            </button>

            {status === 'available' && (
              <button
                type="button"
                onClick={() => void downloadAndInstall()}
                className="inline-flex items-center gap-1.5 rounded bg-kiviAccent px-3 py-1.5 text-xs font-semibold text-kiviAccentFg hover:opacity-90"
              >
                <Download className="h-3.5 w-3.5" />
                <span>Download &amp; Install v{latestVersion}</span>
              </button>
            )}

            {status === 'ready-to-restart' && (
              <button
                type="button"
                onClick={() => void restartToApply()}
                className="inline-flex items-center gap-1.5 rounded bg-emerald-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-emerald-700"
              >
                <RotateCw className="h-3.5 w-3.5" />
                <span>Restart App Now</span>
              </button>
            )}
          </div>
        </div>
      }
    >
      <div className="flex flex-1 flex-col gap-3 overflow-y-auto p-4 text-xs text-kiviSecondary">
        <div className="grid grid-cols-2 gap-2 rounded-md border border-kiviBorder bg-kiviBg p-3 font-mono text-[11px] tabular-nums">
          <div>
            <div className="text-[10px] uppercase text-kiviMuted">Installed</div>
            <div className="mt-0.5 text-sm font-bold text-kiviInk">v{currentVersion}</div>
          </div>
          <div>
            <div className="text-[10px] uppercase text-kiviMuted">Latest Release</div>
            <div className="mt-0.5 flex items-center gap-1.5 text-sm font-bold text-kiviAccent">
              <span>v{latestVersion}</span>
              {info?.updateAvailable && (
                <span className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-1.5 py-0.5 text-[10px]">
                  New
                </span>
              )}
            </div>
          </div>
        </div>

        {status === 'checking' && (
          <div className="rounded border border-kiviBorder bg-kiviBg px-3 py-2 text-kiviSecondary">
            Checking latest release manifest from GitHub...
          </div>
        )}

        {status === 'up-to-date' && (
          <div className="flex items-center gap-2 rounded border border-kiviGreen/40 bg-kiviGreen/10 px-3 py-2 text-kiviGreen">
            <CheckCircle2 className="h-4 w-4 shrink-0" />
            <span>You are running the latest version (v{currentVersion}).</span>
          </div>
        )}

        {status === 'available' && info && (
          <div className="flex items-center justify-between gap-2 rounded border border-kiviAccent/40 bg-kiviAccent/10 px-3 py-2 text-kiviInk">
            <div className="flex items-center gap-2">
              <ShieldCheck className="h-4 w-4 shrink-0 text-kiviAccent" />
              <span>
                {info.signedUpdaterReady
                  ? 'Signed package verification ready.'
                  : info.sha256Url
                    ? 'SHA-256 checksum verification ready.'
                    : 'Verified GitHub release package ready.'}
              </span>
            </div>
            {info.assetSize && (
              <span className="font-mono text-[11px] text-kiviMuted tabular-nums">
                {formatBytesMb(info.assetSize)}
              </span>
            )}
          </div>
        )}

        {(status === 'downloading' || status === 'ready-to-restart') && progress && (
          <div className="space-y-1.5 rounded border border-kiviBorder bg-kiviBg p-3">
            <div className="flex items-center justify-between gap-2 text-[11px] text-kiviInk">
              <span className="truncate">{progress.phase}</span>
              <span className="font-mono font-semibold text-kiviAccent tabular-nums">
                {progress.percent}%
              </span>
            </div>
            <div className="h-2 w-full overflow-hidden rounded bg-kiviElevated">
              <div
                className="h-full bg-kiviAccent transition-all"
                style={{ width: `${Math.max(2, Math.min(100, progress.percent))}%` }}
              />
            </div>
            {progress.total && (
              <div className="font-mono text-[10px] text-kiviMuted tabular-nums">
                {formatBytesMb(progress.downloaded)} / {formatBytesMb(progress.total)}
              </div>
            )}
            {installOutcome && (
              <p className="pt-1 text-[11px] text-kiviGreen">{installOutcome.message}</p>
            )}
          </div>
        )}

        {status === 'error' && error && (
          <div className="rounded border border-kiviRed/40 bg-kiviRed/10 px-3 py-2 text-kiviRed">
            Update check or download failed: {error}
          </div>
        )}

        <div className="flex flex-1 flex-col rounded border border-kiviBorder bg-kiviBg p-3">
          <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-kiviMuted">
            Release Notes
          </div>
          <pre className="flex-1 whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-kiviSecondary">
            {info?.releaseNotes?.trim() ||
              'Click "Check Again" to fetch the newest release notes and installer assets.'}
          </pre>
        </div>
      </div>
    </FloatingWindow>
  );
};
