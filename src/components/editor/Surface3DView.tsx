import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import {
  Box,
  Grid,
  MoveHorizontal,
  MoveVertical,
  Layers,
  Ghost,
  RotateCcw,
  ZoomIn,
  ZoomOut,
  Sliders,
  Palette,
} from 'lucide-react';
import type { SelectionRange } from './HeatmapTable';
import {
  CAMERA_PRESETS,
  calculateAutoYScaling,
  calculateMatrixCenter,
  findClosestCell,
  getMatrixExtremes,
  getPaletteColor,
  projectPoint,
  buildQuads,
  sortQuadsByDepth,
  type CameraState,
  type Point3D,
  type SurfacePalette,
} from '../../lib/surface3d';
import { adjustSelection, scaleSelection } from '../../lib/interpolation';
import { useCorrelationStore } from '../../stores/useCorrelationStore';
import { useThemeStore } from '../../stores/useThemeStore';
import {
  resolveCorrelatedCell,
  get3DSurfaceCrosshairGeometry,
  type CellCoordinate,
} from '../../lib/crosshairModel';

export interface Surface3DViewProps {
  title?: string;
  tableId?: string;
  correlatedCell?: CellCoordinate | null;
  matrix: number[][];
  baseMatrix?: number[][];
  xLabels?: (string | number)[];
  yLabels?: (string | number)[];
  xAxisName?: string;
  yAxisName?: string;
  unit?: string;
  decimals?: number;
  selectedRange?: SelectionRange | null;
  onSelectCell?: (r: number, c: number) => void;
  onChangeMatrix?: (newMatrix: number[][]) => void;
  palette?: 'classic' | 'thermal' | 'spectrum' | 'monochrome';
  readOnly?: boolean;
}

export const Surface3DView: React.FC<Surface3DViewProps> = ({
  title,
  tableId,
  correlatedCell: propCorrelatedCell,
  matrix,
  baseMatrix,
  xLabels,
  yLabels,
  xAxisName = 'TPS / Load',
  yAxisName = 'RPM',
  unit = 'Raw',
  decimals = 1,
  selectedRange,
  onSelectCell,
  onChangeMatrix,
  palette: initialPalette = 'classic',
  readOnly = false,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const canvasRectRef = useRef<{ left: number; top: number; width: number; height: number } | null>(
    null
  );
  const theme = useThemeStore((s) => s.theme);

  const numRows = matrix?.length ?? 0;
  const numCols = matrix?.[0]?.length ?? 0;

  // Correlation Crosshair Resolution
  const activeCorrelation = useCorrelationStore((s) => s.activeCorrelation);
  const targetTableId = useCorrelationStore((s) => s.targetTableId);

  const activeCorrelatedCell: CellCoordinate | null = useMemo(() => {
    if (propCorrelatedCell !== undefined) {
      if (
        propCorrelatedCell &&
        propCorrelatedCell.r >= 0 &&
        propCorrelatedCell.r < numRows &&
        propCorrelatedCell.c >= 0 &&
        propCorrelatedCell.c < numCols
      ) {
        return propCorrelatedCell;
      }
      return null;
    }
    return resolveCorrelatedCell({
      matrixDimensions: { numRows, numCols },
      activeCorrelation,
      targetTableId,
      currentTableId: tableId,
    });
  }, [propCorrelatedCell, numRows, numCols, activeCorrelation, targetTableId, tableId]);

  // Viewport & rendering states
  const [dimensions, setDimensions] = useState<{ width: number; height: number }>({
    width: 640,
    height: 480,
  });

  const [activePalette, setActivePalette] = useState<SurfacePalette>(initialPalette);
  const [renderMode, setRenderMode] = useState<'shaded' | 'wireframe'>('shaded');
  const [showGhost, setShowGhost] = useState<boolean>(true);
  const [elevationMultiplier, setElevationMultiplier] = useState<number>(1.0);
  const [hoveredCell, setHoveredCell] = useState<{ r: number; c: number } | null>(null);

  // Camera state
  const [camera, setCamera] = useState<CameraState>(() => ({
    ...CAMERA_PRESETS.ISOMETRIC,
    scale: 25,
    panX: 0,
    panY: 0,
  }));

  // Matrix geometry bounds
  const center = useMemo(() => calculateMatrixCenter(matrix), [matrix]);
  const extremes = useMemo(() => getMatrixExtremes(matrix), [matrix]);
  const baseExtremes = useMemo(
    () => (baseMatrix ? getMatrixExtremes(baseMatrix) : null),
    [baseMatrix]
  );

  const minVal = useMemo(
    () => (baseExtremes ? Math.min(extremes.min, baseExtremes.min) : extremes.min),
    [extremes.min, baseExtremes]
  );
  const maxVal = useMemo(
    () => (baseExtremes ? Math.max(extremes.max, baseExtremes.max) : extremes.max),
    [extremes.max, baseExtremes]
  );

  // Auto-calculated Y elevation scaling
  const baseYScaling = useMemo(() => calculateAutoYScaling(matrix), [matrix]);
  const effectiveYScaling = baseYScaling * elevationMultiplier;

  // Compute initial auto-fit scale
  const computeAutoFitScale = useCallback(
    (w: number, h: number): number => {
      const maxDim = Math.max(numRows, numCols, 6);
      return Math.max(10, Math.min(w, h) / (maxDim * 1.55));
    },
    [numRows, numCols]
  );

  // Auto-resize canvas according to container and cache bounding rect for zero-reflow pointer math
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const refreshRect = () => {
      if (canvasRef.current) {
        const cr = canvasRef.current.getBoundingClientRect();
        canvasRectRef.current = {
          left: cr.left,
          top: cr.top,
          width: cr.width,
          height: cr.height,
        };
      }
    };

    const updateSize = () => {
      const rect = container.getBoundingClientRect();
      const w = Math.max(300, Math.floor(rect.width));
      const h = Math.max(300, Math.floor(rect.height));

      setDimensions((prev) => {
        if (prev.width === w && prev.height === h) return prev;
        return { width: w, height: h };
      });
      refreshRect();
    };

    updateSize();
    const observer = new ResizeObserver(updateSize);
    observer.observe(container);
    window.addEventListener('resize', refreshRect, { passive: true });
    window.addEventListener('scroll', refreshRect, { capture: true, passive: true });

    return () => {
      observer.disconnect();
      window.removeEventListener('resize', refreshRect);
      window.removeEventListener('scroll', refreshRect, { capture: true });
    };
  }, []);

  // Set initial scale on first load or matrix size change
  const initialFitDone = useRef(false);
  useEffect(() => {
    if (dimensions.width > 0 && dimensions.height > 0 && !initialFitDone.current) {
      const initialScale = computeAutoFitScale(dimensions.width, dimensions.height);
      setCamera((prev) => ({
        ...prev,
        scale: initialScale,
      }));
      initialFitDone.current = true;
    }
  }, [dimensions, computeAutoFitScale]);

  // Reset Camera View
  const handleResetCamera = useCallback(() => {
    const scale = computeAutoFitScale(dimensions.width, dimensions.height);
    setCamera({
      ...CAMERA_PRESETS.ISOMETRIC,
      scale,
      panX: 0,
      panY: 0,
    });
    setElevationMultiplier(1.0);
  }, [computeAutoFitScale, dimensions.width, dimensions.height]);

  // Apply Camera Presets
  const applyPreset = useCallback(
    (presetKey: keyof typeof CAMERA_PRESETS) => {
      const preset = CAMERA_PRESETS[presetKey];
      setCamera((prev) => ({
        ...prev,
        yaw: preset.yaw,
        pitch: preset.pitch,
        panX: 0,
        panY: 0,
      }));
    },
    []
  );

  // Zoom Controls
  const handleZoom = useCallback((factor: number) => {
    setCamera((prev) => ({
      ...prev,
      scale: Math.max(4, Math.min(400, prev.scale * factor)),
    }));
  }, []);

  // Main Canvas Render
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !numRows || !numCols) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const { width, height } = dimensions;

    canvas.width = Math.floor(width * dpr);
    canvas.height = Math.floor(height * dpr);

    ctx.save();
    ctx.scale(dpr, dpr);

    const isLight = theme === 'light';
    ctx.fillStyle = isLight ? '#F4F6F9' : '#0A0D12';
    ctx.fillRect(0, 0, width, height);

    // Subtle background radial depth tone
    const bgGrad = ctx.createRadialGradient(
      width / 2,
      height / 2,
      20,
      width / 2,
      height / 2,
      Math.max(width, height) * 0.7
    );
    bgGrad.addColorStop(0, isLight ? '#FFFFFF' : '#11161F');
    bgGrad.addColorStop(1, isLight ? '#E9EEF5' : '#070A0F');
    ctx.fillStyle = bgGrad;
    ctx.fillRect(0, 0, width, height);

    // 2. Base ROM Ghost Wireframe (if enabled)
    if (showGhost && baseMatrix && baseMatrix.length >= 2 && baseMatrix[0]?.length >= 2) {
      const bRows = baseMatrix.length;
      const bCols = baseMatrix[0].length;

      ctx.save();
      ctx.setLineDash([3, 4]);
      ctx.strokeStyle = isLight ? 'rgba(71, 85, 105, 0.4)' : 'rgba(148, 163, 184, 0.45)';
      ctx.lineWidth = 1;

      // Draw rows
      for (let r = 0; r < bRows; r++) {
        ctx.beginPath();
        for (let c = 0; c < bCols; c++) {
          const pt: Point3D = { x: c, y: baseMatrix[r][c], z: r };
          const proj = projectPoint(pt, center, camera, width, height, effectiveYScaling);
          if (c === 0) ctx.moveTo(proj.x, proj.y);
          else ctx.lineTo(proj.x, proj.y);
        }
        ctx.stroke();
      }

      // Draw columns
      for (let c = 0; c < bCols; c++) {
        ctx.beginPath();
        for (let r = 0; r < bRows; r++) {
          const pt: Point3D = { x: c, y: baseMatrix[r][c], z: r };
          const proj = projectPoint(pt, center, camera, width, height, effectiveYScaling);
          if (r === 0) ctx.moveTo(proj.x, proj.y);
          else ctx.lineTo(proj.x, proj.y);
        }
        ctx.stroke();
      }

      ctx.restore();
    }

    // 3. Build & Depth-Sort Surface Quads (Painter's Algorithm)
    const quads = buildQuads(matrix);
    const sortedQuads = sortQuadsByDepth(quads, center, camera, effectiveYScaling);

    // Render Quads Back-to-Front
    for (const quad of sortedQuads) {
      const p0 = projectPoint(quad.p0, center, camera, width, height, effectiveYScaling);
      const p1 = projectPoint(quad.p1, center, camera, width, height, effectiveYScaling);
      const p2 = projectPoint(quad.p2, center, camera, width, height, effectiveYScaling);
      const p3 = projectPoint(quad.p3, center, camera, width, height, effectiveYScaling);

      ctx.beginPath();
      ctx.moveTo(p0.x, p0.y);
      ctx.lineTo(p1.x, p1.y);
      ctx.lineTo(p2.x, p2.y);
      ctx.lineTo(p3.x, p3.y);
      ctx.closePath();

      // Check if this quad is within selected range
      const isSelected =
        selectedRange &&
        quad.r >= selectedRange.minR &&
        quad.r <= selectedRange.maxR &&
        quad.c >= selectedRange.minC &&
        quad.c <= selectedRange.maxC;

      if (renderMode === 'shaded') {
        // Shaded polygon
        const fillColor = getPaletteColor(
          quad.avgValue,
          minVal,
          maxVal,
          activePalette,
          isSelected ? 0.95 : 0.82
        );
        ctx.fillStyle = fillColor;
        ctx.fill();

        // Wireframe grid edge lines
        ctx.strokeStyle = isSelected ? '#38bdf8' : 'rgba(15, 23, 42, 0.45)';
        ctx.lineWidth = isSelected ? 1.8 : 0.85;
        ctx.stroke();
      } else {
        // Wireframe only mode
        const strokeColor = getPaletteColor(
          quad.avgValue,
          minVal,
          maxVal,
          activePalette,
          isSelected ? 1.0 : 0.88
        );
        ctx.strokeStyle = isSelected ? '#38bdf8' : strokeColor;
        ctx.lineWidth = isSelected ? 2.2 : 1.2;
        ctx.stroke();
      }
    }

    // 4. Highlighted Pillars & Markers for Selected Range
    if (selectedRange) {
      const rMin = Math.max(0, Math.min(selectedRange.minR, selectedRange.maxR));
      const rMax = Math.min(numRows - 1, Math.max(selectedRange.minR, selectedRange.maxR));
      const cMin = Math.max(0, Math.min(selectedRange.minC, selectedRange.maxC));
      const cMax = Math.min(numCols - 1, Math.max(selectedRange.minC, selectedRange.maxC));

      for (let r = rMin; r <= rMax; r++) {
        for (let c = cMin; c <= cMax; c++) {
          const val = matrix[r][c];
          const topPoint: Point3D = { x: c, y: val, z: r };
          const basePoint: Point3D = { x: c, y: minVal, z: r };

          const projTop = projectPoint(
            topPoint,
            center,
            camera,
            width,
            height,
            effectiveYScaling
          );
          const projBase = projectPoint(
            basePoint,
            center,
            camera,
            width,
            height,
            effectiveYScaling
          );

          // Draw vertical neon pillar
          ctx.beginPath();
          ctx.setLineDash([2, 3]);
          ctx.strokeStyle = 'rgba(56, 189, 248, 0.6)';
          ctx.lineWidth = 1.4;
          ctx.moveTo(projBase.x, projBase.y);
          ctx.lineTo(projTop.x, projTop.y);
          ctx.stroke();
          ctx.setLineDash([]);

          // Base footprint dot
          ctx.beginPath();
          ctx.arc(projBase.x, projBase.y, 2.5, 0, Math.PI * 2);
          ctx.fillStyle = 'rgba(56, 189, 248, 0.4)';
          ctx.fill();

          // Surface vertex glow ring
          ctx.beginPath();
          ctx.arc(projTop.x, projTop.y, 6, 0, Math.PI * 2);
          ctx.fillStyle = 'rgba(56, 189, 248, 0.35)';
          ctx.fill();
          ctx.strokeStyle = '#38bdf8';
          ctx.lineWidth = 1.5;
          ctx.stroke();

          // Center solid pip
          ctx.beginPath();
          ctx.arc(projTop.x, projTop.y, 2.5, 0, Math.PI * 2);
          ctx.fillStyle = '#ffffff';
          ctx.fill();
        }
      }
    }

    // 4.5. Telemetry Crosshair Indicator (Amber Neon Reticle)
    if (activeCorrelatedCell) {
      const crosshairGeom = get3DSurfaceCrosshairGeometry({
        r: activeCorrelatedCell.r,
        c: activeCorrelatedCell.c,
        matrix,
        minVal,
        center,
        camera,
        width,
        height,
        effectiveYScaling,
      });

      if (crosshairGeom && crosshairGeom.isValid) {
        const { topProj, baseProj } = crosshairGeom;

        ctx.save();

        // Vertical amber neon dashed pillar
        ctx.beginPath();
        ctx.setLineDash([3, 3]);
        ctx.strokeStyle = '#f59e0b'; // amber-500
        ctx.lineWidth = 2.0;
        ctx.moveTo(baseProj.x, baseProj.y);
        ctx.lineTo(topProj.x, topProj.y);
        ctx.stroke();
        ctx.setLineDash([]);

        // Base footprint dot & ring
        ctx.beginPath();
        ctx.arc(baseProj.x, baseProj.y, 3.5, 0, Math.PI * 2);
        ctx.fillStyle = 'rgba(245, 158, 11, 0.5)';
        ctx.fill();
        ctx.strokeStyle = '#d97706';
        ctx.lineWidth = 1.0;
        ctx.stroke();

        // Surface vertex outer target ring
        ctx.beginPath();
        ctx.arc(topProj.x, topProj.y, 9, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(245, 158, 11, 0.45)';
        ctx.lineWidth = 1.5;
        ctx.stroke();

        // Surface vertex inner target ring
        ctx.beginPath();
        ctx.arc(topProj.x, topProj.y, 5, 0, Math.PI * 2);
        ctx.strokeStyle = '#f59e0b';
        ctx.lineWidth = 2.0;
        ctx.stroke();

        // Center solid amber pip
        ctx.beginPath();
        ctx.arc(topProj.x, topProj.y, 2.5, 0, Math.PI * 2);
        ctx.fillStyle = '#fbbf24'; // amber-400
        ctx.fill();

        // Crosshair reticle ticks
        ctx.strokeStyle = '#f59e0b';
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        // North tick
        ctx.moveTo(topProj.x, topProj.y - 13);
        ctx.lineTo(topProj.x, topProj.y - 7);
        // South tick
        ctx.moveTo(topProj.x, topProj.y + 7);
        ctx.lineTo(topProj.x, topProj.y + 13);
        // West tick
        ctx.moveTo(topProj.x - 13, topProj.y);
        ctx.lineTo(topProj.x - 7, topProj.y);
        // East tick
        ctx.moveTo(topProj.x + 7, topProj.y);
        ctx.lineTo(topProj.x + 13, topProj.y);
        ctx.stroke();

        ctx.restore();
      }
    }

    // 5. Hover indicator marker
    if (hoveredCell) {
      const hVal = matrix[hoveredCell.r]?.[hoveredCell.c];
      if (hVal !== undefined) {
        const hPt: Point3D = { x: hoveredCell.c, y: hVal, z: hoveredCell.r };
        const hProj = projectPoint(
          hPt,
          center,
          camera,
          width,
          height,
          effectiveYScaling
        );

        ctx.beginPath();
        ctx.arc(hProj.x, hProj.y, 7, 0, Math.PI * 2);
        ctx.strokeStyle = '#facc15';
        ctx.lineWidth = 2;
        ctx.stroke();
      }
    }

    // 6. Draw 3D Coordinate Axis Guides
    const originPoint = projectPoint(
      { x: 0, y: minVal, z: 0 },
      center,
      camera,
      width,
      height,
      effectiveYScaling
    );
    const xEnd = projectPoint(
      { x: Math.min(3, numCols - 1), y: minVal, z: 0 },
      center,
      camera,
      width,
      height,
      effectiveYScaling
    );
    const zEnd = projectPoint(
      { x: 0, y: minVal, z: Math.min(3, numRows - 1) },
      center,
      camera,
      width,
      height,
      effectiveYScaling
    );

    ctx.save();
    ctx.lineWidth = 1.8;

    // TPS axis guide (X axis - Cyan)
    ctx.strokeStyle = '#06b6d4';
    ctx.beginPath();
    ctx.moveTo(originPoint.x, originPoint.y);
    ctx.lineTo(xEnd.x, xEnd.y);
    ctx.stroke();

    // RPM axis guide (Z axis - Emerald)
    ctx.strokeStyle = '#10b981';
    ctx.beginPath();
    ctx.moveTo(originPoint.x, originPoint.y);
    ctx.lineTo(zEnd.x, zEnd.y);
    ctx.stroke();

    ctx.font = '10px monospace';
    ctx.fillStyle = '#06b6d4';
    ctx.fillText('TPS →', xEnd.x + 4, xEnd.y + 4);

    ctx.fillStyle = '#10b981';
    ctx.fillText('RPM ↓', zEnd.x + 4, zEnd.y + 4);
    ctx.restore();

    ctx.restore();
  }, [
    dimensions,
    matrix,
    baseMatrix,
    camera,
    center,
    minVal,
    maxVal,
    effectiveYScaling,
    activePalette,
    renderMode,
    showGhost,
    selectedRange,
    hoveredCell,
    activeCorrelatedCell,
    numRows,
    numCols,
    theme,
  ]);

  // Mouse Interaction: Rotate (Left Drag), Pan (Shift+Drag / Right Drag), Zoom (Wheel)
  const isDraggingRef = useRef(false);
  const dragStartRef = useRef<{ x: number; y: number; isPan: boolean }>({
    x: 0,
    y: 0,
    isPan: false,
  });
  const hasMovedRef = useRef(false);

  const readCachedCanvasCoords = (clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const rect = canvasRectRef.current ?? canvas.getBoundingClientRect();
    if (!canvasRectRef.current) {
      canvasRectRef.current = {
        left: rect.left,
        top: rect.top,
        width: rect.width,
        height: rect.height,
      };
    }
    return {
      mouseX: clientX - rect.left,
      mouseY: clientY - rect.top,
    };
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (canvasRef.current) {
      const cr = canvasRef.current.getBoundingClientRect();
      canvasRectRef.current = { left: cr.left, top: cr.top, width: cr.width, height: cr.height };
    }
    const isPan = e.shiftKey || e.button === 2;
    isDraggingRef.current = true;
    hasMovedRef.current = false;
    dragStartRef.current = { x: e.clientX, y: e.clientY, isPan };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const pt = readCachedCanvasCoords(e.clientX, e.clientY);
    if (!pt) return;
    const { mouseX, mouseY } = pt;

    if (isDraggingRef.current) {
      const dx = e.clientX - dragStartRef.current.x;
      const dy = e.clientY - dragStartRef.current.y;

      if (Math.hypot(dx, dy) > 2) {
        hasMovedRef.current = true;
      }

      dragStartRef.current = {
        x: e.clientX,
        y: e.clientY,
        isPan: dragStartRef.current.isPan,
      };

      if (dragStartRef.current.isPan) {
        setCamera((prev) => ({
          ...prev,
          panX: prev.panX + dx,
          panY: prev.panY + dy,
        }));
      } else {
        setCamera((prev) => ({
          ...prev,
          yaw: prev.yaw + dx * 0.007,
          pitch: Math.max(
            -Math.PI / 2 + 0.05,
            Math.min(Math.PI / 2 - 0.05, prev.pitch + dy * 0.007)
          ),
        }));
      }
    } else {
      // Find closest cell under mouse for hover HUD
      const closest = findClosestCell(
        mouseX,
        mouseY,
        matrix,
        center,
        camera,
        dimensions.width,
        dimensions.height,
        effectiveYScaling
      );

      if (closest && closest.distance < 28) {
        setHoveredCell({ r: closest.r, c: closest.c });
      } else {
        setHoveredCell(null);
      }
    }
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isDraggingRef.current) return;
    isDraggingRef.current = false;
    try {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      // Ignore if pointer capture was already released
    }

    // If it was a clean click without dragging, select the clicked cell
    if (!hasMovedRef.current && onSelectCell) {
      const pt = readCachedCanvasCoords(e.clientX, e.clientY);
      if (!pt) return;
      const { mouseX, mouseY } = pt;

      const closest = findClosestCell(
        mouseX,
        mouseY,
        matrix,
        center,
        camera,
        dimensions.width,
        dimensions.height,
        effectiveYScaling
      );

      if (closest && closest.distance < 35) {
        onSelectCell(closest.r, closest.c);
      }
    }
  };

  // Wheel Zoom
  const handleWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.08 : 0.92;
    handleZoom(factor);
  };

  // Keyboard live editing when focused
  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (readOnly || !onChangeMatrix || !selectedRange) return;

    const rMin = selectedRange.minR;
    const rMax = selectedRange.maxR;
    const cMin = selectedRange.minC;
    const cMax = selectedRange.maxC;

    const step = decimals > 0 ? 0.1 : 1;

    switch (e.key) {
      case '+':
      case '=': {
        e.preventDefault();
        const adjusted = adjustSelection(matrix, rMin, rMax, cMin, cMax, step, decimals);
        onChangeMatrix(adjusted);
        break;
      }

      case '-':
      case '_': {
        e.preventDefault();
        const adjusted = adjustSelection(matrix, rMin, rMax, cMin, cMax, -step, decimals);
        onChangeMatrix(adjusted);
        break;
      }

      case '%': {
        e.preventDefault();
        const scaled = scaleSelection(matrix, rMin, rMax, cMin, cMax, 5, decimals);
        onChangeMatrix(scaled);
        break;
      }

      case 'ArrowUp': {
        e.preventDefault();
        if (onSelectCell && selectedRange) {
          const nextR = Math.max(0, selectedRange.minR - 1);
          onSelectCell(nextR, selectedRange.minC);
        }
        break;
      }

      case 'ArrowDown': {
        e.preventDefault();
        if (onSelectCell && selectedRange) {
          const nextR = Math.min(numRows - 1, selectedRange.minR + 1);
          onSelectCell(nextR, selectedRange.minC);
        }
        break;
      }

      case 'ArrowLeft': {
        e.preventDefault();
        if (onSelectCell && selectedRange) {
          const nextC = Math.max(0, selectedRange.minC - 1);
          onSelectCell(selectedRange.minR, nextC);
        }
        break;
      }

      case 'ArrowRight': {
        e.preventDefault();
        if (onSelectCell && selectedRange) {
          const nextC = Math.min(numCols - 1, selectedRange.minC + 1);
          onSelectCell(selectedRange.minR, nextC);
        }
        break;
      }
    }
  };

  // Determine active inspected cell (hovered or selected anchor)
  const activeCellCoord = useMemo(() => {
    if (hoveredCell) return hoveredCell;
    if (selectedRange) return { r: selectedRange.minR, c: selectedRange.minC };
    return null;
  }, [hoveredCell, selectedRange]);

  const activeCellValue = activeCellCoord
    ? matrix[activeCellCoord.r]?.[activeCellCoord.c]
    : null;

  const activeBaseValue =
    activeCellCoord && baseMatrix
      ? baseMatrix[activeCellCoord.r]?.[activeCellCoord.c]
      : null;

  const activeDelta =
    activeCellValue !== null &&
    activeCellValue !== undefined &&
    activeBaseValue !== null &&
    activeBaseValue !== undefined
      ? activeCellValue - activeBaseValue
      : null;

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      className="relative flex flex-col w-full h-full min-h-[460px] bg-kiviBg rounded-lg border border-kiviBorder select-none focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent focus:ring-1 focus:ring-kiviAccent/50"
    >
      {/* 3D Top Header & Camera Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 border-b border-kiviBorder bg-kiviSurface z-10">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-kiviInk tracking-wide flex items-center gap-1.5">
            <Box className="h-4 w-4 text-kiviAccent" />
            <span>{title || '3D Surface Mesh Viewer'}</span>
          </span>
          <span className="text-[11px] font-mono text-kiviMuted px-1.5 py-0.5 rounded bg-kiviElevated border border-kiviBorderStrong">
            {numRows} &times; {numCols}
          </span>
        </div>

        {/* Toolbar Controls */}
        <div className="flex items-center gap-1.5">
          {/* Preset Camera Angles */}
          <div className="flex items-center bg-kiviBg p-0.5 rounded border border-kiviBorder">
            <button
              type="button"
              title="Isometric Perspective"
              onClick={() => applyPreset('ISOMETRIC')}
              className="px-2 py-1 text-[11px] font-medium rounded text-kiviSecondary hover:text-kiviInk hover:bg-kiviElevated transition-colors flex items-center gap-1"
            >
              <Box className="h-3 w-3 text-kiviAccent" />
              <span>Iso</span>
            </button>
            <button
              type="button"
              title="Top-Down View"
              onClick={() => applyPreset('TOP_DOWN')}
              className="px-2 py-1 text-[11px] font-medium rounded text-kiviSecondary hover:text-kiviInk hover:bg-kiviElevated transition-colors flex items-center gap-1"
            >
              <Grid className="h-3 w-3 text-kiviAccent" />
              <span>Top</span>
            </button>
            <button
              type="button"
              title="Front View (RPM Elevation)"
              onClick={() => applyPreset('FRONT_RPM')}
              className="px-2 py-1 text-[11px] font-medium rounded text-kiviSecondary hover:text-kiviInk hover:bg-kiviElevated transition-colors flex items-center gap-1"
            >
              <MoveHorizontal className="h-3 w-3 text-kiviGreen" />
              <span>RPM</span>
            </button>
            <button
              type="button"
              title="Side View (TPS Elevation)"
              onClick={() => applyPreset('SIDE_TPS')}
              className="px-2 py-1 text-[11px] font-medium rounded text-kiviSecondary hover:text-kiviInk hover:bg-kiviElevated transition-colors flex items-center gap-1"
            >
              <MoveVertical className="h-3 w-3 text-kiviWarn" />
              <span>TPS</span>
            </button>
          </div>

          <div className="h-4 w-[1px] bg-kiviElevated mx-0.5" />

          {/* Shading / Wireframe Toggle */}
          <button
            type="button"
            title={renderMode === 'shaded' ? 'Switch to Wireframe Only' : 'Switch to Shaded Mesh'}
            onClick={() => setRenderMode(renderMode === 'shaded' ? 'wireframe' : 'shaded')}
            className={`px-2 py-1 text-[11px] font-medium rounded border transition-colors flex items-center gap-1 ${
              renderMode === 'shaded'
                ? 'bg-kiviElevated text-kiviAccent border-kiviBorderStrong'
                : 'bg-kiviAccent/15 text-kiviAccent border-kiviAccent/40'
            }`}
          >
            <Layers className="h-3 w-3" />
            <span>{renderMode === 'shaded' ? 'Shaded' : 'Wireframe'}</span>
          </button>

          {/* Base Ghost Wireframe Toggle */}
          {baseMatrix && (
            <button
              type="button"
              title="Toggle Base ROM Ghost Wireframe"
              onClick={() => setShowGhost(!showGhost)}
              className={`px-2 py-1 text-[11px] font-medium rounded border transition-colors flex items-center gap-1 ${
                showGhost
                  ? 'bg-kiviWarn/15 text-kiviWarn border-kiviWarn/40'
                  : 'bg-kiviSurface text-kiviMuted border-kiviBorder'
              }`}
            >
              <Ghost className="h-3 w-3" />
              <span>Ghost</span>
            </button>
          )}

          {/* Color Palette Selector */}
          <div className="flex items-center gap-1 bg-kiviBg px-1 py-0.5 rounded border border-kiviBorder">
            <Palette className="h-3 w-3 text-kiviMuted ml-1" />
            <select
              value={activePalette}
              onChange={(e) => setActivePalette(e.target.value as SurfacePalette)}
              className="bg-transparent text-[11px] font-medium text-kiviSecondary focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent cursor-pointer py-0.5 pr-1"
            >
              <option value="classic" className="bg-kiviSurface text-kiviInk">
                Classic
              </option>
              <option value="thermal" className="bg-kiviSurface text-kiviInk">
                Thermal
              </option>
              <option value="spectrum" className="bg-kiviSurface text-kiviInk">
                Spectrum
              </option>
              <option value="monochrome" className="bg-kiviSurface text-kiviInk">
                Monochrome
              </option>
            </select>
          </div>

          <div className="h-4 w-[1px] bg-kiviElevated mx-0.5" />

          {/* Zoom Buttons */}
          <button
            type="button"
            title="Zoom In"
            onClick={() => handleZoom(1.15)}
            className="p-1 rounded text-kiviMuted hover:text-kiviInk hover:bg-kiviElevated transition-colors"
          >
            <ZoomIn className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            title="Zoom Out"
            onClick={() => handleZoom(0.85)}
            className="p-1 rounded text-kiviMuted hover:text-kiviInk hover:bg-kiviElevated transition-colors"
          >
            <ZoomOut className="h-3.5 w-3.5" />
          </button>

          {/* Reset Camera Button */}
          <button
            type="button"
            title="Reset Camera & Zoom"
            onClick={handleResetCamera}
            className="p-1 rounded text-kiviMuted hover:text-kiviInk hover:bg-kiviElevated transition-colors"
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Main Canvas Viewport */}
      <div className="relative flex-1 w-full h-full min-h-[380px] overflow-hidden">
        <canvas
          ref={canvasRef}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onWheel={handleWheel}
          onContextMenu={(e) => e.preventDefault()}
          className="w-full h-full cursor-grab active:cursor-grabbing block touch-none"
        />

        {/* Live HUD / Coordinates Pill Card */}
        {activeCellCoord && activeCellValue !== null && activeCellValue !== undefined && (
          <div className="pointer-events-none absolute top-3 left-3 z-20 min-w-[190px] rounded-lg border border-kiviBorderStrong bg-kiviSurface p-2.5 shadow-xl tabular-nums">
            <div className="mb-1.5 flex items-center justify-between gap-2 border-b border-kiviBorder pb-1 text-[11px] font-semibold text-kiviInk">
              <span>
                Cell [{activeCellCoord.r}, {activeCellCoord.c}]
              </span>
              <span className="font-mono text-[10px] text-kiviAccent">
                {unit}
              </span>
            </div>

            <div className="space-y-1 font-mono text-[11px]">
              <div className="flex justify-between text-kiviSecondary">
                <span className="text-kiviMuted">{yAxisName}:</span>
                <span>{yLabels?.[activeCellCoord.r] ?? activeCellCoord.r}</span>
              </div>
              <div className="flex justify-between text-kiviSecondary">
                <span className="text-kiviMuted">{xAxisName}:</span>
                <span>{xLabels?.[activeCellCoord.c] ?? activeCellCoord.c}</span>
              </div>
              <div className="flex justify-between font-bold text-kiviInk pt-0.5 border-t border-kiviBorder">
                <span className="text-kiviMuted font-normal">Value:</span>
                <span className="text-kiviAccent">
                  {typeof activeCellValue === 'number'
                    ? activeCellValue.toFixed(decimals)
                    : activeCellValue}
                </span>
              </div>

              {activeDelta !== null && (
                <div className="flex justify-between items-center text-[10px] pt-0.5">
                  <span className="text-kiviMuted">Delta:</span>
                  <span
                    className={`font-semibold px-1 rounded ${
                      activeDelta > 0
                        ? 'text-kiviGreen bg-kiviGreen/10'
                        : activeDelta < 0
                        ? 'text-kiviRed bg-kiviRed/10'
                        : 'text-kiviMuted'
                    }`}
                  >
                    {activeDelta > 0 ? `+${activeDelta.toFixed(decimals)}` : activeDelta.toFixed(decimals)}
                  </span>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Floating Elevation Multiplier Slider */}
        <div className="absolute bottom-3 left-3 flex items-center gap-2 bg-kiviSurface border border-kiviBorder px-2.5 py-1.5 rounded-lg text-xs text-kiviSecondary shadow-md">
          <Sliders className="h-3 w-3 text-kiviAccent" />
          <span className="text-[11px] font-medium text-kiviMuted">Elevation:</span>
          <input
            type="range"
            min="0.2"
            max="3.0"
            step="0.1"
            value={elevationMultiplier}
            onChange={(e) => setElevationMultiplier(parseFloat(e.target.value))}
            className="w-20 h-1 bg-kiviBorderStrong rounded-lg appearance-none cursor-pointer accent-kiviAccent"
          />
          <span className="font-mono text-[10px] text-kiviAccent w-8 tabular-nums">
            {elevationMultiplier.toFixed(1)}&times;
          </span>
        </div>

        {/* Live Shortcut Quick Help Pill */}
        {!readOnly && onChangeMatrix && selectedRange && (
          <div className="absolute bottom-3 right-3 flex items-center gap-2 bg-kiviSurface border border-kiviBorder px-3 py-1.5 rounded-lg text-[11px] text-kiviMuted shadow-md">
            <span className="text-kiviSecondary font-medium">Edit:</span>
            <span className="font-mono bg-kiviElevated px-1 py-0.5 rounded text-kiviInk">
              +
            </span>
            <span className="font-mono bg-kiviElevated px-1 py-0.5 rounded text-kiviInk">
              -
            </span>
            <span className="font-mono bg-kiviElevated px-1 py-0.5 rounded text-kiviInk">
              %
            </span>
            <span className="text-kiviMuted">|</span>
            <span className="text-kiviSecondary font-medium">Nav:</span>
            <span className="font-mono bg-kiviElevated px-1 py-0.5 rounded text-kiviInk">
              Arrows
            </span>
          </div>
        )}
      </div>
    </div>
  );
};
