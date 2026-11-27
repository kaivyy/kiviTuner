import React, { useState, useEffect, useMemo, useRef } from 'react';
import {
  GitCompare,
  Upload,
  Activity,
  PlusCircle,
  FileCheck,
  CheckCircle2,
} from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import type { DiffRegion } from '../../types/kivi';
import {
  bridgeCompareRomBinariesWithAddress,
  bridgeIngestFirmware,
  bridgeSaveBinRaw,
  bridgeVerifyRomAgainstReference,
  parseBaseAddressInput,
} from '../../lib/tauriBridge';
import { useMapStore } from '../../stores/useMapStore';
import {
  inspectVbf,
  canLoadAsRom,
  bytesForRom,
  exportRomAsVbf,
  describeSegment,
  VBF_LIMITATION_NOTICE,
  type VbfImportState,
} from './vbfWorkflow';
import {
  runRomVerification,
  describeVerification,
  type RomVerificationState,
} from './romVerification';

export interface BinDiffModalProps {
  isOpen: boolean;
  onClose: () => void;
  baseRom: Uint8Array | null;
  currentDiffs?: DiffRegion[];
  onDiffsChange?: (diffs: DiffRegion[]) => void;
  onJumpToWaveform: (startAddr: number, endAddr: number) => void;
  onExtractMap: (region: DiffRegion) => void;
  onExtractAllTables: (regions: DiffRegion[]) => void;
  /** Loads a single-segment VBF payload into the ROM workspace. */
  onVbfLoadRom?: (bytes: Uint8Array, baseAddress: number) => void;
  standalone?: boolean;
}

export type DiffCategoryFilter = 'all' | 'table_2d' | 'axis_1d' | 'scalar';

export const BinDiffModal: React.FC<BinDiffModalProps> = ({
  isOpen,
  onClose,
  baseRom,
  currentDiffs = [],
  onDiffsChange,
  onJumpToWaveform,
  onExtractMap,
  onExtractAllTables,
  onVbfLoadRom,
  standalone = false,
}) => {
  const [diffs, setDiffs] = useState<DiffRegion[]>(currentDiffs);
  const [filter, setFilter] = useState<DiffCategoryFilter>('all');
  const [isComparing, setIsComparing] = useState<boolean>(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  // Read-only ROM verification against a user-supplied reference backup.
  const [referenceRom, setReferenceRom] = useState<Uint8Array | null>(null);
  const [referenceName, setReferenceName] = useState<string | null>(null);
  const [verifyState, setVerifyState] = useState<RomVerificationState | null>(null);
  const [verifyPending, setVerifyPending] = useState<boolean>(false);
  const verifyRequestRef = useRef(0);
  const compareSeqRef = useRef(0);

  const referenceInputRef = useRef<HTMLInputElement>(null);

  // ---------------------------------------------------------------------
  // Workspace ROM base address (B4a / B4a.1)
  //
  // A raw .bin carries no address, so the workspace base starts unknown. The
  // user may state it here; stating nothing keeps it unknown rather than
  // pretending it is zero.
  //
  // The value lives in `useMapStore`, which owns the ROM. That is not a second
  // source of truth: the Rust comparison command takes the address as a
  // parameter and never reads `MemoryMap.base_address`, so there is nothing in
  // the backend to duplicate. It also gives the browser the same capability as
  // the desktop path instead of failing there.
  // ---------------------------------------------------------------------
  const storedBase = useMapStore((s) => s.romBaseAddress);
  const setStoredBase = useMapStore((s) => s.setRomBaseAddress);
  const romLoaded = useMapStore((s) => s.romLoaded);
  const romSize = useMapStore((s) => s.romSize);
  const hasBaseRom = romLoaded || Boolean(baseRom && baseRom.length > 0);

  // ---------------------------------------------------------------------
  // VBF container workflow (Wave 5)
  //
  // Inspection-first: a VBF is a multi-segment container while the ROM document
  // is a single flat buffer, so loading one silently would drop segment
  // addresses. The panel lists segments and only offers "load" for a
  // single-segment container.
  // ---------------------------------------------------------------------
  const [vbfState, setVbfState] = useState<VbfImportState | null>(null);
  const [vbfFileName, setVbfFileName] = useState<string | null>(null);
  const [vbfBusy, setVbfBusy] = useState<boolean>(false);
  const [vbfNotice, setVbfNotice] = useState<string | null>(null);
  const vbfInputRef = useRef<HTMLInputElement>(null);

  const [baseAddrText, setBaseAddrText] = useState<string>('');
  const [baseAddrError, setBaseAddrError] = useState<string | null>(null);

  // Reflect the stored address into the input whenever it changes (including
  // the automatic reset that `loadRomBuffer` performs on ROM replacement).
  useEffect(() => {
    setBaseAddrError(null);
    setBaseAddrText(storedBase === null ? '' : `0x${storedBase.toString(16).toUpperCase()}`);
  }, [storedBase, baseRom]);

  const handleApplyBaseAddress = () => {
    const parsed = parseBaseAddressInput(baseAddrText);
    if (!parsed.ok) {
      setBaseAddrError(parsed.error);
      return;
    }
    if (!hasBaseRom) {
      setBaseAddrError('Load a base ROM first.');
      return;
    }
    setBaseAddrError(null);
    setStoredBase(parsed.value);
  };

  const handleVbfSelected = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setVbfBusy(true);
    setVbfNotice(null);
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const state = await inspectVbf(bytes);
      setVbfState(state);
      setVbfFileName(file.name);
    } catch (err) {
      setVbfState({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      });
      setVbfFileName(file.name);
    } finally {
      setVbfBusy(false);
      e.target.value = '';
    }
  };

  const handleLoadVbfSegment = async () => {
    if (!vbfState || vbfState.kind !== 'ok') return;
    const bytes = bytesForRom(vbfState.report);
    if (!bytes) {
      setVbfNotice('This container has multiple segments; it cannot be loaded as a single ROM.');
      return;
    }
    onVbfLoadRom?.(bytes, vbfState.report.segments[0].start_address);
    setVbfNotice('Loaded the single segment into the ROM workspace.');
  };

  const handleExportVbf = async () => {
    const romForExport = baseRom ?? (romLoaded ? await bridgeSaveBinRaw(null) : null);
    const state = await exportRomAsVbf(romForExport, storedBase, 'KIVITUNER');
    if (state.kind === 'ok') {
      const blob = new Blob([state.bytes as BlobPart], { type: 'application/octet-stream' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'kivituner_export.vbf';
      a.click();
      URL.revokeObjectURL(url);
      setVbfNotice('Exported a single-segment VBF container.');
      return;
    }
    setVbfNotice(state.message);
  };

  // A reference belongs to the ROM it was chosen for. If the base ROM changes,
  // the previous comparison no longer describes anything on screen.
  useEffect(() => {
    setVerifyState(null);
    verifyRequestRef.current += 1;
  }, [baseRom]);

  const fileInputRef = useRef<HTMLInputElement>(null);

  // Sync external diffs if provided
  useEffect(() => {
    if (currentDiffs) {
      setDiffs(currentDiffs);
    }
  }, [currentDiffs]);

  // Compute summary metrics
  const summary = useMemo(() => {
    let changedBytes = 0;
    let maxDelta = 0;
    let sumDelta = 0;

    for (const r of diffs) {
      changedBytes += r.changed_byte_count;
      if (r.max_abs_delta > maxDelta) maxDelta = r.max_abs_delta;
      sumDelta += r.mean_delta * r.changed_byte_count;
    }

    const meanDelta = changedBytes > 0 ? sumDelta / changedBytes : 0;
    return {
      regionCount: diffs.length,
      changedBytes,
      maxDelta,
      meanDelta: Math.round(meanDelta * 10) / 10,
    };
  }, [diffs]);

  // Filtered diff regions
  const filteredDiffs = useMemo(() => {
    return diffs.filter((d) => {
      switch (filter) {
        case 'table_2d':
          return d.classification.type === 'Table2D';
        case 'axis_1d':
          return d.classification.type === 'Axis1D';
        case 'scalar':
          return d.classification.type === 'Scalar';
        case 'all':
        default:
          return true;
      }
    });
  }, [diffs, filter]);

  // Upload Compare ROM file
  const handleUploadCompareRom = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    if (!hasBaseRom) {
      setStatusMessage('Please load a Base ROM in the workspace first.');
      return;
    }

    const compareSeq = ++compareSeqRef.current;
    setIsComparing(true);
    setStatusMessage(`Reading ${file.name}...`);
    try {
      const buffer = await file.arrayBuffer();
      const rawBytes = new Uint8Array(buffer);

      // Intel HEX and S-Record files must be parsed into firmware bytes.
      // Comparing their ASCII text against the ROM would produce meaningless
      // diffs, so a parse failure is reported rather than silently compared.
      const ingested = await bridgeIngestFirmware(rawBytes);
      if (compareSeq !== compareSeqRef.current) return;
      if (!ingested) {
        // Rejecting must not leave a previous result on screen.
        setDiffs([]);
        onDiffsChange?.([]);
        setStatusMessage(`${file.name}: not a valid Intel HEX or S-Record file`);
        return;
      }
      const targetBytes = new Uint8Array(ingested.bytes);
      if (targetBytes.length === 0) {
        setDiffs([]);
        onDiffsChange?.([]);
        setStatusMessage(`${file.name}: file contains no firmware data`);
        return;
      }

      // Address gate: the workspace base comes from the user's stated address
      // (or stays null when unknown). A HEX/SREC target reports its lowest
      // address, so the two can only be compared when both are known and equal.
      const targetAddr = ingested.format === 'RawBinary' ? null : ingested.base_address;
      const outcome = await bridgeCompareRomBinariesWithAddress(
        baseRom,
        storedBase,
        targetBytes,
        targetAddr,
        4
      );
      if (compareSeq !== compareSeqRef.current) return;

      if (!outcome.ok) {
        // Clear stale results so the rejected inputs are never shown as a diff.
        setDiffs([]);
        onDiffsChange?.([]);
        setStatusMessage(`${file.name}: ${outcome.error ?? 'comparison rejected'}`);
        return;
      }

      const results = outcome.ok.regions;
      setDiffs(results);
      onDiffsChange?.(results);
      const formatLabel =
        ingested.format === 'RawBinary' ? '' : ` (${ingested.format})`;
      setStatusMessage(
        `Compared with ${file.name}${formatLabel}: found ${results.length} modified clusters`
      );
    } catch (err) {
      if (compareSeq !== compareSeqRef.current) return;
      console.error('Binary comparison failed:', err);
      setDiffs([]);
      onDiffsChange?.([]);
      setStatusMessage('Binary compare failed');
    } finally {
      if (compareSeq === compareSeqRef.current) setIsComparing(false);
      e.target.value = '';
    }
  };

  // Read-only verification: choose a reference backup and compare byte offsets.
  const handleReferenceSelected = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const requestId = ++verifyRequestRef.current;
    setVerifyPending(true);
    setVerifyState(null);

    try {
      const buffer = await file.arrayBuffer();
      const rawBytes = new Uint8Array(buffer);

      // A HEX/SREC reference must be decoded to firmware bytes first; comparing
      // its ASCII text against the ROM would report a difference on every byte.
      const ingested = await bridgeIngestFirmware(rawBytes);
      const referenceBytes = ingested ? new Uint8Array(ingested.bytes) : rawBytes;

      if (referenceBytes.length === 0) {
        if (requestId !== verifyRequestRef.current) return;
        setReferenceRom(null);
        setReferenceName(null);
        setVerifyState({
          kind: 'no-reference',
          message: `${file.name}: file contains no firmware data`,
        });
        return;
      }

      let result: RomVerificationState;
      if (baseRom && baseRom.length > 0) {
        result = await runRomVerification(baseRom, referenceBytes);
      } else if (romLoaded) {
        const rep = await bridgeVerifyRomAgainstReference(referenceBytes, null, 32);
        result = rep
          ? { kind: 'ok', report: rep }
          : { kind: 'error', message: 'ROM verification unavailable.' };
      } else {
        result = await runRomVerification(null, referenceBytes);
      }
      if (requestId !== verifyRequestRef.current) return;

      setReferenceRom(referenceBytes);
      setReferenceName(file.name);
      setVerifyState(result);
    } catch (err) {
      if (requestId !== verifyRequestRef.current) return;
      setReferenceRom(null);
      setReferenceName(null);
      setVerifyState({
        kind: 'error',
        message: err instanceof Error ? err.message : String(err),
      });
    } finally {
      if (requestId === verifyRequestRef.current) setVerifyPending(false);
      e.target.value = '';
    }
  };

  const handleReverify = async () => {
    if (!referenceRom) return;
    const requestId = ++verifyRequestRef.current;
    setVerifyPending(true);
    let result: RomVerificationState;
    if (baseRom && baseRom.length > 0) {
      result = await runRomVerification(baseRom, referenceRom);
    } else if (romLoaded) {
      const rep = await bridgeVerifyRomAgainstReference(referenceRom, null, 32);
      result = rep
        ? { kind: 'ok', report: rep }
        : { kind: 'error', message: 'ROM verification unavailable.' };
    } else {
      result = await runRomVerification(null, referenceRom);
    }
    if (requestId !== verifyRequestRef.current) return;
    setVerifyState(result);
    setVerifyPending(false);
  };

  // Batch extract all 2D tables to maps
  const handleExtractAll = () => {
    const tables = diffs.filter((d) => d.classification.type === 'Table2D');
    if (tables.length === 0) {
      setStatusMessage('No 2D table regions found to extract.');
      return;
    }
    onExtractAllTables(tables);
    setStatusMessage(`Extracted ${tables.length} tables to active map definitions!`);
  };

  const spec = TOOL_WINDOW_SPECS.diff;

  return (
    <FloatingWindow
      windowId="diff"
      isOpen={isOpen}
      onClose={onClose}
      title="Binary ROM Difference Analyzer"
      badge="Gap Cluster: 4B"
      icon={<GitCompare className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="diff"
      titlebarTone="sidebar"
      closeLabel="Close binary difference analyzer"
      footer={
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-kiviBorder bg-kiviSurface px-5 py-2.5 text-xs text-kiviMuted">
          <div className="flex items-center gap-2">
            <span>
              Base ROM:{' '}
              <strong className="text-kiviInk">
                {hasBaseRom
                  ? `${(baseRom?.length ?? romSize).toLocaleString()} bytes`
                  : 'None loaded'}
              </strong>
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-kiviElevated px-4 py-1.5 text-xs font-medium text-kiviInk hover:bg-kiviBorderStrong transition-colors"
          >
            Dismiss
          </button>
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col select-none overflow-hidden">
        <div className="shrink-0 border-b border-kiviBorder/50 bg-kiviBg px-5 py-1.5 text-[11px] text-kiviMuted">
          Identify modified calibration tables, axes, and scalar parameters between two ROMs.
        </div>

        {/* Hidden File Input for Compare ROM */}
        <input
          type="file"
          ref={fileInputRef}
          onChange={handleUploadCompareRom}
          accept=".bin,.rom,.hex,.srec,.s19,.mot"
          className="hidden"
        />

        {/* Hidden File Input for Reference ROM (read-only verification) */}
        <input
          type="file"
          ref={referenceInputRef}
          onChange={handleReferenceSelected}
          accept=".bin,.rom,.hex,.srec,.s19"
          className="hidden"
        />

        {/* Hidden input for VBF container selection */}
        <input
          type="file"
          ref={vbfInputRef}
          onChange={handleVbfSelected}
          accept=".vbf"
          className="hidden"
        />

        {/* VBF container panel (Wave 5) */}
        <div className="border-b border-kiviBorder/50 bg-kiviBg px-5 py-2 text-[11px]">
          <div className="flex flex-wrap items-center gap-3">
            <span className="font-semibold uppercase tracking-wide text-kiviMuted">VBF</span>
            <button
              type="button"
              aria-label="Open VBF container"
              disabled={vbfBusy}
              onClick={() => vbfInputRef.current?.click()}
              className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-kiviSecondary hover:bg-kiviElevated disabled:opacity-40 transition-colors"
            >
              {vbfBusy ? 'Reading…' : 'Open .vbf…'}
            </button>
            <button
              type="button"
              aria-label="Export ROM as VBF"
              disabled={!hasBaseRom}
              onClick={() => void handleExportVbf()}
              className="rounded border border-kiviBorder bg-kiviElevated px-2 py-1 text-kiviInk hover:bg-kiviBorder/40 disabled:opacity-40"
            >
              Export ROM as VBF
            </button>
            {vbfFileName && <span className="text-kiviMuted">{vbfFileName}</span>}
          </div>

          <p className="mt-1 text-[10px] leading-tight text-kiviMuted">{VBF_LIMITATION_NOTICE}</p>

          {vbfState?.kind === 'no-backend' && (
            <p className="mt-1 text-[10px] text-kiviWarn">{vbfState.message}</p>
          )}
          {vbfState?.kind === 'empty' && (
            <p className="mt-1 text-[10px] text-kiviWarn">{vbfState.message}</p>
          )}
          {vbfState?.kind === 'error' && (
            <p className="mt-1 text-[10px] text-kiviRed">{vbfState.message}</p>
          )}
          {vbfState?.kind === 'ok' && (
            <div className="mt-1 space-y-1" aria-live="polite">
              <div className="text-[10px] text-kiviMuted">
                {vbfState.report.segments.length} segment
                {vbfState.report.segments.length === 1 ? '' : 's'}
              </div>
              <div className="max-h-28 overflow-y-auto space-y-0.5">
                {vbfState.report.segments.map((seg, i) => (
                  <div
                    key={`vbf-${i}-${seg.start_address}`}
                    className="font-mono text-[10px] text-kiviSecondary"
                  >
                    {describeSegment(seg)}
                  </div>
                ))}
              </div>
              {canLoadAsRom(vbfState.report) ? (
                <button
                  type="button"
                  aria-label="Load VBF segment as ROM"
                  onClick={() => void handleLoadVbfSegment()}
                  className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2 py-0.5 text-kiviGreen hover:bg-kiviGreen/20"
                >
                  Load segment as ROM
                </button>
              ) : (
                <p className="text-[10px] text-kiviMuted">
                  Multi-segment container - loading it as a single ROM would discard
                  segment addresses, so it is inspection-only.
                </p>
              )}
            </div>
          )}
          {vbfNotice && <p className="mt-1 text-[10px] text-kiviAccent">{vbfNotice}</p>}
        </div>

        {/* Workspace ROM base address (B4a) */}
        <div className="flex flex-wrap items-center gap-3 border-b border-kiviBorder/50 bg-kiviBg px-5 py-2 text-[11px]">
          <span className="font-semibold uppercase tracking-wide text-kiviMuted">Base address</span>
          <label className="flex items-center gap-1.5 text-kiviMuted">
            <span>Workspace ROM</span>
            <input
              type="text"
              aria-label="Workspace ROM base address"
              value={baseAddrText}
              onChange={(e) => {
                setBaseAddrText(e.target.value);
                setBaseAddrError(null);
              }}
              placeholder="unknown"
              disabled={!hasBaseRom}
              className="w-28 rounded border border-kiviBorder bg-kiviSurface px-1.5 py-0.5 font-mono text-kiviInk placeholder:text-kiviMuted disabled:opacity-40"
            />
          </label>
          <button
            type="button"
            onClick={() => void handleApplyBaseAddress()}
            disabled={!hasBaseRom}
            className="rounded border border-kiviBorder bg-kiviElevated px-2 py-0.5 text-kiviInk hover:bg-kiviBorder/40 disabled:opacity-40"
          >
            Apply
          </button>
          <span className="font-mono text-kiviMuted" aria-live="polite">
            {!hasBaseRom && 'Load a base ROM first.'}
            {hasBaseRom && baseAddrError && (
              <span className="text-kiviRed">{baseAddrError}</span>
            )}
            {hasBaseRom && !baseAddrError && (
              <>
                {storedBase === null
                  ? 'unknown - leave empty to keep it unknown'
                  : `known: 0x${storedBase.toString(16).toUpperCase()}`}
              </>
            )}
          </span>
        </div>

        {/* Read-only verification bar */}
        <div className="flex flex-wrap items-center gap-3 border-b border-kiviBorder/50 bg-kiviBg px-5 py-2 text-[11px]">
          <span className="font-semibold uppercase tracking-wide text-kiviMuted">Verify</span>
          <button
            type="button"
            disabled={verifyPending || !hasBaseRom}
            onClick={() => referenceInputRef.current?.click()}
            className="flex items-center gap-1.5 rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-kiviSecondary hover:bg-kiviElevated disabled:opacity-40 transition-colors"
            title="Compare the active ROM against a reference backup"
          >
            <FileCheck className="h-3.5 w-3.5 text-kiviMuted" />
            <span aria-label="Verify against reference">Choose reference...</span>
          </button>
          {referenceRom && (
            <button
              type="button"
              disabled={verifyPending}
              onClick={() => void handleReverify()}
              className="min-w-[84px] shrink-0 whitespace-nowrap rounded border border-kiviBorder bg-kiviElevated px-2 py-1 text-center text-kiviInk hover:bg-kiviBorder/40 disabled:opacity-40"
            >
              {verifyPending ? 'Verifying...' : 'Re-verify'}
            </button>
          )}
          <span className="font-mono text-kiviMuted" aria-live="polite">
            {!hasBaseRom && 'Load a base ROM first.'}
            {hasBaseRom && !referenceRom && !verifyState && 'No reference selected.'}
            {referenceName && <span className="text-kiviMuted">{referenceName}: </span>}
            {verifyState?.kind === 'no-active-rom' && verifyState.message}
            {verifyState?.kind === 'no-reference' && verifyState.message}
            {verifyState?.kind === 'error' && verifyState.message}
            {verifyState?.kind === 'ok' && describeVerification(verifyState.report)}
          </span>
        </div>

        {/* Action Controls */}
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-kiviBorder/50 bg-kiviBg px-5 py-2.5">
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={isComparing}
              onClick={() => fileInputRef.current?.click()}
              className="flex items-center gap-1.5 rounded-lg border border-kiviBorder bg-kiviSurface px-3 py-1.5 text-xs font-medium text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk disabled:opacity-50 transition-colors"
              title="Upload modified ROM file (.bin, .rom)"
            >
              <Upload className="h-3.5 w-3.5 text-kiviMuted" />
              <span>Upload Compare ROM...</span>
            </button>

            {diffs.some((d) => d.classification.type === 'Table2D') && (
              <button
                type="button"
                onClick={handleExtractAll}
                className="flex items-center gap-1.5 rounded-lg border border-kiviGreen/40 bg-kiviGreen/10 px-3 py-1.5 text-xs font-semibold text-kiviGreen hover:bg-kiviGreen/20 transition-colors"
                title="Extract all discovered 2D tables into active map list"
              >
                <PlusCircle className="h-3.5 w-3.5" />
                <span>Extract All Tables to Maps</span>
              </button>
            )}
          </div>

          {statusMessage && (
            <div className="flex items-center gap-1.5 text-xs text-kiviAccent font-mono">
              <CheckCircle2 className="h-3.5 w-3.5" />
              <span>{statusMessage}</span>
            </div>
          )}
        </div>

        {/* Statistical Summary Bar */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 border-b border-kiviBorder bg-kiviSurface p-4">
          <div className="rounded-lg border border-kiviBorder bg-kiviSurface/60 p-3">
            <span className="text-[11px] font-medium text-kiviMuted block uppercase tracking-wider">
              Changed Bytes
            </span>
            <span className="mt-1 text-xl font-bold font-mono text-kiviInk">
              {summary.changedBytes.toLocaleString()}{' '}
              <span className="text-xs font-normal text-kiviMuted">bytes</span>
            </span>
          </div>

          <div className="rounded-lg border border-kiviBorder bg-kiviSurface/60 p-3">
            <span className="text-[11px] font-medium text-kiviMuted block uppercase tracking-wider">
              Diff Clusters
            </span>
            <span className="mt-1 text-xl font-bold font-mono text-kiviAccent">
              {summary.regionCount}{' '}
              <span className="text-xs font-normal text-kiviMuted">regions</span>
            </span>
          </div>

          <div className="rounded-lg border border-kiviBorder bg-kiviSurface/60 p-3">
            <span className="text-[11px] font-medium text-kiviMuted block uppercase tracking-wider">
              Max Delta (Abs)
            </span>
            <span className="mt-1 text-xl font-bold font-mono text-kiviWarn">
              &plusmn;{summary.maxDelta}
            </span>
          </div>

          <div className="rounded-lg border border-kiviBorder bg-kiviSurface/60 p-3">
            <span className="text-[11px] font-medium text-kiviMuted block uppercase tracking-wider">
              Mean Delta
            </span>
            <span className="mt-1 text-xl font-bold font-mono text-kiviGreen">
              {summary.meanDelta >= 0 ? `+${summary.meanDelta}` : summary.meanDelta}
            </span>
          </div>
        </div>

        {/* Category Filter Tabs */}
        <div className="flex items-center justify-between border-b border-kiviBorder bg-kiviSurface px-5 py-2">
          <div className="flex items-center gap-1 rounded-lg border border-kiviBorder bg-kiviSurface p-0.5 text-xs">
            <button
              type="button"
              onClick={() => setFilter('all')}
              className={`rounded-md px-3 py-1 font-medium transition-colors ${
                filter === 'all'
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              All Regions ({diffs.length})
            </button>
            <button
              type="button"
              onClick={() => setFilter('table_2d')}
              className={`rounded-md px-3 py-1 font-medium transition-colors ${
                filter === 'table_2d'
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              Tables 2D ({diffs.filter((d) => d.classification.type === 'Table2D').length})
            </button>
            <button
              type="button"
              onClick={() => setFilter('axis_1d')}
              className={`rounded-md px-3 py-1 font-medium transition-colors ${
                filter === 'axis_1d'
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              Axes 1D ({diffs.filter((d) => d.classification.type === 'Axis1D').length})
            </button>
            <button
              type="button"
              onClick={() => setFilter('scalar')}
              className={`rounded-md px-3 py-1 font-medium transition-colors ${
                filter === 'scalar'
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              Scalars ({diffs.filter((d) => d.classification.type === 'Scalar').length})
            </button>
          </div>

          <span className="text-xs text-kiviMuted">
            Showing <strong className="text-kiviInk">{filteredDiffs.length}</strong> of{' '}
            {diffs.length} regions
          </span>
        </div>

        {/* Diff Regions List Table */}
        <div className="flex-1 overflow-auto p-4">
          {filteredDiffs.length === 0 ? (
            <div className="flex h-64 flex-col items-center justify-center text-center text-kiviMuted">
              <FileCheck className="h-10 w-10 text-kiviMuted mb-2" />
              <p className="text-sm font-medium text-kiviSecondary">No Difference Regions</p>
              <p className="text-xs text-kiviMuted max-w-sm mt-1">
                Upload a modified ROM binary (.bin, .rom) to compute real byte-level diff clusters
                against the loaded Base ROM.
              </p>
            </div>
          ) : (
            <table className="w-full border-collapse text-left text-xs">
              <thead>
                <tr className="border-b border-kiviBorder text-[11px] font-semibold text-kiviMuted uppercase tracking-wider">
                  <th className="pb-2 pl-2">ID</th>
                  <th className="pb-2">Address Range</th>
                  <th className="pb-2">Size</th>
                  <th className="pb-2">Classification</th>
                  <th className="pb-2">Max Delta</th>
                  <th className="pb-2">Mean Delta</th>
                  <th className="pb-2 text-right pr-2">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-kiviBorder font-mono">
                {filteredDiffs.map((region) => {
                  const startHex = `0x${region.start_addr.toString(16).toUpperCase().padStart(4, '0')}`;
                  const endHex = `0x${(region.end_addr - 1).toString(16).toUpperCase().padStart(4, '0')}`;

                  return (
                    <tr
                      key={region.id}
                      className="hover:bg-kiviElevated transition-colors group"
                    >
                      <td className="py-2.5 pl-2 font-semibold text-kiviSecondary">{region.id}</td>
                      <td className="py-2.5 text-kiviInk">
                        {startHex} &ndash; {endHex}
                      </td>
                      <td className="py-2.5 text-kiviSecondary">
                        {region.byte_len} B{' '}
                        <span className="text-[10px] text-kiviMuted">
                          ({region.changed_byte_count} changed)
                        </span>
                      </td>
                      <td className="py-2.5">
                        {region.classification.type === 'Table2D' ? (
                          <span className="inline-flex items-center gap-1 rounded bg-kiviAccent/15 px-2 py-0.5 text-[11px] font-semibold text-kiviAccent border border-kiviAccent/40">
                            Table {region.classification.data.suggested_rows}&times;
                            {region.classification.data.suggested_cols}
                          </span>
                        ) : region.classification.type === 'Axis1D' ? (
                          <span className="inline-flex items-center gap-1 rounded bg-kiviWarn/10 px-2 py-0.5 text-[11px] font-semibold text-kiviWarn border border-kiviWarn/40">
                            Axis ({region.classification.data.length})
                          </span>
                        ) : region.classification.type === 'Scalar' ? (
                          <span className="inline-flex items-center gap-1 rounded bg-kiviGreen/10 px-2 py-0.5 text-[11px] font-semibold text-kiviGreen border border-kiviGreen/40">
                            Scalar (1x1)
                          </span>
                        ) : (
                          <span className="inline-flex items-center gap-1 rounded bg-kiviElevated px-2 py-0.5 text-[11px] font-semibold text-kiviMuted border border-kiviBorder">
                            Raw Block
                          </span>
                        )}
                      </td>
                      <td className="py-2.5 text-kiviWarn font-semibold">
                        &plusmn;{region.max_abs_delta}
                      </td>
                      <td className="py-2.5 text-kiviGreen">
                        {region.mean_delta >= 0 ? `+${region.mean_delta}` : region.mean_delta}
                      </td>
                      <td className="py-2.5 pr-2 text-right">
                        <div className="flex items-center justify-end gap-1.5">
                          <button
                            type="button"
                            onClick={() => {
                              onJumpToWaveform(region.start_addr, region.end_addr);
                              onClose();
                            }}
                            className="flex items-center gap-1 rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-[11px] text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
                            title="Jump to region in 2D Waveform Inspector"
                          >
                            <Activity className="h-3 w-3 text-kiviAccent" />
                            <span>Waveform</span>
                          </button>
                          <button
                            type="button"
                            onClick={() => onExtractMap(region)}
                            className="flex items-center gap-1 rounded bg-kiviAccent/15 hover:bg-kiviAccent/15 text-kiviAccent border border-kiviAccent/40 px-2 py-1 text-[11px] font-medium transition-colors"
                            title="Extract region into new Map Definition"
                          >
                            <PlusCircle className="h-3 w-3" />
                            <span>+ Extract Map</span>
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </FloatingWindow>
  );
};

export default BinDiffModal;
