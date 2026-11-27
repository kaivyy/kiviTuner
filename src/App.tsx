import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { useMapStore } from './stores/useMapStore';
import { useTelemetryStore } from './stores/useTelemetryStore';
import { useCorrelationStore } from './stores/useCorrelationStore';
import { useThemeStore, applyThemeToDocument } from './stores/useThemeStore';
import { useWorkspaceTabsStore } from './stores/useWorkspaceTabsStore';
import {
  groupMapsByCategory,
  filterMapTree,
  countMaps,
  type SelectionRange,
  type CellDisplayMode,
  type TablePalette,
  type HeatmapTableHandle,
} from './components/editor';
import { KiviMenuBar } from './components/layout';
import { openToolOrFloating } from './components/layout/toolWindowRegistry';
import { EngineeringToolkit } from './components/toolkit/EngineeringToolkit';
import { FlashPanel } from './components/toolkit/FlashPanel';
import { CataloguePanel } from './components/toolkit/CataloguePanel';
import {
  ShortcutsModal,
  HowToUseModal,
  BinDiffModal,
  CalibrationWorkbenchModal,
  AboutModal,
  MapPropertiesModal,
} from './components/modals';
import { AutoTuneReviewModal } from './components/autotune';
import { TelemetryDevPanel } from './components/telemetry/TelemetryDevPanel';
import { generateAxisLabels, resolveDisplayAxisLabels } from './lib/matrixRom';
import {
  buildCalibrationReportMarkdown,
  downloadTextFile,
  exportDefinitions,
  saveRomBytesToDisk,
  serializeMapToCsv,
} from './lib/exportHelpers';
import {
  isDesktopApp,
  bridgeScanRomHeuristics,
  bridgeScanRomHeuristicWindows,
  bridgeExportXdfXml,
  bridgeExportEcuIni,
  bridgeExportKpAr,
  type DiscoveredWindowMap,
  bridgeConvertDiffToMap,
  type DiffRegion,
  type DetectedMapCandidate,
  type ScanProgressEvent,
} from './lib/tauriBridge';
import type { MapDef } from './types/kivi';
import { useGlobalShortcuts } from './hooks/useGlobalShortcuts';
import { MapTreeSidebar } from './components/workspace/MapTreeSidebar';
import { WorkspaceViewport } from './components/workspace/WorkspaceViewport';
import { ScannerDrawer } from './components/workspace/ScannerDrawer';
import { InspectorSidebar } from './components/workspace/InspectorSidebar';
import { StatusBar } from './components/workspace/StatusBar';
import { WorkspaceTabBar } from './components/workspace/WorkspaceTabBar';
import { ActivityRail, MobileBottomNav } from './components/workspace/ActivityRail';
import { CommandPalette, type CommandPaletteAction } from './components/workspace/CommandPalette';
import { buildCommandPaletteActions } from './lib/workspaceActions';

export type LayoutPreset =
  | 'standard'
  | 'split_3d'
  | 'split_waveform'
  | 'full_waveform'
  | 'full_3d';

export const App: React.FC = () => {
  const {
    maps,
    selectedMapId,
    romLoaded,
    romSize,
    hashes,
    romData,
    romRevision,
    undoStatus,
    romDocuments,
    activeDocId,
    activeMatrix,
    baseMatrix,
    isDesktop,
    checksumStatus,
    checksumAlgorithmName,
    checksumNotice,
    recentRoms,
    selectMap,
    addMaps,
    updateMapDef,
    deleteMapDef,
    setActiveMatrix,
    loadRomBuffer,
    loadXdfString,
    extractMatrix,
    updateMatrix,
    patchActiveMapCells,
    patchRomBytes,
    revertMapToOriginal,
    undoEdit,
    redoEdit,
    openRomDocument,
    openRomDocumentFile,
    switchRomDocument,
    closeRomDocument,
    closeActiveRom,
    refreshRomSession,
    clearRecentRoms,
    verifyChecksum,
    fixChecksum,
    exportRom,
  } = useMapStore();

  const theme = useThemeStore((s) => s.theme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);

  useEffect(() => {
    applyThemeToDocument(theme);
  }, [theme]);

  const {
    tabs,
    activeTabId,
    splitTabId,
    openTab,
    activateTab,
    closeTab,
    togglePinTab,
    openInSplit,
    closeSplit,
    toggleSplitView,
    syncMapTab,
  } = useWorkspaceTabsStore();

  const isEcuConnected = useTelemetryStore((s) => s.connectionState.state === 'Connected');
  const ecuDriverId = useTelemetryStore((s) => s.selectedDriverId);
  const isTelemetryPanelOpen = useTelemetryStore((s) => s.isPanelOpen);
  const isRecording = useTelemetryStore((s) => s.isRecording);
  const connectEcu = useTelemetryStore((s) => s.connect);
  const disconnectEcu = useTelemetryStore((s) => s.disconnect);
  const selectEcuDriver = useTelemetryStore((s) => s.selectDriver);
  const toggleTelemetryPanel = useTelemetryStore((s) => s.togglePanel);
  const startRecording = useTelemetryStore((s) => s.startRecording);
  const stopRecording = useTelemetryStore((s) => s.stopRecording);
  const exportDatalogCsv = useTelemetryStore((s) => s.exportDatalogCsv);

  const hasRom = romLoaded || Boolean(romData && romData.length > 0);
  const selectedMap = maps.find((m) => m.id === selectedMapId);
  const activeTab = tabs.find((t) => t.id === activeTabId);
  const splitTab = tabs.find((t) => t.id === splitTabId);

  const binFileInputRef = useRef<HTMLInputElement>(null);
  const openDocFileInputRef = useRef<HTMLInputElement>(null);
  const xdfFileInputRef = useRef<HTMLInputElement>(null);
  const importDefInputRef = useRef<HTMLInputElement>(null);
  const heatmapActionRef = useRef<HeatmapTableHandle | null>(null);
  const noticeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [layoutPreset, setLayoutPreset] = useState<LayoutPreset>('standard');
  const [showMapTree, setShowMapTree] = useState<boolean>(() =>
    typeof window !== 'undefined' ? window.innerWidth >= 1024 : true
  );
  const [showInspector, setShowInspector] = useState<boolean>(() =>
    typeof window !== 'undefined' ? window.innerWidth >= 1400 : false
  );

  useEffect(() => {
    if (typeof window === 'undefined') return;
    let wasWide = window.innerWidth >= 1024;
    const onResize = () => {
      const isWide = window.innerWidth >= 1024;
      if (wasWide && !isWide) {
        setShowMapTree(false);
        setShowInspector(false);
        setIsScannerDrawerOpen(false);
      } else if (!wasWide && isWide) {
        setShowMapTree(true);
      }
      wasWide = isWide;
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  const [tableSelection, setTableSelection] = useState<SelectionRange | null>(null);
  const [displayMode, setDisplayMode] = useState<CellDisplayMode>('engineering');
  const [palette, setPalette] = useState<TablePalette>('classic');
  const [crosshairSync, setCrosshairSync] = useState<boolean>(true);

  const [isShortcutsOpen, setIsShortcutsOpen] = useState<boolean>(false);
  const [isHowToUseOpen, setIsHowToUseOpen] = useState<boolean>(false);
  const [isDiffModalOpen, setIsDiffModalOpen] = useState<boolean>(false);
  const [isToolkitOpen, setIsToolkitOpen] = useState<boolean>(false);
  const [isFlashPanelOpen, setIsFlashPanelOpen] = useState<boolean>(false);
  const [isCatalogueOpen, setIsCatalogueOpen] = useState<boolean>(false);
  const [isWorkbenchOpen, setIsWorkbenchOpen] = useState<boolean>(false);
  const [isAboutOpen, setIsAboutOpen] = useState<boolean>(false);
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState<boolean>(false);
  const [mapFilter, setMapFilter] = useState<string>('');
  const [isAutoTuneReviewOpen, setIsAutoTuneReviewOpen] = useState<boolean>(false);
  const [mapPropsModal, setMapPropsModal] = useState<{
    open: boolean;
    mode: 'create' | 'edit';
    map: MapDef | null;
    preset?: 'table' | 'axis' | 'scalar' | 'flag';
  }>({ open: false, mode: 'create', map: null });
  const [hexJumpAddress, setHexJumpAddress] = useState<number | null>(null);
  const [appNotice, setAppNotice] = useState<string | null>(null);

  const notify = useCallback((message: string) => {
    if (noticeTimerRef.current) clearTimeout(noticeTimerRef.current);
    setAppNotice(message);
    noticeTimerRef.current = setTimeout(() => setAppNotice(null), 3200);
  }, []);

  useEffect(() => {
    return () => {
      if (noticeTimerRef.current) clearTimeout(noticeTimerRef.current);
    };
  }, []);

  useEffect(() => {
    if (!isDesktop || !hasRom) return;
    const onFocus = () => {
      void refreshRomSession();
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [isDesktop, hasRom, refreshRomSession]);

  const mapTree = useMemo(
    () => filterMapTree(groupMapsByCategory(maps), mapFilter),
    [maps, mapFilter]
  );
  const totalMaps = useMemo(() => countMaps(groupMapsByCategory(maps)), [maps]);

  const [isScannerDrawerOpen, setIsScannerDrawerOpen] = useState<boolean>(false);
  const [isScanning, setIsScanning] = useState<boolean>(false);
  const [scanProgress, setScanProgress] = useState<ScanProgressEvent | null>(null);
  const [scannerSensitivity, setScannerSensitivity] = useState<
    'strict' | 'balanced' | 'aggressive'
  >('balanced');
  const [detectedCandidates, setDetectedCandidates] = useState<DetectedMapCandidate[]>([]);
  const hexMarks = useMemo(
    () =>
      detectedCandidates.map((c) => ({
        name: c.name_hint,
        start: c.start_addr,
        end: c.end_addr + 1,
      })),
    [detectedCandidates]
  );
  const [windowCandidates, setWindowCandidates] = useState<DiscoveredWindowMap[]>([]);
  const [isWindowScanning, setIsWindowScanning] = useState<boolean>(false);
  const [windowScanError, setWindowScanError] = useState<string | null>(null);

  const [diffRegions, setDiffRegions] = useState<DiffRegion[]>([]);
  const [waveformPitch, setWaveformPitch] = useState<number>(16);
  const [waveformIs16Bit, setWaveformIs16Bit] = useState<boolean>(false);
  const [waveformIsBigEndian, setWaveformIsBigEndian] = useState<boolean>(false);
  const [selectedRange, setSelectedRange] = useState<{ start: number; end: number } | null>(null);
  const [romFileName, setRomFileName] = useState<string | undefined>(undefined);

  const activeDocSummary = useMemo(
    () => romDocuments.find((d) => d.id === activeDocId || d.is_active),
    [romDocuments, activeDocId]
  );
  const activeDocLabel =
    activeDocSummary?.label ?? romFileName ?? (hasRom ? 'loaded.bin' : undefined);

  // Reset per-document transient views when active document switches or closes.
  useEffect(() => {
    setDiffRegions([]);
    setDetectedCandidates([]);
    setWindowCandidates([]);
    setWindowScanError(null);
    setSelectedRange(null);
    setTableSelection(null);
    setHexJumpAddress(null);
  }, [activeDocId]);

  useEffect(() => {
    if (selectedMap) {
      setWaveformPitch(selectedMap.cols);
      setWaveformIs16Bit(selectedMap.dtype === 'u16' || selectedMap.dtype === 'i16');
      setWaveformIsBigEndian(selectedMap.endian === 'big');
    }
  }, [selectedMap]);

  const currentMapIdRef = useRef<string | null>(null);
  const currentRevRef = useRef<number>(romRevision);
  useEffect(() => {
    if (selectedMap && hasRom) {
      if (
        currentMapIdRef.current !== selectedMap.id ||
        currentRevRef.current !== romRevision ||
        !activeMatrix
      ) {
        currentMapIdRef.current = selectedMap.id;
        currentRevRef.current = romRevision;
        void extractMatrix(selectedMap);
      }
    } else {
      currentMapIdRef.current = null;
      setActiveMatrix(null);
    }
  }, [selectedMap, hasRom, romRevision, activeMatrix, extractMatrix, setActiveMatrix]);

  const telemetrySource = useTelemetryStore((s) => s.telemetrySource);
  const sourceEpoch = useTelemetryStore((s) => s.sourceEpoch);
  const latestFrame = useTelemetryStore((s) => s.latestFrame);
  const ecuConnectionState = useTelemetryStore((s) => s.connectionState);

  useEffect(() => {
    useCorrelationStore.getState().setSourceEpoch(sourceEpoch);
  }, [sourceEpoch]);
  useEffect(() => {
    if (selectedMap) useCorrelationStore.getState().setTargetTable(selectedMap.id, selectedMap);
    else useCorrelationStore.getState().setTargetTable(null, null);
  }, [selectedMap]);
  useEffect(() => {
    if (latestFrame && crosshairSync) {
      useCorrelationStore.getState().correlateFrame(latestFrame).catch(() => {});
    }
  }, [latestFrame, crosshairSync]);
  useEffect(() => {
    useCorrelationStore.getState().invalidateOnSourceChange(telemetrySource);
  }, [telemetrySource]);
  useEffect(() => {
    if (ecuConnectionState.state !== 'Connected') {
      useCorrelationStore.getState().invalidateOnDisconnect();
    }
  }, [ecuConnectionState.state]);

  const handleSelectMap = useCallback(
    (id: string) => {
      selectMap(id);
      const mapDef = maps.find((m) => m.id === id);
      if (mapDef) {
        syncMapTab(
          mapDef,
          layoutPreset === 'full_3d'
            ? 'map-3d'
            : layoutPreset === 'split_3d'
              ? 'map-split'
              : 'map-table'
        );
      }
      if (typeof window !== 'undefined' && window.innerWidth < 1024) {
        setShowMapTree(false);
      }
    },
    [maps, selectMap, syncMapTab, layoutPreset]
  );

  const handleOpenMapInSplit = useCallback(
    (id: string) => {
      const mapDef = maps.find((m) => m.id === id);
      if (!mapDef) return;
      openInSplit({
        id: `map:${mapDef.id}`,
        kind: 'map-table',
        title: mapDef.name,
        mapId: mapDef.id,
      });
    },
    [maps, openInSplit]
  );

  const handleActivateWorkspaceTab = useCallback(
    (id: string) => {
      activateTab(id);
      const tab = tabs.find((t) => t.id === id);
      if (tab?.mapId && tab.mapId !== selectedMapId) {
        selectMap(tab.mapId);
      }
    },
    [activateTab, tabs, selectedMapId, selectMap]
  );

  const handleOpenHexTab = useCallback(() => {
    openTab({ id: 'tool:hex', kind: 'hex', title: 'Hex Viewer' });
  }, [openTab]);

  const handleOpenWaveformTab = useCallback(() => {
    openTab({ id: 'tool:waveform', kind: 'waveform', title: '2D Waveform' });
  }, [openTab]);

  const handleToggleSplitView = useCallback(() => {
    toggleSplitView({ id: 'tool:hex', kind: 'hex', title: 'Hex Viewer' });
  }, [toggleSplitView]);

  const handleChangeMatrix = useCallback(
    (newMatrix: number[][]) => {
      if (selectedMap) void updateMatrix(selectedMap, newMatrix);
      else setActiveMatrix(newMatrix);
    },
    [selectedMap, updateMatrix, setActiveMatrix]
  );

  const handlePatchCells = useCallback(
    (edits: Array<{ row: number; col: number; value: number }>, label?: string) => {
      if (selectedMap) {
        void patchActiveMapCells(selectedMap, edits, label);
      }
    },
    [selectedMap, patchActiveMapCells]
  );

  const handleBinFileSelected = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      await loadRomBuffer(new Uint8Array(await file.arrayBuffer()), undefined, { label: file.name });
      setRomFileName(file.name);
      handleOpenWaveformTab();
      notify(`Loaded ${file.name}`);
    } catch (err) {
      notify(`Failed to open ROM: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      e.target.value = '';
    }
  };

  const handleOpenMultiRomSelected = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      await openRomDocument(new Uint8Array(await file.arrayBuffer()), { label: file.name });
      setRomFileName(file.name);
      notify(`Opened project tab ${file.name}`);
    } catch (err) {
      notify(`Failed to open tab: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      e.target.value = '';
    }
  };

  const handleXdfFileSelected = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const defs = await loadXdfString(await file.text());
      if (defs.length > 0) {
        syncMapTab(defs[0], 'map-table');
      }
      notify(`Loaded ${defs.length} map definition(s)`);
    } catch (err) {
      notify(`Failed to parse XDF: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      e.target.value = '';
    }
  };

  const handleImportDefFileSelected = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      if (file.name.toLowerCase().endsWith('.xdf') || file.name.toLowerCase().endsWith('.xml')) {
        const defs = await loadXdfString(text);
        if (defs.length > 0) syncMapTab(defs[0], 'map-table');
        notify(`Imported ${defs.length} definition(s) from ${file.name}`);
      } else {
        const parsed = JSON.parse(text) as MapDef[] | { maps?: MapDef[] };
        const incoming = Array.isArray(parsed) ? parsed : Array.isArray(parsed.maps) ? parsed.maps : [];
        if (incoming.length === 0) {
          notify('No map definitions found in JSON file.');
          return;
        }
        addMaps(incoming);
        syncMapTab(incoming[0], 'map-table');
        notify(`Imported ${incoming.length} definition(s) from ${file.name}`);
      }
    } catch (err) {
      notify(`Definition import failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      e.target.value = '';
    }
  };

  const handleSaveRom = useCallback(
    async (forcePicker = false) => {
      const bytes = await exportRom();
      if (!bytes) {
        notify('Load a ROM first before saving.');
        return;
      }
      const suggested = activeDocLabel ? `edited_${activeDocLabel}` : 'kivituner_modified.bin';
      const outcome = await saveRomBytesToDisk(bytes, suggested, forcePicker);
      if (outcome.saved) {
        notify(`Saved ${outcome.filename}`);
      }
    },
    [exportRom, activeDocLabel, notify]
  );

  const handleExportCsv = useCallback(() => {
    if (!selectedMap || !activeMatrix) {
      notify('Select a loaded calibration table to export CSV.');
      return;
    }
    const csv = serializeMapToCsv(selectedMap, activeMatrix, baseMatrix);
    const cleanName = selectedMap.name.replace(/[^A-Za-z0-9_-]/g, '_');
    downloadTextFile(csv, `${cleanName}.csv`, 'text/csv;charset=utf-8');
    notify(`Exported ${cleanName}.csv`);
  }, [selectedMap, activeMatrix, baseMatrix, notify]);

  const handleExportReport = useCallback(() => {
    if (!hasRom) {
      notify('Load a ROM first to export a calibration report.');
      return;
    }
    const md = buildCalibrationReportMarkdown({
      romLabel: activeDocLabel ?? 'loaded.bin',
      romSize,
      hashes,
      checksumStatus,
      checksumAlgorithmName,
      maps,
      selectedMap,
      activeMatrix,
      baseMatrix,
    });
    downloadTextFile(md, 'kivituner_calibration_report.md', 'text/markdown;charset=utf-8');
    notify('Exported kivituner_calibration_report.md');
  }, [
    hasRom,
    activeDocLabel,
    romSize,
    hashes,
    checksumStatus,
    checksumAlgorithmName,
    maps,
    selectedMap,
    activeMatrix,
    baseMatrix,
    notify,
  ]);

  const handleExportDef = useCallback(
    async (format: 'xdf' | 'json' | 'ini' | 'kp') => {
      try {
        if (format === 'xdf') {
          const xml = await bridgeExportXdfXml(activeDocLabel ?? 'kiviTuner Calibration', maps);
          if (xml) {
            downloadTextFile(xml, 'kivituner_export.xdf', 'application/xml');
            notify('Exported kivituner_export.xdf');
            return;
          }
        } else if (format === 'ini') {
          const ini = await bridgeExportEcuIni('kiviTuner Export 1.0', undefined, maps);
          if (ini) {
            downloadTextFile(ini, 'kivituner_export.ini');
            notify('Exported kivituner_export.ini');
            return;
          }
        } else if (format === 'kp') {
          const kp = await bridgeExportKpAr(activeDocLabel ?? 'kiviTuner', maps);
          if (kp) {
            downloadTextFile(kp, 'kivituner_export.kp.txt');
            notify('Exported kivituner_export.kp.txt');
            return;
          }
        }
        const fallback = exportDefinitions(maps, format, romSize);
        downloadTextFile(fallback.content, fallback.filename, fallback.mime);
        notify(`Exported ${fallback.filename}`);
      } catch (err) {
        notify(`Export failed: ${err instanceof Error ? err.message : String(err)}`);
      }
    },
    [maps, activeDocLabel, romSize, notify]
  );

  const handleExportLogCsv = useCallback(async () => {
    try {
      const csv = await exportDatalogCsv();
      if (!csv) return;
      downloadTextFile(csv, `kivi_datalog_${Date.now()}.csv`, 'text/csv;charset=utf-8');
      notify('Exported telemetry CSV datalog');
    } catch (err) {
      notify(`Failed to export datalog: ${err instanceof Error ? err.message : String(err)}`);
    }
  }, [exportDatalogCsv, notify]);

  const { xLabels, yLabels, xAxisName, yAxisName } = useMemo(() => {
    if (!selectedMap) return { xLabels: [], yLabels: [], xAxisName: 'TPS', yAxisName: 'RPM' };
    const resolved = resolveDisplayAxisLabels(selectedMap);
    const hasX = Boolean(
      selectedMap.x_axis?.values && selectedMap.x_axis.values.length === selectedMap.cols
    );
    const hasY = Boolean(
      selectedMap.y_axis?.values && selectedMap.y_axis.values.length === selectedMap.rows
    );
    const synth = generateAxisLabels(selectedMap.rows, selectedMap.cols, selectedMap.name);
    return {
      xLabels: hasX ? resolved.xLabels : synth.xLabels,
      yLabels: hasY ? resolved.yLabels : synth.yLabels,
      xAxisName: hasX ? resolved.xAxisName : synth.xAxisName,
      yAxisName: hasY ? resolved.yAxisName : synth.yAxisName,
    };
  }, [selectedMap]);

  const interpolationAxes = useMemo(() => {
    if (!selectedMap) return { x: null, y: null };
    return { x: selectedMap.x_axis?.values ?? null, y: selectedMap.y_axis?.values ?? null };
  }, [selectedMap]);

  const handleSelectRange = (start: number, end: number) => setSelectedRange({ start, end });

  const handleJumpToWaveform = useCallback(
    (startAddr: number, endAddr: number) => {
      setSelectedRange({ start: startAddr, end: endAddr });
      setLayoutPreset('full_waveform');
      handleOpenWaveformTab();
    },
    [handleOpenWaveformTab]
  );

  const handleJumpToHex = useCallback(
    (address: number) => {
      setHexJumpAddress(address);
      openTab({ id: 'tool:hex', kind: 'hex', title: 'Hex Viewer' });
    },
    [openTab]
  );

  const handleCreateMapFromHex = useCallback(
    (startAddress: number, byteLength: number) => {
      const cols = Math.min(16, Math.max(1, byteLength));
      const rows = Math.max(1, Math.floor(byteLength / cols));
      setMapPropsModal({
        open: true,
        mode: 'create',
        preset: rows === 1 && cols === 1 ? 'scalar' : 'table',
        map: {
          id: `map_hex_${startAddress.toString(16)}`,
          name: `Hex Selection @ 0x${startAddress.toString(16).toUpperCase()}`,
          category: 'Custom Tables',
          address: startAddress,
          rows,
          cols,
          dtype: 'u8',
          endian: 'big',
          scaling: { equation: 'X', unit: 'raw', decimals: 0 },
        },
      });
    },
    []
  );

  const handleExtractMap = () => {
    if (!selectedRange) return;
    const length = selectedRange.end - selectedRange.start;
    const cols = waveformPitch > 0 ? waveformPitch : 16;
    const stepBytes = waveformIs16Bit ? 2 : 1;
    const rows = Math.max(1, Math.floor(Math.floor(length / stepBytes) / cols));
    const newMapId = `map_custom_${Date.now()}`;
    const newMap: MapDef = {
      id: newMapId,
      name: `Candidate Map (0x${selectedRange.start.toString(16).toUpperCase()})`,
      category: 'Discovered',
      address: selectedRange.start,
      rows,
      cols,
      dtype: waveformIs16Bit ? 'u16' : 'u8',
      endian: waveformIsBigEndian ? 'big' : 'little',
      scaling: { equation: 'X', unit: 'raw', decimals: 0 },
    };
    addMaps([newMap]);
    selectMap(newMapId);
    syncMapTab(newMap, 'map-table');
    setLayoutPreset('standard');
  };

  const handleExtractDiffRegion = useCallback(
    async (region: DiffRegion) => {
      const mapDef = await bridgeConvertDiffToMap(region);
      addMaps([mapDef]);
      selectMap(mapDef.id);
      syncMapTab(mapDef, 'map-table');
      setLayoutPreset('standard');
    },
    [addMaps, selectMap, syncMapTab]
  );

  const handleExtractAllDiffTables = useCallback(
    async (regions: DiffRegion[]) => {
      const newMaps: MapDef[] = [];
      for (const r of regions) newMaps.push(await bridgeConvertDiffToMap(r));
      addMaps(newMaps);
      if (newMaps.length > 0) {
        selectMap(newMaps[0].id);
        syncMapTab(newMaps[0], 'map-table');
        setLayoutPreset('standard');
      }
    },
    [addMaps, selectMap, syncMapTab]
  );

  const handleRunScanner = useCallback(async () => {
    if (!hasRom) return;
    setIsScanning(true);
    setScanProgress({ stage: 'starting', percent: 5, message: 'Scanning ROM heuristics' });
    setIsScannerDrawerOpen(true);
    try {
      const minConfidence =
        scannerSensitivity === 'strict' ? 85 : scannerSensitivity === 'aggressive' ? 50 : 70;
      const results = await bridgeScanRomHeuristics(romData, minConfidence);
      setDetectedCandidates(results);
      setScanProgress({
        stage: 'complete',
        percent: 100,
        message: `Found ${results.length} candidate(s)`,
      });
    } catch (err) {
      console.error('Scan heuristics failed:', err);
      setScanProgress(null);
    } finally {
      setIsScanning(false);
    }
  }, [hasRom, romData, scannerSensitivity]);

  const handleVbfLoadRom = useCallback(
    async (bytes: Uint8Array, baseAddress: number) => {
      await loadRomBuffer(bytes, undefined, { label: 'VBF Segment' });
      useMapStore.getState().setRomBaseAddress(baseAddress);
    },
    [loadRomBuffer]
  );

  const handleRunWindowScanner = useCallback(async () => {
    if (!hasRom) {
      setWindowScanError('Load a ROM first.');
      return;
    }
    setIsWindowScanning(true);
    setWindowScanError(null);
    try {
      const results = await bridgeScanRomHeuristicWindows(romData, [16, 12, 8], 6, 16, true);
      if (results === null) {
        setWindowScanError('Window scan requires the desktop application.');
        setWindowCandidates([]);
        return;
      }
      setWindowCandidates(results);
    } catch (err) {
      setWindowScanError(err instanceof Error ? err.message : String(err));
      setWindowCandidates([]);
    } finally {
      setIsWindowScanning(false);
    }
  }, [hasRom, romData]);

  const handleLoadCandidate = useCallback(
    (candidate: DetectedMapCandidate) => {
      const mapDef: MapDef = {
        id: candidate.id,
        name: candidate.name_hint,
        category: candidate.category_hint || 'Discovered',
        address: candidate.start_addr,
        rows: candidate.rows,
        cols: candidate.cols,
        dtype: candidate.dtype,
        endian: candidate.endian,
        scaling: { equation: 'X', unit: 'raw', decimals: 1 },
      };
      addMaps([mapDef]);
      selectMap(mapDef.id);
      syncMapTab(mapDef, 'map-table');
      setLayoutPreset('standard');
    },
    [addMaps, selectMap, syncMapTab]
  );

  const handleQuickDiff = useCallback(async () => {
    setIsDiffModalOpen(true);
  }, []);

  const handleLayoutChange = useCallback(
    (layout: string) => {
      const l = layout.toLowerCase().replace('-', '_');
      if (l === 'waveform' || l === 'full_waveform') {
        setLayoutPreset('full_waveform');
        handleOpenWaveformTab();
        return;
      }
      // When currently viewing tool:hex or tool:waveform, switch back to a map tab or close the tool tab
      if (activeTab?.kind === 'hex' || activeTab?.kind === 'waveform') {
        const mapTab = tabs.find((t) => t.kind.startsWith('map-'));
        if (mapTab) {
          activateTab(mapTab.id);
        } else {
          closeTab(activeTab.id);
        }
      }
      if (l === 'table' || l === 'standard') {
        setLayoutPreset('standard');
        if (selectedMap) syncMapTab(selectedMap, 'map-table');
      } else if (l === 'split' || l === 'split_3d') {
        setLayoutPreset('split_3d');
        if (selectedMap) syncMapTab(selectedMap, 'map-split');
      } else if (l === 'split_waveform') {
        setLayoutPreset('split_waveform');
        if (selectedMap) syncMapTab(selectedMap, 'map-table');
      } else if (l === 'surface3d' || l === 'full_3d') {
        setLayoutPreset('full_3d');
        if (selectedMap) syncMapTab(selectedMap, 'map-3d');
      }
    },
    [activeTab, tabs, activateTab, closeTab, selectedMap, syncMapTab, handleOpenWaveformTab]
  );

  const handleDisplayModeChange = useCallback((mode: string) => {
    const m = mode.toLowerCase();
    if (m === 'eng' || m === 'engineering') setDisplayMode('engineering');
    else if (m === 'hex' || m === 'raw_hex') setDisplayMode('raw_hex');
    else if (m === 'dec' || m === 'raw_dec') setDisplayMode('raw_dec');
    else if (m === 'delta') setDisplayMode('delta');
    else if (m === 'pct-delta' || m === 'pct_delta' || m === 'percent_delta')
      setDisplayMode('percent_delta');
  }, []);

  const handlePaletteChange = useCallback((pal: string) => {
    const p = pal.toLowerCase();
    if (p === 'thermal') setPalette('thermal');
    else if (p === 'spectrum') setPalette('spectrum');
    else if (p === 'mono' || p === 'monochrome') setPalette('monochrome');
    else setPalette('classic');
  }, []);

  const handleRevertToOriginal = useCallback(
    async (selection?: SelectionRange | null) => {
      if (!selectedMap) {
        notify('Select a map table first.');
        return;
      }
      const reverted = await revertMapToOriginal(selectedMap, selection ?? null);
      notify(
        reverted
          ? selection
            ? 'Reverted selected cells to stock baseline'
            : `Reverted ${selectedMap.name} to stock baseline`
          : 'Cells already match stock baseline'
      );
    },
    [selectedMap, revertMapToOriginal, notify]
  );

  const handleOpenRecentRom = useCallback(
    async (entryId: string) => {
      const rec = recentRoms.find((r) => r.id === entryId);
      if (!rec) return;
      const openDoc = romDocuments.find(
        (d) =>
          (rec.filePath && d.file_path === rec.filePath) ||
          (rec.sha256 && d.sha256 === rec.sha256)
      );
      if (openDoc) {
        await switchRomDocument(openDoc.id);
        notify(`Switched to ${openDoc.label}`);
        return;
      }
      if (isDesktopApp() && rec.filePath) {
        await openRomDocumentFile(rec.filePath, { label: rec.label });
        notify(`Re-opened ${rec.label}`);
        return;
      }
      notify(`Select "${rec.label}" again via Open ROM (Ctrl+O) in Web mode.`);
      binFileInputRef.current?.click();
    },
    [recentRoms, romDocuments, switchRomDocument, openRomDocumentFile, notify]
  );

  const runTableAction = useCallback(
    (fn: (handle: HeatmapTableHandle) => void) => {
      const handle = heatmapActionRef.current;
      if (!handle) {
        notify('Focus an open 2D map table to run table edit commands.');
        return;
      }
      fn(handle);
    },
    [notify]
  );

  useGlobalShortcuts({
    binFileInputRef,
    xdfFileInputRef,
    setLayoutPreset,
    setIsWorkbenchOpen,
    setIsDiffModalOpen,
    setIsShortcutsOpen,
    onRunScanner: handleRunScanner,
    onSaveRom: () => void handleSaveRom(false),
    onSaveAsRom: () => void handleSaveRom(true),
    onOpenCommandPalette: () => setIsCommandPaletteOpen((v) => !v),
    onUndo: () => {
      void undoEdit();
    },
    onRedo: () => {
      void redoEdit();
    },
    onCloseActiveTab: () => {
      if (activeTabId) closeTab(activeTabId);
    },
  });

  const paletteActions = useMemo<CommandPaletteAction[]>(
    () =>
      buildCommandPaletteActions({
        hasRom,
        selectedMap,
        undoStatus,
        splitTabId,
        mapsCount: maps.length,
        theme,
        onOpenRom: () => binFileInputRef.current?.click(),
        onOpenRomTab: () => openDocFileInputRef.current?.click(),
        onSaveRom: (saveAs) => void handleSaveRom(saveAs),
        onCreateMap: () =>
          setMapPropsModal({ open: true, mode: 'create', map: null, preset: 'table' }),
        onRevertToOriginal: () => void handleRevertToOriginal(null),
        onUndo: () => void undoEdit(),
        onRedo: () => void redoEdit(),
        onOpenHex: handleOpenHexTab,
        onJumpHexMap: handleJumpToHex,
        onOpenWaveform: handleOpenWaveformTab,
        onToggleSplit: handleToggleSplitView,
        onRunScanner: () => void handleRunScanner(),
        onOpenWorkbench: () => setIsWorkbenchOpen(true),
        onOpenFlash: () => void openToolOrFloating('flash', () => setIsFlashPanelOpen(true)),
        onOpenDiff: () => setIsDiffModalOpen(true),
        onFixChecksum: () => void fixChecksum(),
        onExportDef: (fmt) => void handleExportDef(fmt),
        onToggleTheme: () => toggleTheme(),
        onOpenHowToUse: () => void openToolOrFloating('guide', () => setIsHowToUseOpen(true)),
      }),
    [
      hasRom,
      selectedMap,
      undoStatus,
      splitTabId,
      maps.length,
      theme,
      handleSaveRom,
      handleRevertToOriginal,
      undoEdit,
      redoEdit,
      handleOpenHexTab,
      handleJumpToHex,
      handleOpenWaveformTab,
      handleToggleSplitView,
      handleRunScanner,
      fixChecksum,
      handleExportDef,
      toggleTheme,
    ]
  );

  const totalDeltaBytes = useMemo(
    () => diffRegions.reduce((sum, r) => sum + r.changed_byte_count, 0),
    [diffRegions]
  );

  return (
    <div className="flex h-screen w-screen flex-col bg-kiviBg text-kiviInk select-none">
      <input
        type="file"
        ref={binFileInputRef}
        onChange={handleBinFileSelected}
        accept=".bin,.rom,.hex,.s19,.srec"
        className="hidden"
      />
      <input
        type="file"
        ref={openDocFileInputRef}
        onChange={handleOpenMultiRomSelected}
        accept=".bin,.rom,.hex,.s19,.srec"
        className="hidden"
      />
      <input
        type="file"
        ref={xdfFileInputRef}
        onChange={handleXdfFileSelected}
        accept=".xdf,.xml"
        className="hidden"
      />
      <input
        type="file"
        ref={importDefInputRef}
        onChange={handleImportDefFileSelected}
        accept=".json,.kivi.json,.xdf,.xml"
        className="hidden"
      />

      <KiviMenuBar
        checksumStatus={checksumStatus}
        onVerifyChecksum={() => verifyChecksum()}
        onFixChecksum={() => fixChecksum()}
        deltaCount={totalDeltaBytes}
        onDeltaClick={() => setIsDiffModalOpen(true)}
        displayModeText={
          selectedMap
            ? `${selectedMap.dtype.toUpperCase()} ${
                selectedMap.endian === 'big' ? 'BE' : 'LE'
              } | ${displayMode}`
            : '16-Bit BE | Eng Val'
        }
        romName={hasRom ? activeDocLabel : undefined}
        romSizeBytes={romSize}
        onOpenShortcuts={() => void openToolOrFloating('shortcuts', () => setIsShortcutsOpen(true))}
        callbacks={{
          onOpenRom: () => binFileInputRef.current?.click(),
          onOpenRecent: (id) => void handleOpenRecentRom(id),
          onClearRecentRoms: () => {
            clearRecentRoms();
            notify('Cleared recent ROM list');
          },
          onReferenceSlot: () => setIsDiffModalOpen(true),
          onClearReferences: () => {
            setDiffRegions([]);
            notify('Cleared diff comparison regions');
          },
          onSaveRom: () => void handleSaveRom(false),
          onSaveAsRom: () => void handleSaveRom(true),
          onExportCsv: handleExportCsv,
          onExportReport: handleExportReport,
          onCloseRom: () => void closeActiveRom(),
          onLoadXdf: () => xdfFileInputRef.current?.click(),
          onSaveKiviJson: () => void handleExportDef('json'),
          onExportDef: (fmt) => {
            void handleExportDef(fmt);
          },
          onImportDef: () => importDefInputRef.current?.click(),
          onCreateParameter: (preset) =>
            setMapPropsModal({ open: true, mode: 'create', map: null, preset }),
          onXdfHeader: () => {
            if (selectedMap) {
              setMapPropsModal({ open: true, mode: 'edit', map: selectedMap });
            } else {
              setIsWorkbenchOpen(true);
            }
          },
          onUndo: () => void undoEdit(),
          onRedo: () => void redoEdit(),
          onCommitProject: () => {
            void verifyChecksum().then((ok) =>
              notify(ok ? 'Checksum verified cleanly' : 'Snapshot noted (checksum unverified)')
            );
          },
          onRollbackProject: () => void handleRevertToOriginal(null),
          onViewLedger: () => setShowInspector(true),
          onCopy: () => runTableAction((h) => h.copySelection()),
          onPaste: () => runTableAction((h) => h.pasteSelection()),
          onPasteSpecial: (mode) => runTableAction((h) => h.pasteSpecial(mode)),
          onSelectAll: () => runTableAction((h) => h.selectAll()),
          onInvertSelection: () => runTableAction((h) => h.invertSelection()),
          onClearSelection: () => runTableAction((h) => h.clearSelection()),
          onCellMath: () => runTableAction((h) => h.openMathDialog()),
          onSmooth3x3: () => runTableAction((h) => h.applySmooth3x3()),
          onInterpolate: (mode) => runTableAction((h) => h.applyInterpolate(mode)),
          onLayoutChange: handleLayoutChange,
          onDisplayModeChange: handleDisplayModeChange,
          onPaletteChange: handlePaletteChange,
          onToggleSplitView: handleToggleSplitView,
          onOpenCommandPalette: () => setIsCommandPaletteOpen(true),
          onToggleTheme: () => toggleTheme(),
          onResetLayout: () => {
            setLayoutPreset('standard');
            closeSplit();
          },
          onExtractWaveformMap: handleExtractMap,
          onRunScanner: handleRunScanner,
          onScannerSensitivityChange: (s) => setScannerSensitivity(s),
          onClearScanner: () => setDetectedCandidates([]),
          onOpenDiffModal: () => setIsDiffModalOpen(true),
          onQuickDiff: handleQuickDiff,
          onExtractDiffs: () =>
            handleExtractAllDiffTables(diffRegions.filter((d) => d.classification.type === 'Table2D')),
          onVerifyChecksum: () => verifyChecksum(),
          onFixChecksum: () => fixChecksum(),
          onOpenHowToUse: () => void openToolOrFloating('guide', () => setIsHowToUseOpen(true)),
          onOpenShortcuts: () => void openToolOrFloating('shortcuts', () => setIsShortcutsOpen(true)),
          onResetSettings: () => {
            setLayoutPreset('standard');
            setDisplayMode('engineering');
            setPalette('classic');
            setScannerSensitivity('balanced');
            setCrosshairSync(true);
            closeSplit();
            clearRecentRoms();
            notify('Reset workspace view and settings');
          },
          onAbout: () => void openToolOrFloating('about', () => setIsAboutOpen(true)),
          onDriverChange: (driver) => selectEcuDriver(driver),
          onConnectEcu: () => connectEcu(),
          onDisconnectEcu: () => disconnectEcu(),
          onToggleTelemetry: () => void openToolOrFloating('telemetry', () => toggleTelemetryPanel()),
          onToggleCrosshairSync: () => {
            setCrosshairSync((v) => {
              notify(!v ? 'Synchronized crosshair enabled' : 'Synchronized crosshair paused');
              return !v;
            });
          },
          onAfrAutoTune: () => setIsAutoTuneReviewOpen(true),
          onStartLogging: () => startRecording(),
          onStopLogging: () => stopRecording(),
          onExportLogCsv: handleExportLogCsv,
          onReadEcuId: () => void openToolOrFloating('flash', () => setIsFlashPanelOpen(true)),
          onReadFlash: () => void openToolOrFloating('flash', () => setIsFlashPanelOpen(true)),
          onWriteFlash: () => void openToolOrFloating('flash', () => setIsFlashPanelOpen(true)),
          onOpenCatalogue: () => void openToolOrFloating('catalogue', () => setIsCatalogueOpen(true)),
          onOpenWorkbench: () => setIsWorkbenchOpen(true),
        }}
        state={{
          romLoaded: hasRom,
          canUndo: undoStatus.can_undo,
          canRedo: undoStatus.can_redo,
          currentLayout: layoutPreset,
          displayMode:
            displayMode === 'engineering'
              ? 'eng'
              : displayMode === 'raw_hex'
                ? 'hex'
                : displayMode === 'raw_dec'
                  ? 'dec'
                  : displayMode === 'percent_delta'
                    ? 'pct-delta'
                    : 'delta',
          palette:
            palette === 'thermal'
              ? 'thermal'
              : palette === 'spectrum'
                ? 'spectrum'
                : palette === 'monochrome'
                  ? 'mono'
                  : 'classic',
          scannerSensitivity,
          ecuDriver: ecuDriverId,
          ecuConnected: isEcuConnected,
          isLogging: isRecording,
          crosshairSync,
          theme,
          recentRoms: recentRoms.map((r) => ({
            id: r.id,
            label: r.label,
            size: r.size,
            sha256: r.sha256,
          })),
        }}
      />

      <WorkspaceTabBar
        romDocuments={romDocuments}
        activeDocId={activeDocId}
        onSwitchRomDocument={(docId) => void switchRomDocument(docId)}
        onCloseRomDocument={(docId) => void closeRomDocument(docId)}
        onOpenRomDocument={() => openDocFileInputRef.current?.click()}
        tabs={tabs}
        activeTabId={activeTabId}
        splitTabId={splitTabId}
        onActivateTab={handleActivateWorkspaceTab}
        onCloseTab={closeTab}
        onTogglePinTab={togglePinTab}
        onToggleSplitView={handleToggleSplitView}
        onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
      />

      {appNotice && (
        <div
          role="status"
          aria-live="polite"
          className="flex items-center justify-between border-b border-kiviAccent/30 bg-kiviAccent/10 px-4 py-1 text-xs text-kiviAccent"
        >
          <span>{appNotice}</span>
          <button
            type="button"
            onClick={() => setAppNotice(null)}
            className="text-[11px] font-medium hover:underline"
          >
            Dismiss
          </button>
        </div>
      )}

      <div className="relative flex min-h-0 flex-1 overflow-hidden">
        {(showMapTree || showInspector || isScannerDrawerOpen) && (
          <div
            aria-hidden="true"
            onClick={() => {
              setShowMapTree(false);
              setShowInspector(false);
              setIsScannerDrawerOpen(false);
            }}
            className="fixed inset-0 z-30 bg-black/50 lg:hidden"
          />
        )}

        <ActivityRail
          showMapTree={showMapTree}
          onToggleMapTree={() => setShowMapTree((v) => !v)}
          showInspector={showInspector}
          onToggleInspector={() => setShowInspector((v) => !v)}
          isScannerOpen={isScannerDrawerOpen}
          onToggleScanner={() => setIsScannerDrawerOpen((v) => !v)}
          activeTabKind={activeTab?.kind ?? null}
          onOpenHex={handleOpenHexTab}
          onOpenWaveform={handleOpenWaveformTab}
          onOpenWorkbench={() => setIsWorkbenchOpen(true)}
          onOpenFlash={() => void openToolOrFloating('flash', () => setIsFlashPanelOpen(true))}
          onOpenDiff={() => setIsDiffModalOpen(true)}
          onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
          theme={theme}
          onToggleTheme={() => toggleTheme()}
        />

        {showMapTree && (
          <MapTreeSidebar
            totalMaps={totalMaps}
            mapsCount={maps.length}
            mapFilter={mapFilter}
            onChangeMapFilter={setMapFilter}
            mapTree={mapTree}
            selectedMapId={selectedMapId}
            onSelectMap={handleSelectMap}
            onOpenMapInSplit={handleOpenMapInSplit}
            onCreateMap={() =>
              setMapPropsModal({ open: true, mode: 'create', map: null, preset: 'table' })
            }
            onEditMap={(map) => setMapPropsModal({ open: true, mode: 'edit', map })}
            onDeleteMap={(id) => deleteMapDef(id)}
            potentialMaps={detectedCandidates}
            onInspectPotentialInHex={handleJumpToHex}
            onPromotePotentialMap={handleLoadCandidate}
            onClose={() => setShowMapTree(false)}
          />
        )}

        <WorkspaceViewport
          layoutPreset={layoutPreset}
          setLayoutPreset={handleLayoutChange}
          selectedMap={selectedMap}
          activeMatrix={activeMatrix}
          baseMatrix={baseMatrix}
          displayMode={displayMode}
          setDisplayMode={setDisplayMode}
          palette={palette}
          setPalette={setPalette}
          xLabels={xLabels}
          yLabels={yLabels}
          xAxisName={xAxisName}
          yAxisName={yAxisName}
          interpolationAxes={interpolationAxes}
          tableSelection={tableSelection}
          setTableSelection={setTableSelection}
          onChangeMatrix={handleChangeMatrix}
          onPatchCells={handlePatchCells}
          romData={romData}
          romLoaded={romLoaded}
          romSize={romSize}
          romRevision={romRevision}
          activeDocId={activeDocId}
          hexMarks={hexMarks}
          hexJumpAddress={hexJumpAddress}
          onCreateMapFromHex={handleCreateMapFromHex}
          heatmapActionRef={heatmapActionRef}
          onRevertToOriginal={(sel) => void handleRevertToOriginal(sel)}
          onEditMapProperties={
            selectedMap
              ? () => setMapPropsModal({ open: true, mode: 'edit', map: selectedMap })
              : undefined
          }
          waveformPitch={waveformPitch}
          setWaveformPitch={setWaveformPitch}
          waveformIs16Bit={waveformIs16Bit}
          waveformIsBigEndian={waveformIsBigEndian}
          diffRegions={diffRegions}
          detectedCandidates={detectedCandidates}
          selectedRange={selectedRange}
          onSelectRange={handleSelectRange}
          onJumpToWaveform={handleJumpToWaveform}
          onExtractMap={handleExtractMap}
          onOpenToolkit={() => void openToolOrFloating('toolkit', () => setIsToolkitOpen(true))}
          onRunScanner={handleRunScanner}
          activeTab={activeTab}
          splitTab={splitTab}
          onCloseSplit={closeSplit}
        />

        <ScannerDrawer
          isOpen={isScannerDrawerOpen}
          onClose={() => setIsScannerDrawerOpen(false)}
          isDesktop={isDesktop}
          romData={romData}
          scannerSensitivity={scannerSensitivity}
          onChangeSensitivity={setScannerSensitivity}
          isScanning={isScanning}
          onRunScanner={handleRunScanner}
          scanProgress={scanProgress}
          hasRomData={hasRom}
          isWindowScanning={isWindowScanning}
          onRunWindowScanner={() => void handleRunWindowScanner()}
          windowScanError={windowScanError}
          windowCandidates={windowCandidates}
          detectedCandidates={detectedCandidates}
          onLoadCandidate={handleLoadCandidate}
          onAddMaps={(newMaps, selectFirst) => {
            addMaps(newMaps);
            if (selectFirst && newMaps.length > 0) {
              selectMap(newMaps[0].id);
              syncMapTab(newMaps[0], 'map-table');
            }
          }}
          onJumpToWaveform={handleJumpToWaveform}
        />

        {showInspector && (
          <InspectorSidebar
            selectedRange={selectedRange}
            waveformPitch={waveformPitch}
            onExtractMap={handleExtractMap}
            checksumStatus={checksumStatus}
            checksumAlgorithmName={checksumAlgorithmName}
            checksumNotice={checksumNotice}
            onFixChecksum={() => fixChecksum()}
            romLoaded={hasRom}
            romSize={romSize}
            romLabel={activeDocLabel}
            hashes={hashes}
            undoStatus={undoStatus}
            selectedMap={selectedMap}
            onEditMapProperties={(map) =>
              setMapPropsModal({ open: true, mode: 'edit', map })
            }
            onViewInHex={handleJumpToHex}
            onRevertMapToOriginal={() => void handleRevertToOriginal(null)}
            onClose={() => setShowInspector(false)}
          />
        )}
      </div>

      <MobileBottomNav
        showMapTree={showMapTree}
        onToggleMapTree={() => setShowMapTree((v) => !v)}
        showInspector={showInspector}
        onToggleInspector={() => setShowInspector((v) => !v)}
        activeTabKind={activeTab?.kind ?? null}
        onOpenHex={handleOpenHexTab}
        onOpenWaveform={handleOpenWaveformTab}
        onOpenWorkbench={() => setIsWorkbenchOpen(true)}
        theme={theme}
        onToggleTheme={() => toggleTheme()}
      />

      <StatusBar
        selectedMapName={selectedMap?.name}
        isDesktop={isDesktop}
        layoutPreset={layoutPreset}
        diffRegionsCount={diffRegions.length}
        waveformPitch={waveformPitch}
        romLabel={activeDocLabel}
        romSize={romSize}
        sha256Short={hashes?.sha256.slice(0, 8) ?? null}
        checksumAlgorithmName={checksumAlgorithmName}
        checksumStatus={checksumStatus}
        undoStatus={undoStatus}
        onUndo={() => void undoEdit()}
        onRedo={() => void redoEdit()}
      />

      <CommandPalette
        isOpen={isCommandPaletteOpen}
        onClose={() => setIsCommandPaletteOpen(false)}
        maps={maps}
        onSelectMap={(map) => handleSelectMap(map.id)}
        actions={paletteActions}
      />

      <EngineeringToolkit isOpen={isToolkitOpen} onClose={() => setIsToolkitOpen(false)} />
      <CataloguePanel isOpen={isCatalogueOpen} onClose={() => setIsCatalogueOpen(false)} />
      <FlashPanel
        isOpen={isFlashPanelOpen}
        onClose={() => setIsFlashPanelOpen(false)}
        rom={romData}
        onDumped={async (bytes, baseAddress) => {
          await loadRomBuffer(bytes, undefined, {
            label: `ECU Dump @ 0x${baseAddress.toString(16).toUpperCase()}`,
          });
          useMapStore.getState().setRomBaseAddress(baseAddress);
        }}
      />
      <BinDiffModal
        isOpen={isDiffModalOpen}
        onClose={() => setIsDiffModalOpen(false)}
        baseRom={romData}
        currentDiffs={diffRegions}
        onDiffsChange={setDiffRegions}
        onJumpToWaveform={handleJumpToWaveform}
        onExtractMap={handleExtractDiffRegion}
        onExtractAllTables={handleExtractAllDiffTables}
        onVbfLoadRom={handleVbfLoadRom}
      />
      <CalibrationWorkbenchModal
        isOpen={isWorkbenchOpen}
        onClose={() => setIsWorkbenchOpen(false)}
        rom={romData}
        romLoaded={hasRom}
        romSize={romSize}
        maps={maps}
        selectedMap={selectedMap ?? null}
        activeMatrix={activeMatrix}
        selection={tableSelection}
        onAddMaps={(newMaps, selectFirst) => {
          addMaps(newMaps);
          if (selectFirst && newMaps.length > 0) {
            handleSelectMap(newMaps[0].id);
          }
        }}
        onSelectMap={(mapId) => handleSelectMap(mapId)}
        onRomUpdated={async (nextRom, checksumOk) => {
          if (isDesktopApp() && hasRom && nextRom.length === romSize) {
            await refreshRomSession();
          } else if (!hasRom || nextRom.length !== romSize) {
            await loadRomBuffer(nextRom, undefined, { label: activeDocLabel });
          } else if (romData) {
            const writes: Array<{ offset: number; bytes: number[] }> = [];
            for (let i = 0; i < nextRom.length; i++) {
              if (nextRom[i] !== romData[i]) {
                writes.push({ offset: i, bytes: [nextRom[i]] });
              }
            }
            if (writes.length > 0) {
              await patchRomBytes(writes, 'Workbench Checksum Repair');
            }
          }
          if (checksumOk !== undefined) {
            useMapStore.getState().setChecksumStatus(checksumOk ? 'valid' : 'invalid');
          }
        }}
        onMatrixUpdated={async (mapDef, nextMatrix) => {
          await updateMatrix(mapDef, nextMatrix);
        }}
        onUpdateMapScaling={(mapId, equation) => {
          const target = maps.find((m) => m.id === mapId);
          if (target) {
            void updateMapDef({ ...target, scaling: { ...target.scaling, equation } });
          }
        }}
      />
      <MapPropertiesModal
        isOpen={mapPropsModal.open}
        onClose={() => setMapPropsModal((s) => ({ ...s, open: false }))}
        initialMap={mapPropsModal.map}
        mode={mapPropsModal.mode}
        createPresetType={mapPropsModal.preset}
        romSize={romSize}
        onSaveMap={async (mapDef) => {
          await updateMapDef(mapDef);
          selectMap(mapDef.id);
          syncMapTab(mapDef, 'map-table');
          setLayoutPreset('standard');
          notify(
            mapPropsModal.mode === 'edit'
              ? `Updated ${mapDef.name}`
              : `Added ${mapDef.name}`
          );
        }}
      />
      <AboutModal
        isOpen={isAboutOpen}
        onClose={() => setIsAboutOpen(false)}
        isDesktop={isDesktop}
        romLabel={activeDocLabel ?? 'None'}
        romSize={romSize}
        sha256={hashes?.sha256 ?? null}
        mapsCount={maps.length}
        checksumAlgorithmName={checksumAlgorithmName}
      />
      <ShortcutsModal isOpen={isShortcutsOpen} onClose={() => setIsShortcutsOpen(false)} />
      <HowToUseModal isOpen={isHowToUseOpen} onClose={() => setIsHowToUseOpen(false)} />
      <AutoTuneReviewModal
        isOpen={isAutoTuneReviewOpen}
        onClose={() => setIsAutoTuneReviewOpen(false)}
      />
      {isTelemetryPanelOpen && (
        <TelemetryDevPanel onClose={() => toggleTelemetryPanel(false)} />
      )}
    </div>
  );
};

export default App;
