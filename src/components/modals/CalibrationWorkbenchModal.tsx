/**
 * Multi-ECU Calibration Workbench Modal.
 *
 * Exposes native calibration workbench engines in five honest,
 * state-driven tabs delegated to focused subcomponents under `./workbench/`.
 */

import React, { useEffect, useState } from 'react';
import { Activity, Calculator, Cpu, FileCode, GitCompare, ShieldCheck, Sliders } from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import type { SelectionRange } from '../editor';
import type { MapDef } from '../../types/kivi';
import type {
  A2lImportSummary,
  ChecksumEngineInfo,
  DamosImportSummary,
  DetectedChecksumMatch,
  FactorOffsetSolution,
  PatchDef,
  RegionInterpAxes,
  RegionInterpMode,
  SimilarMapHit,
  SimilarMapSpec,
  XdfFullDefinitions,
} from '../../lib/workbenchBridge';
import { mapDtypeToBitsSigned } from '../../lib/workbenchBridge';
import {
  bridgeApplyXdfPatchWorkspace,
  bridgeBlindAutomapV2,
  bridgeClassifyBlindTables,
  bridgeFindMigratedMapsWorkspaceRaw,
  bridgeRepairChecksumWorkspace,
} from '../../lib/workbenchBridge';
import { useMapStore } from '../../stores/useMapStore';
import {
  parseAddressInput,
  runAutoDetectChecksums,
  runBlindV2WithClassification,
  runFactorOffsetSolve,
  runImportA2l,
  runImportDamos,
  runImportXdfFull,
  runListChecksumEngines,
  runRegionInterpolation,
  runRepairChecksum,
  runSelectionEdit,
  runSimilaritySearch,
  runXdfPatchToggle,
  type ClassifiedBlindEntry,
  type WorkbenchState,
} from './calibrationWorkbenchController';
import { BlindScanTab } from './workbench/BlindScanTab';
import { DefinitionsTab, type DefFormat } from './workbench/DefinitionsTab';
import { SimilarityTab } from './workbench/SimilarityTab';
import { ChecksumsTab } from './workbench/ChecksumsTab';
import { EcuToolsTab } from './workbench/EcuToolsTab';
import { MatrixEditSolverTab, type EditOpKind } from './workbench/MatrixEditSolverTab';

export interface CalibrationWorkbenchModalProps {
  isOpen: boolean;
  onClose: () => void;
  rom: Uint8Array | null;
  romLoaded?: boolean;
  romSize?: number;
  maps: MapDef[];
  selectedMap: MapDef | null;
  activeMatrix: number[][] | null;
  selection: SelectionRange | null;
  onAddMaps: (newMaps: MapDef[], selectFirst?: boolean) => void;
  onSelectMap: (mapId: string) => void;
  onRomUpdated: (nextRom: Uint8Array, checksumOk?: boolean) => Promise<void> | void;
  onMatrixUpdated: (mapDef: MapDef, nextMatrix: number[][]) => Promise<void> | void;
  onUpdateMapScaling: (mapId: string, equation: string) => void;
  standalone?: boolean;
}

type WorkbenchTab = 'blind' | 'defs' | 'similar' | 'checksum' | 'edit' | 'tools';

function FeedbackBanner({
  state,
  infoMessage,
}: {
  state: WorkbenchState<unknown> | null;
  infoMessage: string | null;
}) {
  if (state?.kind === 'no-backend') {
    return (
      <div className="rounded border border-kiviWarn/40 bg-kiviWarn/10 px-3 py-1.5 text-[11px] text-kiviWarn">
        {state.message}
      </div>
    );
  }
  if (state?.kind === 'error') {
    return (
      <div className="rounded border border-kiviRed/40 bg-kiviRed/10 px-3 py-1.5 text-[11px] text-kiviRed">
        {state.message}
      </div>
    );
  }
  if (infoMessage) {
    return (
      <div className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-3 py-1.5 text-[11px] text-kiviGreen">
        {infoMessage}
      </div>
    );
  }
  return null;
}

export const CalibrationWorkbenchModal: React.FC<CalibrationWorkbenchModalProps> = ({
  isOpen,
  onClose,
  rom,
  romLoaded: propRomLoaded,
  romSize: propRomSize,
  maps: _maps,
  selectedMap,
  activeMatrix,
  selection,
  onAddMaps,
  onSelectMap,
  onRomUpdated,
  onMatrixUpdated,
  onUpdateMapScaling,
  standalone = false,
}) => {
  const storeRomLoaded = useMapStore((s) => s.romLoaded);
  const storeRomSize = useMapStore((s) => s.romSize);
  const refreshRomSession = useMapStore((s) => s.refreshRomSession);
  const romLoaded = Boolean(propRomLoaded || storeRomLoaded);
  const effectiveRomSize = propRomSize ?? storeRomSize ?? rom?.length ?? 0;
  const hasRom = romLoaded || Boolean(rom && rom.length > 0);
  const opSeqRef = React.useRef(0);
  const [tab, setTab] = useState<WorkbenchTab>('blind');
  const [busy, setBusy] = useState(false);
  const [bannerState, setBannerState] = useState<WorkbenchState<unknown> | null>(null);
  const [infoMessage, setInfoMessage] = useState<string | null>(null);

  const [includeCurves, setIncludeCurves] = useState(true);
  const [include8bit, setInclude8bit] = useState(true);
  const [include32bit, setInclude32bit] = useState(true);
  const [maxTables, setMaxTables] = useState(128);
  const [blindResults, setBlindResults] = useState<ClassifiedBlindEntry[]>([]);

  const [defFormat, setDefFormat] = useState<DefFormat>('a2l');
  const [defText, setDefText] = useState('');
  const [baseOffsetText, setBaseOffsetText] = useState('0x0');
  const [defFilter, setDefFilter] = useState('');
  const [a2lSummary, setA2lSummary] = useState<A2lImportSummary | null>(null);
  const [damosSummary, setDamosSummary] = useState<DamosImportSummary | null>(null);
  const [xdfSummary, setXdfSummary] = useState<XdfFullDefinitions | null>(null);

  const [simMode, setSimMode] = useState<'in-rom' | 'cross-rom'>('in-rom');
  const [simAddrText, setSimAddrText] = useState('0x0');
  const [simRows, setSimRows] = useState(8);
  const [simCols, setSimCols] = useState(8);
  const [simBits, setSimBits] = useState(16);
  const [simLittleEndian, setSimLittleEndian] = useState(true);
  const [simSigned, setSimSigned] = useState(false);
  const [minPearsonR, setMinPearsonR] = useState(0.9);
  const [maxNrmse, setMaxNrmse] = useState(0.2);
  const [targetRom, setTargetRom] = useState<Uint8Array | null>(null);
  const [targetRomName, setTargetRomName] = useState<string | null>(null);
  const [simHits, setSimHits] = useState<SimilarMapHit[]>([]);

  const [engines, setEngines] = useState<ChecksumEngineInfo[]>([]);
  const [checksumMatches, setChecksumMatches] = useState<DetectedChecksumMatch[]>([]);

  const [editOpKind, setEditOpKind] = useState<EditOpKind>('AddDelta');
  const [editValue, setEditValue] = useState('1');
  const [clampMin, setClampMin] = useState('0');
  const [clampMax, setClampMax] = useState('255');
  const [interpMode, setInterpMode] = useState<RegionInterpMode>('Linear');
  const [interpAxes, setInterpAxes] = useState<RegionInterpAxes>('AllFourSides');
  const [allowExtrapolation, setAllowExtrapolation] = useState(false);
  const [p1x, setP1x] = useState('0');
  const [p1y, setP1y] = useState('0');
  const [p2x, setP2x] = useState('100');
  const [p2y, setP2y] = useState('10');
  const [useThirdPoint, setUseThirdPoint] = useState(false);
  const [p3x, setP3x] = useState('200');
  const [p3y, setP3y] = useState('15');
  const [swapInputOutput, setSwapInputOutput] = useState(false);
  const [solverResult, setSolverResult] = useState<FactorOffsetSolution | null>(null);

  useEffect(() => {
    if (!selectedMap) return;
    setSimAddrText(`0x${selectedMap.address.toString(16).toUpperCase()}`);
    setSimRows(selectedMap.rows);
    setSimCols(selectedMap.cols);
    const { bits, signed } = mapDtypeToBitsSigned(selectedMap.dtype);
    setSimBits(bits);
    setSimSigned(signed);
    setSimLittleEndian(selectedMap.endian === 'little');
  }, [selectedMap]);

  useEffect(() => {
    if (!isOpen) return;
    void (async () => {
      const res = await runListChecksumEngines();
      if (res.kind === 'ok') setEngines(res.data);
    })();
  }, [isOpen]);

  if (!isOpen) return null;

  const currentSelectionBox = () => {
    if (selection) {
      return {
        row_start: Math.min(selection.minR, selection.maxR),
        row_end: Math.max(selection.minR, selection.maxR),
        col_start: Math.min(selection.minC, selection.maxC),
        col_end: Math.max(selection.minC, selection.maxC),
      };
    }
    return {
      row_start: 0,
      row_end: Math.max(0, (activeMatrix?.length ?? 1) - 1),
      col_start: 0,
      col_end: Math.max(0, (activeMatrix?.[0]?.length ?? 1) - 1),
    };
  };

  const handleRunBlindV2 = async () => {
    const seq = ++opSeqRef.current;
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      const res = await runBlindV2WithClassification(
        rom,
        {
          includeCurves,
          include8bit,
          include32bit,
          maxTables,
        },
        bridgeBlindAutomapV2,
        bridgeClassifyBlindTables,
        hasRom
      );
      if (seq !== opSeqRef.current) return;
      setBannerState(res);
      if (res.kind === 'ok') {
        setBlindResults(res.data);
        setInfoMessage(`Blind V2 scan completed: ${res.data.length} non-overlapping candidates found.`);
      }
    } finally {
      if (seq === opSeqRef.current) setBusy(false);
    }
  };

  const handleParseDef = async () => {
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      if (defFormat === 'a2l') {
        const res = await runImportA2l(defText);
        setBannerState(res);
        if (res.kind === 'ok') {
          setA2lSummary(res.data);
          setDamosSummary(null);
          setXdfSummary(null);
          if (res.data.ecu_base_suggestions.length > 0) {
            setBaseOffsetText(`0x${res.data.ecu_base_suggestions[0].toString(16).toUpperCase()}`);
          }
          setInfoMessage(
            `Parsed A2L: ${res.data.characteristic_count} characteristics (${res.data.map_count} maps, ${res.data.curve_count} curves).`
          );
        }
      } else if (defFormat === 'damos') {
        const res = await runImportDamos(defText);
        setBannerState(res);
        if (res.kind === 'ok') {
          setDamosSummary(res.data);
          setA2lSummary(null);
          setXdfSummary(null);
          if (res.data.base_address !== null) {
            setBaseOffsetText(`0x${res.data.base_address.toString(16).toUpperCase()}`);
          }
          setInfoMessage(
            `Parsed DAMOS: ${res.data.spz_total_count} SPZ objects, ${res.data.ump_count} UMP tables.`
          );
        }
      } else {
        const res = await runImportXdfFull(defText);
        setBannerState(res);
        if (res.kind === 'ok') {
          setXdfSummary(res.data);
          setA2lSummary(null);
          setDamosSummary(null);
          setInfoMessage(
            `Parsed XDF Full: ${res.data.maps.length} maps, ${res.data.patches.length} patches, ${res.data.checksums.length} checksums.`
          );
        }
      }
    } finally {
      setBusy(false);
    }
  };

  const handleTogglePatch = async (patch: PatchDef, revert: boolean) => {
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      if (!rom && romLoaded) {
        const res = await bridgeApplyXdfPatchWorkspace(patch, revert);
        if (!res) {
          setBannerState({ kind: 'no-backend', message: 'Requires the kiviTuner desktop application.' });
          return;
        }
        await refreshRomSession();
        setInfoMessage(
          `Patch "${patch.name}" ${revert ? 'reverted' : 'applied'} (${JSON.stringify(res.status_after)}).`
        );
        return;
      }
      const res = await runXdfPatchToggle(rom, patch, revert);
      setBannerState(res);
      if (res.kind === 'ok') {
        await onRomUpdated(new Uint8Array(res.data.rom));
        setInfoMessage(
          `Patch "${patch.name}" ${revert ? 'reverted' : 'applied'} (${JSON.stringify(
            res.data.status_after
          )}).`
        );
      }
    } catch (err) {
      setBannerState({ kind: 'error', message: String(err) });
    } finally {
      setBusy(false);
    }
  };

  const handleRunSimilarity = async () => {
    const addr = parseAddressInput(simAddrText);
    if (addr === null) {
      setBannerState({ kind: 'error', message: 'Enter a valid source map hex or decimal address.' });
      return;
    }
    const spec: SimilarMapSpec = {
      address: addr,
      rows: simRows,
      cols: simCols,
      cell_bits: simBits,
      little_endian: simLittleEndian,
      is_signed: simSigned,
    };
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      if (!rom && romLoaded && simMode === 'cross-rom') {
        if (!targetRom || targetRom.length === 0) {
          setBannerState({
            kind: 'error',
            message: 'Load a target ROM binary for cross-ROM map migration.',
          });
          return;
        }
        const hits = await bridgeFindMigratedMapsWorkspaceRaw(targetRom, spec, {
          min_pearson_r: minPearsonR,
          max_nrmse: maxNrmse,
          max_rel_diff: null,
          stride_bytes: 0,
          exclude_overlapping_source: true,
          max_results: 64,
        });
        if (!hits) {
          setBannerState({ kind: 'no-backend', message: 'Requires the kiviTuner desktop application.' });
          return;
        }
        setSimHits(hits);
        setInfoMessage(`Found ${hits.length} migrated region(s).`);
        return;
      }
      const res = await runSimilaritySearch(simMode, rom ?? new Uint8Array(hasRom ? 1 : 0), targetRom, spec, {
        min_pearson_r: minPearsonR,
        max_nrmse: maxNrmse,
        max_rel_diff: null,
        stride_bytes: 0,
        exclude_overlapping_source: true,
        max_results: 64,
      });
      setBannerState(res);
      if (res.kind === 'ok') {
        setSimHits(res.data);
        setInfoMessage(`Found ${res.data.length} matching region(s).`);
      }
    } catch (err) {
      setBannerState({ kind: 'error', message: String(err) });
    } finally {
      setBusy(false);
    }
  };

  const handleDetectChecksums = async () => {
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      const res = await runAutoDetectChecksums(rom, undefined, hasRom);
      setBannerState(res);
      if (res.kind === 'ok') {
        setChecksumMatches(res.data);
        setInfoMessage(
          res.data.length === 0
            ? 'No registered moto ECU checksum layout matched this ROM buffer.'
            : `Detected ${res.data.length} matching checksum layout(s).`
        );
      }
    } finally {
      setBusy(false);
    }
  };

  const handleRepairChecksum = async (
    algorithmId: string,
    mode: 'UpdateChecksum' | 'CompensateKeepCrc'
  ) => {
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      if (!rom && romLoaded) {
        const wsRes = await bridgeRepairChecksumWorkspace(algorithmId, mode);
        if (!wsRes) {
          setBannerState({ kind: 'no-backend', message: 'Requires the kiviTuner desktop application.' });
          return;
        }
        await refreshRomSession();
        useMapStore.getState().setChecksumStatus(wsRes.match_info.all_ok ? 'valid' : 'invalid');
        setChecksumMatches((prev) =>
          prev.map((m) => (m.algorithm_id === algorithmId ? wsRes.match_info : m))
        );
        setInfoMessage(
          `${wsRes.match_info.algorithm_name} repaired via ${mode} (${
            wsRes.match_info.all_ok ? 'ALL OK' : 'CHECK BLOCKS'
          }).`
        );
        return;
      }
      const res = await runRepairChecksum(rom, algorithmId, mode);
      setBannerState(res);
      if (res.kind === 'ok') {
        await onRomUpdated(new Uint8Array(res.data.rom), res.data.match_info.all_ok);
        setChecksumMatches((prev) =>
          prev.map((m) => (m.algorithm_id === algorithmId ? res.data.match_info : m))
        );
        setInfoMessage(
          `${res.data.match_info.algorithm_name} repaired via ${mode} (${
            res.data.match_info.all_ok ? 'ALL OK' : 'CHECK BLOCKS'
          }).`
        );
      }
    } catch (err) {
      setBannerState({ kind: 'error', message: String(err) });
    } finally {
      setBusy(false);
    }
  };

  const handleSelectionEdit = async () => {
    if (!selectedMap) {
      setBannerState({ kind: 'error', message: 'Select a map in the sidebar first.' });
      return;
    }
    const v = Number.parseFloat(editValue);
    const min = Number.parseFloat(clampMin);
    const max = Number.parseFloat(clampMax);
    const op =
      editOpKind === 'SetAbsolute'
        ? { SetAbsolute: v }
        : editOpKind === 'AddDelta'
        ? { AddDelta: v }
        : editOpKind === 'ScalePercent'
        ? { ScalePercent: v }
        : { ClampRange: { min, max } };
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      const res = await runSelectionEdit(activeMatrix, currentSelectionBox(), op);
      setBannerState(res);
      if (res.kind === 'ok') {
        await onMatrixUpdated(selectedMap, res.data);
        setInfoMessage(`Applied ${editOpKind} to selected cells of ${selectedMap.name}.`);
      }
    } finally {
      setBusy(false);
    }
  };

  const handleInterpolate = async () => {
    if (!selectedMap) {
      setBannerState({ kind: 'error', message: 'Select a map in the sidebar first.' });
      return;
    }
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      const res = await runRegionInterpolation(activeMatrix, currentSelectionBox(), {
        mode: interpMode,
        axes: interpAxes,
        allow_extrapolation: allowExtrapolation,
        fallback_all_sides: true,
      });
      setBannerState(res);
      if (res.kind === 'ok') {
        await onMatrixUpdated(selectedMap, res.data.matrix);
        setInfoMessage(
          `Interpolated (${res.data.applied_mode}, ${res.data.applied_axes}${
            res.data.used_extrapolation ? ', extrapolated' : ''
          }).`
        );
      }
    } finally {
      setBusy(false);
    }
  };

  const handleSolveConversion = async () => {
    const pt1: [number, number] = [Number.parseFloat(p1x), Number.parseFloat(p1y)];
    const pt2: [number, number] = [Number.parseFloat(p2x), Number.parseFloat(p2y)];
    const pt3: [number, number] | null = useThirdPoint
      ? [Number.parseFloat(p3x), Number.parseFloat(p3y)]
      : null;
    setBusy(true);
    setBannerState(null);
    setInfoMessage(null);
    try {
      const res = await runFactorOffsetSolve(pt1, pt2, pt3, swapInputOutput);
      setBannerState(res);
      if (res.kind === 'ok') {
        setSolverResult(res.data);
        setInfoMessage(`Solved equation: ${res.data.equation}`);
      }
    } finally {
      setBusy(false);
    }
  };

  const tabButtons: Array<{ id: WorkbenchTab; label: string; icon: React.ReactNode }> = [
    { id: 'blind', label: 'Auto-Scan', icon: <Activity className="h-3.5 w-3.5" /> },
    { id: 'defs', label: 'A2L / DAMOS / XDF', icon: <FileCode className="h-3.5 w-3.5" /> },
    { id: 'similar', label: 'Similar & Migration', icon: <GitCompare className="h-3.5 w-3.5" /> },
    { id: 'checksum', label: 'Auto Checksums', icon: <ShieldCheck className="h-3.5 w-3.5" /> },
    { id: 'edit', label: 'Edit & Scaling', icon: <Calculator className="h-3.5 w-3.5" /> },
    { id: 'tools', label: 'ECU Tools', icon: <Cpu className="h-3.5 w-3.5" /> },
  ];

  return (
    <FloatingWindow
      windowId="workbench"
      isOpen={isOpen}
      onClose={onClose}
      title="Calibration Workbench"
      ariaLabel="Calibration Workbench"
      badge={hasRom ? `ROM ${(effectiveRomSize / 1024).toFixed(1)} KiB` : 'No ROM loaded'}
      icon={<Sliders className="h-4 w-4" />}
      defaultWidth={TOOL_WINDOW_SPECS.workbench.width}
      defaultHeight={TOOL_WINDOW_SPECS.workbench.height}
      minWidth={TOOL_WINDOW_SPECS.workbench.minWidth}
      minHeight={TOOL_WINDOW_SPECS.workbench.minHeight}
      dock={TOOL_WINDOW_SPECS.workbench.dock}
      standalone={standalone}
      popoutToolId="workbench"
      titlebarTone="sidebar"
      closeLabel="Close calibration workbench"
    >
      <div className="flex shrink-0 flex-wrap gap-1 border-b border-kiviBorder bg-kiviBg px-3 py-2">
          {tabButtons.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => {
                setTab(item.id);
                setBannerState(null);
                setInfoMessage(null);
              }}
              className={`flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-medium transition-colors ${
                tab === item.id
                  ? 'border border-kiviAccent/30 bg-kiviAccent/20 text-kiviAccent'
                  : 'border border-transparent text-kiviMuted hover:text-kiviInk'
              }`}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          ))}
        </div>

        <div className="flex-1 space-y-3 overflow-y-auto p-4 text-xs">
          <FeedbackBanner state={bannerState} infoMessage={infoMessage} />

          {tab === 'blind' && (
            <BlindScanTab
              busy={busy}
              includeCurves={includeCurves}
              include8bit={include8bit}
              include32bit={include32bit}
              maxTables={maxTables}
              blindResults={blindResults}
              onSetIncludeCurves={setIncludeCurves}
              onSetInclude8bit={setInclude8bit}
              onSetInclude32bit={setInclude32bit}
              onSetMaxTables={setMaxTables}
              onRunScan={() => void handleRunBlindV2()}
              onAddMaps={onAddMaps}
              onSelectMap={onSelectMap}
              onInfo={setInfoMessage}
            />
          )}

          {tab === 'defs' && (
            <DefinitionsTab
              busy={busy}
              rom={rom}
              defFormat={defFormat}
              defText={defText}
              baseOffsetText={baseOffsetText}
              baseOffset={parseAddressInput(baseOffsetText) ?? 0}
              defFilter={defFilter}
              a2lSummary={a2lSummary}
              damosSummary={damosSummary}
              xdfSummary={xdfSummary}
              onSetDefFormat={setDefFormat}
              onSetDefText={setDefText}
              onSetBaseOffsetText={setBaseOffsetText}
              onSetDefFilter={setDefFilter}
              onParseDef={() => void handleParseDef()}
              onTogglePatch={(patch, revert) => void handleTogglePatch(patch, revert)}
              onAddMaps={onAddMaps}
              onInfo={setInfoMessage}
            />
          )}

          {tab === 'similar' && (
            <SimilarityTab
              busy={busy}
              rom={rom}
              simMode={simMode}
              simAddrText={simAddrText}
              simRows={simRows}
              simCols={simCols}
              simBits={simBits}
              simLittleEndian={simLittleEndian}
              minPearsonR={minPearsonR}
              maxNrmse={maxNrmse}
              targetRomName={targetRomName}
              simHits={simHits}
              onSetSimMode={setSimMode}
              onSetSimAddrText={setSimAddrText}
              onSetSimRows={setSimRows}
              onSetSimCols={setSimCols}
              onSetSimBits={setSimBits}
              onSetSimLittleEndian={setSimLittleEndian}
              onSetMinPearsonR={setMinPearsonR}
              onSetMaxNrmse={setMaxNrmse}
              onSelectTargetRom={(buf, name) => {
                setTargetRom(buf);
                setTargetRomName(name);
              }}
              onRunSimilarity={() => void handleRunSimilarity()}
              onAddMaps={onAddMaps}
              onSelectMap={onSelectMap}
              onInfo={setInfoMessage}
            />
          )}

          {tab === 'checksum' && (
            <ChecksumsTab
              busy={busy}
              rom={rom}
              engines={engines}
              checksumMatches={checksumMatches}
              onDetectChecksums={() => void handleDetectChecksums()}
              onRepairChecksum={(id, mode) => void handleRepairChecksum(id, mode)}
              onRomUpdated={onRomUpdated}
              onInfo={setInfoMessage}
            />
          )}

          {tab === 'tools' && <EcuToolsTab rom={rom} onInfo={setInfoMessage} />}

          {tab === 'edit' && (
            <MatrixEditSolverTab
              busy={busy}
              selectedMap={selectedMap}
              selection={selection}
              editOpKind={editOpKind}
              editValue={editValue}
              clampMin={clampMin}
              clampMax={clampMax}
              interpMode={interpMode}
              interpAxes={interpAxes}
              allowExtrapolation={allowExtrapolation}
              p1x={p1x}
              p1y={p1y}
              p2x={p2x}
              p2y={p2y}
              useThirdPoint={useThirdPoint}
              p3x={p3x}
              p3y={p3y}
              swapInputOutput={swapInputOutput}
              solverResult={solverResult}
              onSetEditOpKind={setEditOpKind}
              onSetEditValue={setEditValue}
              onSetClampMin={setClampMin}
              onSetClampMax={setClampMax}
              onSelectionEdit={() => void handleSelectionEdit()}
              onSetInterpMode={setInterpMode}
              onSetInterpAxes={setInterpAxes}
              onSetAllowExtrapolation={setAllowExtrapolation}
              onInterpolate={() => void handleInterpolate()}
              onSetP1x={setP1x}
              onSetP1y={setP1y}
              onSetP2x={setP2x}
              onSetP2y={setP2y}
              onSetUseThirdPoint={setUseThirdPoint}
              onSetP3x={setP3x}
              onSetP3y={setP3y}
              onSetSwapInputOutput={setSwapInputOutput}
              onSolveConversion={() => void handleSolveConversion()}
              onUpdateMapScaling={onUpdateMapScaling}
              onInfo={setInfoMessage}
            />
          )}
        </div>
    </FloatingWindow>
  );
};
