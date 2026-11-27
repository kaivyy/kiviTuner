import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  Disc,
  Download,
  Pause,
  Play,
  Power,
  Repeat,
  RotateCcw,
  Square,
  Upload,
  X,
} from 'lucide-react';
import { useTelemetryStore } from '../../stores/useTelemetryStore.ts';
import { SupercarCluster } from './SupercarCluster.tsx';
import {
  formatTimecode,
  getDatalogToolbarStates,
  getRecordingBadgeInfo,
  getSourceBadgeInfo,
} from './telemetryPanelModel.ts';

import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';

export * from './telemetryPanelModel.ts';

export interface TelemetryDevPanelProps {
  onClose?: () => void;
  standalone?: boolean;
}

export const TelemetryDevPanel: React.FC<TelemetryDevPanelProps> = ({
  onClose,
  standalone = false,
}) => {
  const {
    connectionState,
    identity,
    latestFrame,
    selectedDriverId,
    history,
    availableDrivers,
    fetchDrivers,
    selectDriver,
    connect,
    disconnect,
    togglePanel,

    // Milestone 3B Datalogger & Playback slices
    telemetrySource,
    recordingStatus,
    playbackStatus,
    playbackSession,
    isRecording,
    startRecording,
    stopRecording,
    exportDatalogCsv,
    importDatalogCsv,
    startPlayback,
    pausePlayback,
    seekPlayback,
    setPlaybackSpeed,
    setPlaybackLoop,
    stopPlayback,
    unloadPlayback,
    setTelemetrySource,
  } = useTelemetryStore();

  const [isConnecting, setIsConnecting] = useState(false);
  const csvFileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    fetchDrivers();
  }, [fetchDrivers]);

  const handleClose = () => {
    if (onClose) {
      onClose();
    } else {
      togglePanel(false);
    }
  };

  const isConnected = connectionState.state === 'Connected';
  const isFaulted = connectionState.state === 'Faulted';

  const handleToggleConnection = async () => {
    if (isConnected) {
      await disconnect();
    } else {
      setIsConnecting(true);
      try {
        await connect();
      } catch (err) {
        console.warn('Connect error:', err);
      } finally {
        setIsConnecting(false);
      }
    }
  };

  const handleExportCsv = async () => {
    try {
      const csv = await exportDatalogCsv();
      if (!csv) return;
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = 'kivi_datalog_session.csv';
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
    } catch (err) {
      console.warn('Failed to export datalog CSV:', err);
    }
  };

  const handleImportCsvFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const content = await file.text();
      await importDatalogCsv(content);
    } catch (err) {
      console.warn('Failed to import datalog CSV:', err);
    } finally {
      if (csvFileInputRef.current) {
        csvFileInputRef.current.value = '';
      }
    }
  };

  // Toolbar & badge helpers
  const toolbarStates = useMemo(() => {
    return getDatalogToolbarStates({
      telemetrySource,
      isRecording,
      recordingState: recordingStatus.state.state,
      droppedCount: recordingStatus.dropped_count,
      hasPlayback: playbackSession !== null,
    });
  }, [telemetrySource, isRecording, recordingStatus, playbackSession]);

  const recordingBadge = useMemo(() => {
    return getRecordingBadgeInfo({
      isRecording,
      elapsedMs: recordingStatus.elapsed_ms,
      sampleCount: recordingStatus.sample_count,
      droppedCount: recordingStatus.dropped_count,
      state: recordingStatus.state.state,
    });
  }, [isRecording, recordingStatus]);

  const sourceBadge = useMemo(() => {
    return getSourceBadgeInfo(telemetrySource);
  }, [telemetrySource]);

  const currentTimeMs = playbackStatus?.current_time_ms ?? 0;
  const totalDurationMs = playbackStatus?.total_duration_ms ?? 0;
  const isPlaying = playbackStatus?.state === 'Playing';
  const isLoop = playbackStatus?.loop_playback ?? false;
  const currentSpeed = playbackStatus?.speed ?? 1.0;
  const spec = TOOL_WINDOW_SPECS.telemetry;

  return (
    <FloatingWindow
      windowId="telemetry"
      isOpen
      onClose={handleClose}
      title="ECU Live Telemetry & Diagnostics"
      ariaLabel="ECU Live Telemetry & Diagnostics"
      icon={<Activity className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="telemetry"
      titlebarTone="sidebar"
      closeLabel="Close diagnostic panel"
      footer={
        <div
          data-testid="telemetry-footer"
          className="flex shrink-0 items-center justify-between gap-2 whitespace-nowrap border-t border-kiviBorder bg-kiviSurface px-4 py-2 font-mono text-[10px] text-kiviMuted"
        >
          <span className="min-w-0 truncate tabular-nums">
            Frame Sequence: #{latestFrame?.sequence ?? 0}
          </span>
          <span className="shrink-0 tabular-nums">
            Stream:{' '}
            {telemetrySource === 'playback'
              ? `Playback (${currentSpeed}x)`
              : isConnected
                ? '20 Hz Active'
                : 'Idle'}
          </span>
        </div>
      }
    >
      {/* Inline Bench Simulation Banner */}
      <div className="flex shrink-0 items-center gap-1.5 border-b border-kiviWarn/30 bg-kiviWarn/10 px-4 py-1 text-[10px] font-medium text-kiviWarn">
        <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">SIMULATED BENCH DATA - NOT PHYSICAL ECU</span>
      </div>

      {/* 1B. STATUS & SOURCE STRIP */}
      <div
        data-testid="telemetry-source-strip"
        className="flex min-h-[38px] flex-wrap items-center justify-between gap-2 border-b border-kiviBorder bg-kiviBg/60 px-4 py-2 text-xs"
      >
        {/* Source Badge & Live/Playback Toggle */}
        <div className="flex items-center gap-2">
          {sourceBadge.color === 'emerald' ? (
            <span className="inline-flex min-w-[106px] items-center justify-center gap-1.5 whitespace-nowrap rounded-full border border-kiviGreen/40 bg-kiviGreen/10 px-2.5 py-0.5 text-[11px] font-semibold text-kiviGreen">
              <span className="h-2 w-2 shrink-0 rounded-full bg-kiviGreen" />
              <span>{sourceBadge.label}</span>
            </span>
          ) : sourceBadge.color === 'sky' ? (
            <span className="inline-flex min-w-[106px] items-center justify-center gap-1.5 whitespace-nowrap rounded-full border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-0.5 text-[11px] font-semibold text-kiviAccent">
              <span className="h-2 w-2 shrink-0 rounded-full bg-kiviAccent" />
              <span>{sourceBadge.label}</span>
            </span>
          ) : (
            <span className="inline-flex min-w-[106px] items-center justify-center gap-1.5 whitespace-nowrap rounded-full border border-kiviBorderStrong bg-kiviMuted/10 px-2.5 py-0.5 text-[11px] font-medium text-kiviMuted">
              <span className="h-2 w-2 shrink-0 rounded-full bg-kiviMuted" />
              <span>{sourceBadge.label}</span>
            </span>
          )}

          {/* Toggle pill if connected to ECU AND a playback session is loaded */}
          {isConnected && (playbackSession !== null || playbackStatus !== null) && (
            <div className="inline-flex items-center rounded-md border border-kiviBorder bg-kiviBg p-0.5 text-[10px] font-semibold">
              <button
                onClick={() => setTelemetrySource('live')}
                className={`rounded px-2 py-0.5 transition-colors ${
                  telemetrySource === 'live'
                    ? 'bg-kiviGreen text-kiviAccentFg'
                    : 'text-kiviMuted hover:text-kiviInk'
                }`}
              >
                View: Live
              </button>
              <button
                onClick={() => setTelemetrySource('playback')}
                className={`rounded px-2 py-0.5 transition-colors ${
                  telemetrySource === 'playback'
                    ? 'bg-kiviAccent text-kiviAccentFg'
                    : 'text-kiviMuted hover:text-kiviInk'
                }`}
              >
                View: Playback
              </button>
            </div>
          )}
        </div>

        {/* Recording Status Badge */}
        <div className="flex min-h-[22px] items-center gap-1.5 tabular-nums whitespace-nowrap">
          {recordingBadge.badge === 'recording' && (
            <span className="inline-flex items-center gap-1.5 whitespace-nowrap rounded border border-kiviRed/40 bg-kiviRed/15 px-2 py-0.5 text-[10px] font-bold text-kiviRed">
              <span className="h-2 w-2 shrink-0 rounded-full bg-kiviRed" />
              <span>{recordingBadge.label}</span>
            </span>
          )}
          {recordingBadge.badge === 'interrupted' && (
            <span
              className="inline-flex items-center gap-1 whitespace-nowrap rounded border border-kiviWarn/40 bg-kiviWarn/15 px-2 py-0.5 text-[10px] font-bold text-kiviWarn"
              title={`${recordingStatus.dropped_count} frames dropped`}
            >
              <AlertTriangle className="h-3 w-3 shrink-0 text-kiviWarn" />
              <span>{recordingBadge.label}</span>
            </span>
          )}
          {recordingBadge.badge === 'ready' && (
            <span className="inline-flex items-center gap-1 whitespace-nowrap rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2 py-0.5 text-[10px] font-medium text-kiviAccent">
              <Download className="h-3 w-3 shrink-0 text-kiviAccent" />
              <span>{recordingBadge.label}</span>
            </span>
          )}
        </div>
      </div>

      {/* SCROLLABLE MAIN CONTENT */}
      <div className="flex-1 overflow-y-auto bg-kiviBg p-3.5 space-y-3.5">
        <SupercarCluster latestFrame={latestFrame} history={history} />

        {/* 2. ACCESSIBLE PLAYBACK SCRUBBER (Visible when playback session or status exists) */}
        {(playbackSession || playbackStatus) && (
          <div className="rounded-md border border-kiviAccent/40 bg-kiviAccent/10 p-3 space-y-2.5">
            <div className="flex items-center justify-between gap-2 text-xs">
              <div className="flex items-center gap-1.5 min-w-0 flex-1">
                <span className="h-2 w-2 shrink-0 rounded-full bg-kiviAccent" />
                <span className="font-semibold text-kiviAccent truncate">
                  Playback: {playbackSession?.notes || 'Imported Datalog Session'}
                </span>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <span className="min-w-[116px] text-right font-mono text-xs font-bold tabular-nums whitespace-nowrap text-kiviAccent">
                  {formatTimecode(currentTimeMs)} / {formatTimecode(totalDurationMs)}
                </span>
                <button
                  onClick={() => unloadPlayback()}
                  aria-label="Unload playback session"
                  title="Close playback session"
                  className="rounded p-1 text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk transition-colors"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            {/* Range Slider */}
            <div className="relative flex items-center">
              <input
                type="range"
                min={0}
                max={totalDurationMs}
                value={currentTimeMs}
                onChange={(e) => seekPlayback(Number(e.target.value))}
                aria-label="Playback timeline scrubber"
                aria-valuenow={currentTimeMs}
                aria-valuemin={0}
                aria-valuemax={totalDurationMs}
                aria-valuetext={`${formatTimecode(currentTimeMs)} / ${formatTimecode(totalDurationMs)}`}
                className="w-full h-1.5 bg-kiviBg rounded-lg appearance-none cursor-pointer accent-kiviAccent"
              />
            </div>

            {/* Transport Controls Row */}
            <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
              <div className="flex flex-wrap items-center gap-1.5">
                {/* Rewind */}
                <button
                  onClick={() => seekPlayback(0)}
                  aria-label="Rewind to start"
                  title="Rewind to start"
                  className="rounded p-1.5 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
                >
                  <RotateCcw className="h-3.5 w-3.5" />
                </button>

                {/* Play/Pause */}
                {isPlaying ? (
                  <button
                    onClick={() => pausePlayback()}
                    aria-label="Pause playback"
                    title="Pause"
                    className="flex w-[76px] items-center justify-center gap-1 whitespace-nowrap rounded bg-kiviAccent px-2.5 py-1 text-xs font-semibold text-kiviAccentFg transition-opacity hover:opacity-90"
                  >
                    <Pause className="h-3.5 w-3.5 shrink-0" />
                    <span>Pause</span>
                  </button>
                ) : (
                  <button
                    onClick={() => startPlayback()}
                    aria-label="Start playback"
                    title="Play"
                    className="flex w-[76px] items-center justify-center gap-1 whitespace-nowrap rounded bg-kiviGreen px-2.5 py-1 text-xs font-semibold text-kiviAccentFg transition-opacity hover:opacity-90"
                  >
                    <Play className="h-3.5 w-3.5 shrink-0" />
                    <span>Play</span>
                  </button>
                )}

                {/* Stop */}
                <button
                  onClick={() => stopPlayback()}
                  aria-label="Stop playback"
                  title="Stop"
                  className="rounded p-1.5 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
                >
                  <Square className="h-3.5 w-3.5" />
                </button>

                {/* Loop */}
                <button
                  onClick={() => setPlaybackLoop(!isLoop)}
                  aria-label="Toggle playback loop"
                  aria-pressed={isLoop}
                  title={isLoop ? 'Looping enabled' : 'Looping disabled'}
                  className={`flex items-center gap-1 rounded px-2 py-1 text-xs font-medium transition-colors ${
                    isLoop
                      ? 'bg-kiviAccent/15 text-kiviAccent border border-kiviAccent/40'
                      : 'text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk'
                  }`}
                >
                  <Repeat className="h-3.5 w-3.5" />
                  <span>Loop</span>
                </button>
              </div>

              {/* Speed Pills */}
              <div className="flex items-center gap-1">
                <span className="text-[10px] text-kiviMuted font-medium mr-1">Speed:</span>
                {([0.25, 0.5, 1.0, 2.0, 4.0] as const).map((speed) => (
                  <button
                    key={speed}
                    onClick={() => setPlaybackSpeed(speed)}
                    className={`rounded px-1.5 py-0.5 text-[10px] font-mono tabular-nums transition-colors ${
                      currentSpeed === speed
                        ? 'bg-kiviAccent text-kiviAccentFg font-bold shadow-xs'
                        : 'bg-kiviBg text-kiviMuted hover:text-kiviInk border border-kiviBorder'
                    }`}
                  >
                    {speed}x
                  </button>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* 3. DATALOGGER RECORDING TOOLBAR */}
        <div className="rounded-md border border-kiviBorder bg-kiviSurface/60 p-3 space-y-2.5">
          <div className="flex items-center justify-between gap-2">
            <span className="flex min-w-0 items-center gap-1.5 text-xs font-semibold text-kiviSecondary">
              <Disc className="h-3.5 w-3.5 shrink-0 text-kiviRed" />
              <span className="truncate">Datalogger Recording &amp; Export</span>
            </span>
            <span className="min-w-[118px] shrink-0 text-right font-mono text-[10px] tabular-nums whitespace-nowrap text-kiviMuted">
              {recordingStatus.sample_count} pts
              {recordingStatus.elapsed_ms > 0
                ? ` (${formatTimecode(recordingStatus.elapsed_ms)})`
                : ''}
            </span>
          </div>

          {/* Button Group */}
          <div className="flex flex-wrap items-center gap-2">
            {/* Hidden CSV file input */}
            <input
              type="file"
              ref={csvFileInputRef}
              onChange={handleImportCsvFile}
              accept=".csv"
              className="hidden"
            />

            {/* Record (F9) */}
            <button
              onClick={() => startRecording()}
              disabled={!toolbarStates.canRecord}
              className={`flex items-center gap-1.5 whitespace-nowrap rounded px-3 py-1 text-xs font-semibold transition-colors ${
                toolbarStates.canRecord
                  ? 'bg-kiviRed text-kiviAccentFg hover:opacity-90'
                  : 'bg-kiviBg text-kiviMuted border border-kiviBorder cursor-not-allowed opacity-50'
              }`}
            >
              <span className="h-2 w-2 shrink-0 rounded-full bg-current" />
              <span>Record (F9)</span>
            </button>

            {/* Stop Log */}
            <button
              onClick={() => stopRecording()}
              disabled={!toolbarStates.canStop}
              className={`flex items-center gap-1.5 whitespace-nowrap rounded px-3 py-1 text-xs font-semibold transition-colors ${
                toolbarStates.canStop
                  ? 'bg-kiviWarn text-kiviAccentFg hover:opacity-90'
                  : 'bg-kiviBg text-kiviMuted border border-kiviBorder cursor-not-allowed opacity-50'
              }`}
            >
              <Square className="h-3 w-3 shrink-0" />
              <span>Stop Log</span>
            </button>

            {/* Export CSV */}
            <button
              onClick={handleExportCsv}
              disabled={!toolbarStates.canExport}
              className={`flex items-center gap-1.5 whitespace-nowrap rounded px-3 py-1 text-xs font-semibold transition-colors ${
                toolbarStates.canExport
                  ? 'bg-kiviAccent text-kiviAccentFg hover:opacity-90'
                  : 'bg-kiviBg text-kiviMuted border border-kiviBorder cursor-not-allowed opacity-50'
              }`}
            >
              <Download className="h-3 w-3 shrink-0" />
              <span>Export CSV</span>
            </button>

            {/* Import CSV */}
            <button
              onClick={() => csvFileInputRef.current?.click()}
              disabled={!toolbarStates.canImport}
              className={`flex items-center gap-1.5 whitespace-nowrap rounded px-3 py-1 text-xs font-semibold transition-colors ${
                toolbarStates.canImport
                  ? 'border border-kiviBorderStrong bg-kiviElevated text-kiviInk hover:border-kiviAccent'
                  : 'bg-kiviBg text-kiviMuted border border-kiviBorder cursor-not-allowed opacity-50'
              }`}
            >
              <Upload className="h-3 w-3 shrink-0" />
              <span>Import CSV</span>
            </button>
          </div>

          {/* Data loss warning banner if dropped_count > 0 */}
          {toolbarStates.showDataLossWarning && (
            <div
              role="alert"
              className="flex items-center gap-2 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-3 py-2 text-xs text-kiviWarn"
            >
              <AlertTriangle className="h-4 w-4 text-kiviWarn shrink-0" />
              <span>
                Data Loss Warning: {recordingStatus.dropped_count} telemetry samples dropped due to buffer overrun or serial latency.
              </span>
            </div>
          )}
        </div>

        {/* 4. CONNECTION HEADER */}
        <div className="rounded-md border border-kiviBorder bg-kiviSurface/60 p-3 space-y-2.5">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2 flex-1 min-w-0">
              <label htmlFor="driver-select" className="text-[11px] font-medium text-kiviMuted shrink-0">
                Driver:
              </label>
              <select
                id="driver-select"
                value={selectedDriverId}
                onChange={(e) => selectDriver(e.target.value)}
                disabled={isConnected || isConnecting}
                className="w-full truncate rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-xs text-kiviInk focus:border-kiviAccent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent disabled:opacity-60"
              >
                {availableDrivers.length > 0 ? (
                  availableDrivers.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))
                ) : (
                  <option value="">No hardware serial adapters detected</option>
                )}
              </select>
            </div>

            <button
              onClick={handleToggleConnection}
              disabled={isConnecting || (!isConnected && !selectedDriverId)}
              className={`flex min-w-[108px] shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded px-3 py-1 text-xs font-semibold transition-colors disabled:opacity-50 ${
                isConnected
                  ? 'bg-kiviRed/10 text-kiviRed border border-kiviRed/40 hover:bg-kiviRed/20'
                  : 'bg-kiviGreen text-kiviAccentFg hover:opacity-90'
              }`}
            >
              <Power className="h-3.5 w-3.5 shrink-0" />
              <span>{isConnecting ? 'Connecting...' : isConnected ? 'Disconnect' : 'Connect'}</span>
            </button>
          </div>

          {(selectedDriverId.startsWith('obd2:') || selectedDriverId.startsWith('obd2_wifi:')) && (
            <div className="flex items-center gap-2 text-[11px]">
              <label htmlFor="driver-target-override" className="shrink-0 font-mono text-[10px] text-kiviMuted">
                OBD Target:
              </label>
              <input
                id="driver-target-override"
                type="text"
                disabled={isConnected || isConnecting}
                value={selectedDriverId}
                onChange={(e) => selectDriver(e.target.value)}
                placeholder="obd2_wifi:192.168.0.10:35000#auto or obd2:/dev/rfcomm0@38400#kwp_fast_init"
                className="w-full rounded border border-kiviBorder bg-kiviBg px-2 py-0.5 font-mono text-[11px] text-kiviInk disabled:opacity-60"
              />
            </div>
          )}

          <div className="flex items-center justify-between pt-1 text-[11px] border-t border-kiviBorder/60">
            <div className="flex items-center gap-2">
              <span className="text-kiviMuted">Status:</span>
              {isConnected ? (
                <span className="flex items-center gap-1.5 font-medium text-kiviGreen">
                  <span className="h-2 w-2 shrink-0 rounded-full bg-kiviGreen" />
                  <span>Connected ({identity?.ecu_id || 'ECU'})</span>
                </span>
              ) : isFaulted ? (
                <span className="flex items-center gap-1 font-medium text-kiviWarn">
                  <AlertTriangle className="h-3.5 w-3.5 text-kiviWarn shrink-0" />
                  <span>
                    Faulted ({connectionState.state === 'Faulted' ? connectionState.details.error : 'Error'})
                  </span>
                </span>
              ) : (
                <span className="flex items-center gap-1 font-medium text-kiviMuted">
                  <span className="h-2 w-2 shrink-0 rounded-full bg-kiviMuted" />
                  <span>Disconnected</span>
                </span>
              )}
            </div>

            {identity && (
              <span className="text-[10px] text-kiviMuted font-mono">
                {identity.protocol_name} | v{identity.software_version}
              </span>
            )}
          </div>
        </div>
      </div>
    </FloatingWindow>
  );
};
