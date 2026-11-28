import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import {
  Box,
  Compass,
  Layers,
  RotateCcw,
  Sliders,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import type { SelectionRange } from './HeatmapTable';
import {
  ViewportIconMenu,
  ViewportIconPopover,
} from './surface3d/ViewportOverlayControls';
import {
  CAMERA_PRESETS,
  buildQuads,
  calculateAutoYScaling,
  calculateMatrixCenter,
  computeAxisTickIndices,
  computeQuadShade,
  findClosestCell,
  getMatrixExtremes,
  getPaletteColor,
  getShadedPaletteColor,
  normalizeMatrixFor3D,
  projectPoint,
  sortQuadsByDepth,
  type CameraState,
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
  headerActions?: React.ReactNode;
}

type SurfaceRenderMode = 'shaded' | 'smooth' | 'wireframe';
type CameraPresetKey = keyof typeof CAMERA_PRESETS;

function formatCompactTick(val: number, decimals: number): string {
  if (!Number.isFinite(val)) return '0';
  if (Math.abs(val) >= 1000 && decimals === 0) return Math.round(val).toString();
  const fixed = val.toFixed(Math.min(decimals, 2));
  return fixed.replace(/\.00$/, '');
}

function drawPolygon(ctx: CanvasRenderingContext2D, pts: Array<{ x: number; y: number }>): void {
  if (pts.length === 0) return;
  ctx.beginPath();
  ctx.moveTo(pts[0].x, pts[0].y);
  for (let i = 1; i < pts.length; i++) {
    ctx.lineTo(pts[i].x, pts[i].y);
  }
  ctx.closePath();
}

export const Surface3DView: React.FC<Surface3DViewProps> = React.memo(({
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
  headerActions,
}) => {
  const canvasHostRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const canvasRectRef = useRef<{ left: number; top: number; width: number; height: number } | null>(
    null
  );
  const hoverRafRef = useRef<number | null>(null);
  const pendingPointerRef = useRef<{ x: number; y: number } | null>(null);
  const theme = useThemeStore((s) => s.theme);

  const normalized = useMemo(() => normalizeMatrixFor3D(matrix), [matrix]);
  const normalizedBase = useMemo(
    () => (baseMatrix ? normalizeMatrixFor3D(baseMatrix) : null),
    [baseMatrix]
  );

  const renderMatrix = normalized.matrix;
  const renderBaseMatrix =
    normalizedBase && normalizedBase.matrix.length >= 2 ? normalizedBase.matrix : undefined;

  const origRows = matrix?.length ?? 0;
  const origCols = matrix?.[0]?.length ?? 0;
  const numRows = renderMatrix.length;
  const numCols = renderMatrix[0]?.length ?? 0;

  // Map axis labels cleanly when 1D column curves are transposed into 2xN ribbons.
  const effectiveXLabels = useMemo(() => {
    if (normalized.transpose1D) return yLabels;
    return xLabels;
  }, [normalized.transpose1D, xLabels, yLabels]);

  const effectiveYLabels = useMemo(() => {
    if (normalized.is1DRibbon) return ['0', '1'];
    return yLabels;
  }, [normalized.is1DRibbon, yLabels]);

  const effectiveXAxisName = normalized.transpose1D ? yAxisName : xAxisName;
  const effectiveYAxisName = normalized.is1DRibbon ? '' : yAxisName;

  const activeCorrelation = useCorrelationStore((s) => s.activeCorrelation);
  const targetTableId = useCorrelationStore((s) => s.targetTableId);

  const activeCorrelatedCell: CellCoordinate | null = useMemo(() => {
    if (propCorrelatedCell !== undefined) {
      if (
        propCorrelatedCell &&
        propCorrelatedCell.r >= 0 &&
        propCorrelatedCell.r < origRows &&
        propCorrelatedCell.c >= 0 &&
        propCorrelatedCell.c < origCols
      ) {
        return propCorrelatedCell;
      }
      return null;
    }
    return resolveCorrelatedCell({
      matrixDimensions: { numRows: origRows, numCols: origCols },
      activeCorrelation,
      targetTableId,
      currentTableId: tableId,
    });
  }, [propCorrelatedCell, origRows, origCols, activeCorrelation, targetTableId, tableId]);

  const [dimensions, setDimensions] = useState<{ width: number; height: number }>({
    width: 600,
    height: 360,
  });

  const [activePalette, setActivePalette] = useState<SurfacePalette>(initialPalette);
  const [renderMode, setRenderMode] = useState<SurfaceRenderMode>('shaded');
  const [showCage, setShowCage] = useState<boolean>(true);
  const [showGhost, setShowGhost] = useState<boolean>(true);
  const [showColorScale, setShowColorScale] = useState<boolean>(true);
  const [elevationMultiplier, setElevationMultiplier] = useState<number>(1.0);
  const [activePreset, setActivePreset] = useState<CameraPresetKey | null>('ISOMETRIC');
  const [hoveredCell, setHoveredCell] = useState<{ r: number; c: number } | null>(null);

  useEffect(() => {
    setActivePalette(initialPalette);
  }, [initialPalette]);

  const [camera, setCamera] = useState<CameraState>(() => ({
    ...CAMERA_PRESETS.ISOMETRIC,
    scale: 24,
    panX: 0,
    panY: 0,
  }));

  const center = useMemo(() => calculateMatrixCenter(renderMatrix), [renderMatrix]);
  const extremes = useMemo(() => getMatrixExtremes(renderMatrix), [renderMatrix]);
  const baseExtremes = useMemo(
    () => (renderBaseMatrix ? getMatrixExtremes(renderBaseMatrix) : null),
    [renderBaseMatrix]
  );

  const minVal = useMemo(
    () => (baseExtremes ? Math.min(extremes.min, baseExtremes.min) : extremes.min),
    [extremes.min, baseExtremes]
  );
  const maxVal = useMemo(
    () => (baseExtremes ? Math.max(extremes.max, baseExtremes.max) : extremes.max),
    [extremes.max, baseExtremes]
  );

  const quads = useMemo(() => buildQuads(renderMatrix), [renderMatrix]);
  const baseYScaling = useMemo(() => calculateAutoYScaling(renderMatrix), [renderMatrix]);
  const effectiveYScaling = baseYScaling * elevationMultiplier;

  const computeAutoFitScale = useCallback(
    (w: number, h: number): number => {
      const spanX = Math.max(numCols - 1, 2);
      const spanZ = Math.max(numRows - 1, 2);
      const span = Math.max(spanX, spanZ, 5);
      const reserveLeft = w >= 340 ? 48 : 20;
      const reserveRight = 24;
      const usableW = Math.max(120, w - reserveLeft - reserveRight);
      const usableH = Math.max(110, h - 46);
      return Math.max(6, Math.min(260, Math.min(usableW, usableH) / (span * 1.52)));
    },
    [numRows, numCols]
  );

  // Measure only the dedicated canvas host element so the canvas NEVER inflates container height.
  useEffect(() => {
    const host = canvasHostRef.current;
    if (!host) return;

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
      const w = Math.max(220, Math.min(4096, Math.floor(host.clientWidth)));
      const h = Math.max(130, Math.min(4096, Math.floor(host.clientHeight)));
      setDimensions((prev) => (prev.width === w && prev.height === h ? prev : { width: w, height: h }));
      refreshRect();
    };

    updateSize();
    const observer = new ResizeObserver(updateSize);
    observer.observe(host);
    window.addEventListener('resize', refreshRect, { passive: true });
    window.addEventListener('scroll', refreshRect, { capture: true, passive: true });

    return () => {
      observer.disconnect();
      window.removeEventListener('resize', refreshRect);
      window.removeEventListener('scroll', refreshRect, { capture: true });
      if (hoverRafRef.current !== null) cancelAnimationFrame(hoverRafRef.current);
      if (canvasRef.current) {
        canvasRef.current.width = 0;
        canvasRef.current.height = 0;
      }
    };
  }, []);

  // Re-fit scale on table dimension change or initial mount.
  const fitSignatureRef = useRef<string>('');
  useEffect(() => {
    const sig = `${tableId ?? 'none'}:${numRows}x${numCols}`;
    if (dimensions.width > 0 && dimensions.height > 0 && fitSignatureRef.current !== sig) {
      const initialScale = computeAutoFitScale(dimensions.width, dimensions.height);
      setCamera((prev) => ({
        ...prev,
        scale: initialScale,
        panX: dimensions.width >= 340 ? 12 : 0,
        panY: 4,
      }));
      fitSignatureRef.current = sig;
    }
  }, [tableId, numRows, numCols, dimensions, computeAutoFitScale]);

  const handleResetCamera = useCallback(() => {
    const scale = computeAutoFitScale(dimensions.width, dimensions.height);
    setCamera({
      ...CAMERA_PRESETS.ISOMETRIC,
      scale,
      panX: dimensions.width >= 340 ? 12 : 0,
      panY: 4,
    });
    setActivePreset('ISOMETRIC');
    setElevationMultiplier(1.0);
  }, [computeAutoFitScale, dimensions.width, dimensions.height]);

  const applyPreset = useCallback(
    (presetKey: CameraPresetKey) => {
      const preset = CAMERA_PRESETS[presetKey];
      setActivePreset(presetKey);
      setCamera((prev) => ({
        ...prev,
        yaw: preset.yaw,
        pitch: preset.pitch,
        panX: dimensions.width >= 340 ? 12 : 0,
        panY: 4,
      }));
    },
    [dimensions.width]
  );

  const handleZoom = useCallback((factor: number) => {
    setCamera((prev) => ({
      ...prev,
      scale: Math.max(4, Math.min(400, prev.scale * factor)),
    }));
  }, []);

  // Native non-passive wheel handler prevents parent scroll hijacking while zooming 3D surface.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const factor = e.deltaY < 0 ? 1.09 : 0.91;
      handleZoom(factor);
    };
    canvas.addEventListener('wheel', onWheel, { passive: false });
    return () => canvas.removeEventListener('wheel', onWheel);
  }, [handleZoom]);

  const mapRenderToOrigCoord = useCallback(
    (r: number, c: number): { r: number; c: number } => {
      if (normalized.is1DRibbon) {
        return normalized.transpose1D ? { r: c, c: 0 } : { r: 0, c };
      }
      return { r, c };
    },
    [normalized.is1DRibbon, normalized.transpose1D]
  );

  const mapOrigToRenderCoord = useCallback(
    (r: number, c: number): { r: number; c: number } => {
      if (normalized.is1DRibbon) {
        return normalized.transpose1D ? { r: 0, c: r } : { r: 0, c };
      }
      return { r, c };
    },
    [normalized.is1DRibbon, normalized.transpose1D]
  );

  // Main Canvas Render
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || numRows < 2 || numCols < 2) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const { width, height } = dimensions;

    const pixelW = Math.floor(width * dpr);
    const pixelH = Math.floor(height * dpr);
    if (canvas.width !== pixelW || canvas.height !== pixelH) {
      canvas.width = pixelW;
      canvas.height = pixelH;
    }

    ctx.save();
    ctx.scale(dpr, dpr);

    const isLight = theme === 'light';
    const bgColor = isLight ? '#F4F6F9' : '#0A0D12';
    const plateFill = isLight ? 'rgba(226, 232, 240, 0.55)' : 'rgba(17, 22, 31, 0.72)';
    const wallFill = isLight ? 'rgba(241, 245, 249, 0.72)' : 'rgba(15, 20, 29, 0.72)';
    const cageBorder = isLight ? 'rgba(100, 116, 139, 0.55)' : 'rgba(71, 85, 105, 0.65)';
    const gridMajor = isLight ? 'rgba(148, 163, 184, 0.45)' : 'rgba(51, 65, 85, 0.55)';
    const labelColor = isLight ? '#334155' : '#94A3B8';
    const accentColor = isLight ? '#0369A1' : '#38BDF8';
    const warnColor = isLight ? '#B45309' : '#FBBF24';

    ctx.fillStyle = bgColor;
    ctx.fillRect(0, 0, width, height);

    const maxC = numCols - 1;
    const maxR = numRows - 1;
    const midX = maxC / 2;
    const midZ = maxR / 2;
    const spanY = Math.max(maxVal - minVal, 1e-6);
    const topBoxVal = minVal + spanY * 1.05;

    const proj = (x: number, y: number, z: number) =>
      projectPoint({ x, y, z }, center, camera, width, height, effectiveYScaling);

    const maxTicks = width < 480 ? 5 : 7;
    const xTicks = computeAxisTickIndices(numCols, Math.min(maxTicks, numCols));
    const zTicks = computeAxisTickIndices(numRows, Math.min(maxTicks, numRows));

    // Determine back vs front walls cleanly from projected zDepth so walls never occlude mesh.
    const x0Depth = proj(0, minVal, midZ).zDepth;
    const x1Depth = proj(maxC, minVal, midZ).zDepth;
    const backX = x0Depth <= x1Depth ? 0 : maxC;
    const frontX = backX === 0 ? maxC : 0;

    const z0Depth = proj(midX, minVal, 0).zDepth;
    const z1Depth = proj(midX, minVal, maxR).zDepth;
    const backZ = z0Depth <= z1Depth ? 0 : maxR;
    const frontZ = backZ === 0 ? maxR : 0;

    // 1. 3D Calibration Cage (Floor + Camera-Aware Back Walls + Ticks)
    if (showCage) {
      const f00 = proj(0, minVal, 0);
      const f10 = proj(maxC, minVal, 0);
      const f11 = proj(maxC, minVal, maxR);
      const f01 = proj(0, minVal, maxR);

      let zMinLabelPos: { x: number; y: number } | null = null;

      ctx.save();
      drawPolygon(ctx, [f00, f10, f11, f01]);
      ctx.fillStyle = plateFill;
      ctx.fill();

      // Back wall perpendicular to X (`x = backX`)
      if (Math.abs(camera.pitch) < Math.PI / 2 - 0.14) {
        const wA = proj(backX, minVal, 0);
        const wB = proj(backX, minVal, maxR);
        const wC = proj(backX, topBoxVal, maxR);
        const wD = proj(backX, topBoxVal, 0);
        drawPolygon(ctx, [wA, wB, wC, wD]);
        ctx.fillStyle = wallFill;
        ctx.fill();
        ctx.strokeStyle = cageBorder;
        ctx.lineWidth = 1;
        ctx.stroke();

        // Back wall perpendicular to Z (`z = backZ`)
        const zA = proj(0, minVal, backZ);
        const zB = proj(maxC, minVal, backZ);
        const zC = proj(maxC, topBoxVal, backZ);
        const zD = proj(0, topBoxVal, backZ);
        drawPolygon(ctx, [zA, zB, zC, zD]);
        ctx.fillStyle = wallFill;
        ctx.fill();
        ctx.strokeStyle = cageBorder;
        ctx.stroke();

        // Z elevation gridlines + numeric labels on the leftmost outer cage vertical edge
        ctx.font = '10px "JetBrains Mono", ui-monospace, monospace';
        const edgeA = proj(frontX, minVal, backZ);
        const edgeB = proj(backX, minVal, frontZ);
        const useEdgeA = edgeA.x <= edgeB.x;
        const zRailX = useEdgeA ? frontX : backX;
        const zRailZ = useEdgeA ? backZ : frontZ;
        const steps = 4;
        for (let s = 0; s <= steps; s++) {
          const v = minVal + (spanY * s) / steps;
          ctx.beginPath();
          const a = proj(backX, v, 0);
          const b = proj(backX, v, maxR);
          const c = proj(0, v, backZ);
          const d = proj(maxC, v, backZ);
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.moveTo(c.x, c.y);
          ctx.lineTo(d.x, d.y);
          ctx.strokeStyle = gridMajor;
          ctx.lineWidth = s === 0 ? 1.1 : 0.75;
          ctx.stroke();

          if (width >= 300) {
            const lp = proj(zRailX, v, zRailZ);
            if (lp.x >= 26) {
              const labelX = lp.x - 6;
              const labelY = lp.y;
              ctx.fillStyle = labelColor;
              ctx.textAlign = 'right';
              ctx.textBaseline = 'middle';
              ctx.fillText(formatCompactTick(v, decimals), labelX, labelY);
              if (s === 0) {
                zMinLabelPos = { x: labelX, y: labelY };
              }
            }
          }
        }
      }

      // Floor grid lines aligned with X & Z ticks
      ctx.beginPath();
      for (const cIdx of xTicks) {
        const a = proj(cIdx, minVal, 0);
        const b = proj(cIdx, minVal, maxR);
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(b.x, b.y);
      }
      for (const rIdx of zTicks) {
        const a = proj(0, minVal, rIdx);
        const b = proj(maxC, minVal, rIdx);
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(b.x, b.y);
      }
      ctx.strokeStyle = gridMajor;
      ctx.lineWidth = 0.75;
      ctx.stroke();

      // Floor outer outline
      drawPolygon(ctx, [f00, f10, f11, f01]);
      ctx.strokeStyle = cageBorder;
      ctx.lineWidth = 1.1;
      ctx.stroke();

      // X-axis tick labels along frontZ (de-duped against Z-min label and nearby ticks)
      const xSlopeDownRight = proj(maxC, minVal, frontZ).y >= proj(0, minVal, frontZ).y;
      const xAlign: CanvasTextAlign = xSlopeDownRight ? 'right' : 'left';
      const xTickDx = xSlopeDownRight ? -6 : 6;
      ctx.font = '10px "JetBrains Mono", ui-monospace, monospace';
      ctx.fillStyle = labelColor;
      ctx.textAlign = xAlign;
      ctx.textBaseline = 'top';
      let lastXLabelPos: { x: number; y: number } | null = null;
      for (const cIdx of xTicks) {
        const pTick = proj(cIdx, minVal, frontZ);
        const lx = pTick.x + xTickDx;
        const ly = pTick.y + 5;
        if (zMinLabelPos && Math.hypot(lx - zMinLabelPos.x, ly - zMinLabelPos.y) < 26) continue;
        if (lastXLabelPos && Math.hypot(lx - lastXLabelPos.x, ly - lastXLabelPos.y) < 22) continue;
        const rawLbl = effectiveXLabels?.[cIdx] ?? cIdx;
        ctx.fillText(String(rawLbl), lx, ly);
        lastXLabelPos = { x: lx, y: ly };
      }
      if (effectiveXAxisName) {
        const xTitlePos = proj(midX, minVal, frontZ);
        ctx.fillStyle = accentColor;
        ctx.font = '600 10px "JetBrains Mono", ui-monospace, monospace';
        ctx.textAlign = xAlign;
        ctx.fillText(
          effectiveXAxisName,
          xTitlePos.x + (xSlopeDownRight ? -26 : 26),
          xTitlePos.y + 24
        );
      }

      // Y/Row-axis tick labels along frontX (only for genuine 2D maps)
      if (!normalized.is1DRibbon) {
        const labelOnLeft = proj(frontX, minVal, midZ).x < width / 2;
        const yAlign: CanvasTextAlign = labelOnLeft ? 'right' : 'left';
        ctx.font = '10px "JetBrains Mono", ui-monospace, monospace';
        ctx.fillStyle = labelColor;
        ctx.textAlign = yAlign;
        ctx.textBaseline = 'top';
        let lastYLabelPos: { x: number; y: number } | null = null;
        for (const rIdx of zTicks) {
          const pTick = proj(frontX, minVal, rIdx);
          const lx = pTick.x + (labelOnLeft ? -8 : 8);
          const ly = pTick.y + 5;
          if (zMinLabelPos && Math.hypot(lx - zMinLabelPos.x, ly - zMinLabelPos.y) < 26) continue;
          if (lastYLabelPos && Math.hypot(lx - lastYLabelPos.x, ly - lastYLabelPos.y) < 22) continue;
          const rawLbl = effectiveYLabels?.[rIdx] ?? rIdx;
          ctx.fillText(String(rawLbl), lx, ly);
          lastYLabelPos = { x: lx, y: ly };
        }
        if (effectiveYAxisName) {
          const yTitlePos = proj(frontX, minVal, midZ);
          ctx.fillStyle = accentColor;
          ctx.font = '600 10px "JetBrains Mono", ui-monospace, monospace';
          ctx.textAlign = yAlign;
          ctx.fillText(
            effectiveYAxisName,
            yTitlePos.x + (labelOnLeft ? -26 : 26),
            yTitlePos.y + 24
          );
        }
      }
      ctx.restore();
    }

    // 2. Base ROM Ghost Wireframe (if enabled)
    if (showGhost && renderBaseMatrix) {
      const bRows = renderBaseMatrix.length;
      const bCols = renderBaseMatrix[0].length;
      ctx.save();
      ctx.setLineDash([3, 3]);
      ctx.strokeStyle = isLight ? 'rgba(71, 85, 105, 0.42)' : 'rgba(148, 163, 184, 0.42)';
      ctx.lineWidth = 1;
      for (let r = 0; r < bRows; r++) {
        ctx.beginPath();
        for (let c = 0; c < bCols; c++) {
          const p = proj(c, renderBaseMatrix[r][c], r);
          if (c === 0) ctx.moveTo(p.x, p.y);
          else ctx.lineTo(p.x, p.y);
        }
        ctx.stroke();
      }
      for (let c = 0; c < bCols; c++) {
        ctx.beginPath();
        for (let r = 0; r < bRows; r++) {
          const p = proj(c, renderBaseMatrix[r][c], r);
          if (r === 0) ctx.moveTo(p.x, p.y);
          else ctx.lineTo(p.x, p.y);
        }
        ctx.stroke();
      }
      ctx.restore();
    }

    // 3. Subtle Base Skirt Walls along Front Perimeter (solid CAD volume anchor)
    if (renderMode !== 'wireframe' && showCage) {
      ctx.save();
      const skirtFill = isLight ? 'rgba(100, 116, 139, 0.14)' : 'rgba(56, 189, 248, 0.10)';
      const skirtEdge = isLight ? 'rgba(71, 85, 105, 0.28)' : 'rgba(56, 189, 248, 0.22)';
      ctx.fillStyle = skirtFill;
      ctx.strokeStyle = skirtEdge;
      ctx.lineWidth = 0.8;

      for (let c = 0; c < maxC; c++) {
        const b0 = proj(c, minVal, frontZ);
        const b1 = proj(c + 1, minVal, frontZ);
        const t1 = proj(c + 1, renderMatrix[frontZ][c + 1], frontZ);
        const t0 = proj(c, renderMatrix[frontZ][c], frontZ);
        drawPolygon(ctx, [b0, b1, t1, t0]);
        ctx.fill();
        ctx.stroke();
      }
      for (let r = 0; r < maxR; r++) {
        const b0 = proj(frontX, minVal, r);
        const b1 = proj(frontX, minVal, r + 1);
        const t1 = proj(frontX, renderMatrix[r + 1][frontX], r + 1);
        const t0 = proj(frontX, renderMatrix[r][frontX], r);
        drawPolygon(ctx, [b0, b1, t1, t0]);
        ctx.fill();
        ctx.stroke();
      }
      ctx.restore();
    }

    // 4. Depth-Sorted Sculpted Surface Quads (Painter's Algorithm + Directional Lighting)
    const sortedQuads = sortQuadsByDepth(quads, center, camera, effectiveYScaling);
    for (const quad of sortedQuads) {
      const p0 = projectPoint(quad.p0, center, camera, width, height, effectiveYScaling);
      const p1 = projectPoint(quad.p1, center, camera, width, height, effectiveYScaling);
      const p2 = projectPoint(quad.p2, center, camera, width, height, effectiveYScaling);
      const p3 = projectPoint(quad.p3, center, camera, width, height, effectiveYScaling);

      drawPolygon(ctx, [p0, p1, p2, p3]);

      const origCorner = mapRenderToOrigCoord(quad.r, quad.c);
      const isSelected = Boolean(
        selectedRange &&
          origCorner.r >= selectedRange.minR &&
          origCorner.r <= selectedRange.maxR &&
          origCorner.c >= selectedRange.minC &&
          origCorner.c <= selectedRange.maxC
      );

      if (renderMode === 'wireframe') {
        const strokeColor = getPaletteColor(
          quad.avgValue,
          minVal,
          maxVal,
          activePalette,
          isSelected ? 1.0 : 0.9
        );
        ctx.strokeStyle = isSelected ? accentColor : strokeColor;
        ctx.lineWidth = isSelected ? 1.8 : 1.15;
        ctx.stroke();
      } else {
        const shade = computeQuadShade(quad, effectiveYScaling);
        ctx.fillStyle = getShadedPaletteColor(
          quad.avgValue,
          minVal,
          maxVal,
          activePalette,
          shade,
          isSelected ? 0.98 : 0.92
        );
        ctx.fill();

        if (isSelected) {
          ctx.strokeStyle = accentColor;
          ctx.lineWidth = 1.6;
        } else if (renderMode === 'shaded') {
          ctx.strokeStyle = isLight ? 'rgba(15, 23, 42, 0.32)' : 'rgba(10, 13, 18, 0.55)';
          ctx.lineWidth = 0.8;
        } else {
          ctx.strokeStyle = isLight ? 'rgba(15, 23, 42, 0.12)' : 'rgba(255, 255, 255, 0.08)';
          ctx.lineWidth = 0.45;
        }
        ctx.stroke();
      }
    }

    // 5. Selected Range Vertex Markers
    if (selectedRange) {
      const rMin = Math.max(0, Math.min(selectedRange.minR, selectedRange.maxR));
      const rMax = Math.min(origRows - 1, Math.max(selectedRange.minR, selectedRange.maxR));
      const cMin = Math.max(0, Math.min(selectedRange.minC, selectedRange.maxC));
      const cMax = Math.min(origCols - 1, Math.max(selectedRange.minC, selectedRange.maxC));
      const selCount = (rMax - rMin + 1) * (cMax - cMin + 1);

      for (let r = rMin; r <= rMax; r++) {
        for (let c = cMin; c <= cMax; c++) {
          const val = matrix[r]?.[c];
          if (val === undefined) continue;
          const mapped = mapOrigToRenderCoord(r, c);
          const projTop = proj(mapped.c, val, mapped.r);
          const projBase = proj(mapped.c, minVal, mapped.r);

          if (selCount <= 16) {
            ctx.beginPath();
            ctx.setLineDash([2, 3]);
            ctx.strokeStyle = accentColor;
            ctx.lineWidth = 1.1;
            ctx.moveTo(projBase.x, projBase.y);
            ctx.lineTo(projTop.x, projTop.y);
            ctx.stroke();
            ctx.setLineDash([]);
          }

          ctx.beginPath();
          ctx.arc(projTop.x, projTop.y, selCount <= 4 ? 4.5 : 3.0, 0, Math.PI * 2);
          ctx.fillStyle = isLight ? '#FFFFFF' : '#0A0D12';
          ctx.fill();
          ctx.strokeStyle = accentColor;
          ctx.lineWidth = 1.8;
          ctx.stroke();
        }
      }
    }

    // 6. Live Telemetry Crosshair Needle
    if (activeCorrelatedCell) {
      const mapped = mapOrigToRenderCoord(activeCorrelatedCell.r, activeCorrelatedCell.c);
      const crosshairGeom = get3DSurfaceCrosshairGeometry({
        r: mapped.r,
        c: mapped.c,
        matrix: renderMatrix,
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
        ctx.beginPath();
        ctx.setLineDash([3, 3]);
        ctx.strokeStyle = warnColor;
        ctx.lineWidth = 1.6;
        ctx.moveTo(baseProj.x, baseProj.y);
        ctx.lineTo(topProj.x, topProj.y);
        ctx.stroke();
        ctx.setLineDash([]);

        ctx.beginPath();
        ctx.arc(topProj.x, topProj.y, 6, 0, Math.PI * 2);
        ctx.strokeStyle = warnColor;
        ctx.lineWidth = 1.8;
        ctx.stroke();

        ctx.beginPath();
        ctx.arc(topProj.x, topProj.y, 2.2, 0, Math.PI * 2);
        ctx.fillStyle = warnColor;
        ctx.fill();
        ctx.restore();
      }
    }

    // 7. Hover Target Pin
    if (hoveredCell) {
      const hVal = matrix[hoveredCell.r]?.[hoveredCell.c];
      if (hVal !== undefined) {
        const mapped = mapOrigToRenderCoord(hoveredCell.r, hoveredCell.c);
        const hProj = proj(mapped.c, hVal, mapped.r);
        ctx.beginPath();
        ctx.arc(hProj.x, hProj.y, 5.5, 0, Math.PI * 2);
        ctx.strokeStyle = accentColor;
        ctx.lineWidth = 1.8;
        ctx.stroke();
      }
    }

    ctx.restore();
  }, [
    dimensions,
    matrix,
    renderMatrix,
    renderBaseMatrix,
    quads,
    camera,
    center,
    minVal,
    maxVal,
    effectiveYScaling,
    activePalette,
    renderMode,
    showCage,
    showGhost,
    selectedRange,
    hoveredCell,
    activeCorrelatedCell,
    numRows,
    numCols,
    origRows,
    origCols,
    effectiveXLabels,
    effectiveYLabels,
    effectiveXAxisName,
    effectiveYAxisName,
    normalized.is1DRibbon,
    unit,
    decimals,
    theme,
    mapRenderToOrigCoord,
    mapOrigToRenderCoord,
  ]);

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

    if (isDraggingRef.current) {
      const dx = e.clientX - dragStartRef.current.x;
      const dy = e.clientY - dragStartRef.current.y;
      if (Math.hypot(dx, dy) > 2) {
        hasMovedRef.current = true;
        setActivePreset(null);
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
          pitch: Math.max(-Math.PI / 2 + 0.05, Math.min(Math.PI / 2 - 0.05, prev.pitch + dy * 0.007)),
        }));
      }
      return;
    }

    pendingPointerRef.current = { x: pt.mouseX, y: pt.mouseY };
    if (hoverRafRef.current !== null) return;
    hoverRafRef.current = requestAnimationFrame(() => {
      hoverRafRef.current = null;
      const coords = pendingPointerRef.current;
      if (!coords) return;
      const closest = findClosestCell(
        coords.x,
        coords.y,
        renderMatrix,
        center,
        camera,
        dimensions.width,
        dimensions.height,
        effectiveYScaling
      );
      if (closest && closest.distance < 26) {
        const mapped = mapRenderToOrigCoord(closest.r, closest.c);
        setHoveredCell((prev) =>
          prev && prev.r === mapped.r && prev.c === mapped.c ? prev : mapped
        );
      } else {
        setHoveredCell((prev) => (prev === null ? null : null));
      }
    });
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isDraggingRef.current) return;
    isDraggingRef.current = false;
    try {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      // Ignore if pointer capture already released.
    }

    if (!hasMovedRef.current && onSelectCell) {
      const pt = readCachedCanvasCoords(e.clientX, e.clientY);
      if (!pt) return;
      const closest = findClosestCell(
        pt.mouseX,
        pt.mouseY,
        renderMatrix,
        center,
        camera,
        dimensions.width,
        dimensions.height,
        effectiveYScaling
      );
      if (closest && closest.distance < 32) {
        const mapped = mapRenderToOrigCoord(closest.r, closest.c);
        onSelectCell(mapped.r, mapped.c);
      }
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (readOnly || !onChangeMatrix || !selectedRange) return;
    const { minR, maxR, minC, maxC } = selectedRange;
    const step = decimals > 0 ? 0.1 : 1;

    switch (e.key) {
      case '+':
      case '=':
        e.preventDefault();
        onChangeMatrix(adjustSelection(matrix, minR, maxR, minC, maxC, step, decimals));
        break;
      case '-':
      case '_':
        e.preventDefault();
        onChangeMatrix(adjustSelection(matrix, minR, maxR, minC, maxC, -step, decimals));
        break;
      case '%':
        e.preventDefault();
        onChangeMatrix(scaleSelection(matrix, minR, maxR, minC, maxC, 5, decimals));
        break;
      case 'ArrowUp':
        e.preventDefault();
        onSelectCell?.(Math.max(0, minR - 1), minC);
        break;
      case 'ArrowDown':
        e.preventDefault();
        onSelectCell?.(Math.min(origRows - 1, minR + 1), minC);
        break;
      case 'ArrowLeft':
        e.preventDefault();
        onSelectCell?.(minR, Math.max(0, minC - 1));
        break;
      case 'ArrowRight':
        e.preventDefault();
        onSelectCell?.(minR, Math.min(origCols - 1, minC + 1));
        break;
    }
  };

  const activeCellCoord = useMemo(() => {
    if (hoveredCell) return hoveredCell;
    if (selectedRange) return { r: selectedRange.minR, c: selectedRange.minC };
    return null;
  }, [hoveredCell, selectedRange]);

  const activeCellValue = activeCellCoord
    ? matrix[activeCellCoord.r]?.[activeCellCoord.c]
    : null;
  const activeBaseValue =
    activeCellCoord && baseMatrix ? baseMatrix[activeCellCoord.r]?.[activeCellCoord.c] : null;
  const activeDelta =
    activeCellValue !== null &&
    activeCellValue !== undefined &&
    activeBaseValue !== null &&
    activeBaseValue !== undefined
      ? activeCellValue - activeBaseValue
      : null;

  const colorScaleGradient = useMemo(() => {
    const stops = [0, 0.25, 0.5, 0.75, 1].map((frac) => {
      const val = minVal + frac * Math.max(maxVal - minVal, 1e-6);
      return `${getPaletteColor(val, minVal, maxVal, activePalette, 1)} ${Math.round(frac * 100)}%`;
    });
    return `linear-gradient(90deg, ${stops.join(', ')})`;
  }, [minVal, maxVal, activePalette]);

  return (
    <div
      tabIndex={0}
      onKeyDown={handleKeyDown}
      className="relative flex h-full w-full min-h-0 min-w-0 flex-col overflow-hidden rounded-md border border-kiviBorder/60 bg-kiviBg select-none focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent"
    >
      {/* Slim Borderless 1-Row 3D Header */}
      <div
        data-testid="panel-titlebar-surface3d"
        className="z-10 flex h-9 shrink-0 items-center justify-between gap-2 bg-kiviSurface px-3"
      >
        <div className="flex min-w-0 items-center gap-1.5">
          <Box className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          <span className="truncate text-xs font-semibold text-kiviInk">
            {title || '3D Surface Mesh'}
          </span>
          <span className="shrink-0 whitespace-nowrap rounded bg-kiviElevated px-1.5 py-0.5 font-mono text-[10px] text-kiviMuted tabular-nums">
            {origRows}&times;{origCols}
          </span>
          {normalized.is1DRibbon && (
            <span className="shrink-0 whitespace-nowrap rounded bg-kiviAccent/15 px-1.5 py-0.5 font-mono text-[10px] text-kiviAccent">
              1D Ribbon
            </span>
          )}
        </div>
        {headerActions && <div className="flex shrink-0 items-center gap-1">{headerActions}</div>}
      </div>

      {/* Dedicated Fixed-Bounds Canvas Host with Craftlab + Map Overlay HUD */}
      <div
        ref={canvasHostRef}
        data-testid="surface3d-canvas-host"
        className="relative min-h-[130px] min-w-0 flex-1 overflow-hidden"
      >
        <canvas
          ref={canvasRef}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerLeave={() => setHoveredCell(null)}
          onContextMenu={(e) => e.preventDefault()}
          className="absolute inset-0 block h-full w-full cursor-grab active:cursor-grabbing touch-none"
        />

        {/* Ultra-Compact Top-Left Cell Readout Pill */}
        {activeCellCoord && activeCellValue !== null && activeCellValue !== undefined && (
          <div
            data-testid="surface3d-cell-pill"
            className="kivi-panel-surface pointer-events-none absolute top-2 left-2 z-10 flex max-w-[calc(100%-84px)] flex-wrap items-center gap-x-2 gap-y-0.5 rounded-md bg-kiviSurface/90 px-2 py-1 font-mono text-[11px] backdrop-blur-sm tabular-nums"
          >
            <span className="text-[10px] text-kiviMuted">
              [{activeCellCoord.r},{activeCellCoord.c}]
            </span>
            <span className="font-semibold text-kiviAccent">
              {typeof activeCellValue === 'number'
                ? activeCellValue.toFixed(decimals)
                : activeCellValue}
              {unit ? <span className="ml-1 text-[10px] font-normal text-kiviSecondary">{unit}</span> : null}
            </span>
            <span className="text-[10px] text-kiviSecondary">
              {xLabels?.[activeCellCoord.c] ?? activeCellCoord.c} &middot;{' '}
              {yLabels?.[activeCellCoord.r] ?? activeCellCoord.r}
            </span>
            {activeDelta !== null && Math.abs(activeDelta) > 1e-6 && (
              <span
                className={`text-[10px] font-semibold ${
                  activeDelta > 0 ? 'text-kiviGreen' : 'text-kiviRed'
                }`}
              >
                {activeDelta > 0
                  ? `+${activeDelta.toFixed(decimals)}`
                  : activeDelta.toFixed(decimals)}
              </span>
            )}
          </div>
        )}

        {/* Top-Right Craftlab Click-to-Expand 3D Controls */}
        <div
          data-testid="surface3d-overlay-controls"
          className="absolute top-2 right-2 z-20 flex items-center gap-1"
        >
          <ViewportIconMenu<CameraPresetKey>
            testId="surface3d-camera-menu"
            ariaLabel="3D Camera View"
            headerLabel="Camera Angle"
            icon={<Compass className="h-3.5 w-3.5" />}
            value={activePreset}
            onSelect={applyPreset}
            extraAction={{ label: 'Reset View', onClick: handleResetCamera }}
            items={[
              { value: 'ISOMETRIC', label: 'Isometric', badge: 'Iso' },
              { value: 'TOP_DOWN', label: 'Top Plan', badge: 'Top' },
              { value: 'FRONT_RPM', label: 'Front Profile', badge: 'X' },
              { value: 'SIDE_TPS', label: 'Side Profile', badge: 'Y' },
            ]}
          />

          <ViewportIconPopover
            testId="surface3d-layers-popover"
            ariaLabel="3D Display Options"
            headerLabel="3D Surface Options"
            icon={<Layers className="h-3.5 w-3.5" />}
            active={renderMode !== 'shaded' || !showCage || activePalette !== initialPalette || Math.abs(elevationMultiplier - 1.0) > 0.05}
            popupWidth={238}
          >
            <div className="space-y-2.5 text-[11px]">
              <div>
                <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
                  Surface Mode
                </div>
                <div className="grid grid-cols-3 gap-1 rounded-md bg-kiviBg p-0.5">
                  {(
                    [
                      { id: 'shaded', label: 'Mesh' },
                      { id: 'smooth', label: 'Solid' },
                      { id: 'wireframe', label: 'Wire' },
                    ] as const
                  ).map((mode) => (
                    <button
                      key={mode.id}
                      type="button"
                      onClick={() => setRenderMode(mode.id)}
                      className={`rounded py-1 font-medium transition-colors ${
                        renderMode === mode.id
                          ? 'bg-kiviAccent text-kiviAccentFg'
                          : 'text-kiviSecondary hover:text-kiviInk'
                      }`}
                    >
                      {mode.label}
                    </button>
                  ))}
                </div>
              </div>

              <div>
                <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
                  Overlays
                </div>
                <div className="flex flex-wrap gap-1">
                  <button
                    type="button"
                    aria-pressed={showCage}
                    onClick={() => setShowCage((v) => !v)}
                    className={`rounded px-2 py-1 text-[11px] font-medium transition-colors ${
                      showCage
                        ? 'bg-kiviAccent/15 text-kiviAccent'
                        : 'bg-kiviBg text-kiviSecondary hover:text-kiviInk'
                    }`}
                  >
                    Cage &amp; Axes
                  </button>
                  {renderBaseMatrix && (
                    <button
                      type="button"
                      aria-pressed={showGhost}
                      onClick={() => setShowGhost((v) => !v)}
                      className={`rounded px-2 py-1 text-[11px] font-medium transition-colors ${
                        showGhost
                          ? 'bg-kiviWarn/15 text-kiviWarn'
                          : 'bg-kiviBg text-kiviSecondary hover:text-kiviInk'
                      }`}
                    >
                      Base Ghost
                    </button>
                  )}
                  <button
                    type="button"
                    aria-pressed={showColorScale}
                    onClick={() => setShowColorScale((v) => !v)}
                    className={`rounded px-2 py-1 text-[11px] font-medium transition-colors ${
                      showColorScale
                        ? 'bg-kiviAccent/15 text-kiviAccent'
                        : 'bg-kiviBg text-kiviSecondary hover:text-kiviInk'
                    }`}
                  >
                    Color Scale
                  </button>
                </div>
              </div>

              <div>
                <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
                  Color Palette
                </div>
                <div className="grid grid-cols-2 gap-1">
                  {(
                    [
                      { id: 'classic', label: 'Classic' },
                      { id: 'thermal', label: 'Thermal' },
                      { id: 'spectrum', label: 'Spectrum' },
                      { id: 'monochrome', label: 'Mono' },
                    ] as const
                  ).map((p) => (
                    <button
                      key={p.id}
                      type="button"
                      onClick={() => setActivePalette(p.id)}
                      className={`rounded px-2 py-1 text-left font-medium transition-colors ${
                        activePalette === p.id
                          ? 'bg-kiviAccent/15 text-kiviAccent'
                          : 'bg-kiviBg text-kiviSecondary hover:text-kiviInk'
                      }`}
                    >
                      {p.label}
                    </button>
                  ))}
                </div>
              </div>

              <div>
                <div className="mb-1 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
                  <span className="flex items-center gap-1">
                    <Sliders className="h-3 w-3 text-kiviAccent" />
                    <label htmlFor="surface3d-elevation">Z-Scale</label>
                  </span>
                  <span className="font-mono text-kiviAccent tabular-nums">
                    {elevationMultiplier.toFixed(1)}&times;
                  </span>
                </div>
                <input
                  id="surface3d-elevation"
                  type="range"
                  min="0.2"
                  max="3.0"
                  step="0.1"
                  value={elevationMultiplier}
                  onChange={(e) => setElevationMultiplier(parseFloat(e.target.value))}
                  className="h-1.5 w-full cursor-pointer appearance-none rounded-lg bg-kiviBorderStrong accent-kiviAccent"
                />
              </div>
            </div>
          </ViewportIconPopover>
        </div>

        {/* Bottom-Left Sleek Horizontal Color Scale Pill */}
        {showColorScale && (
          <div
            data-testid="surface3d-color-scale"
            className="kivi-panel-surface pointer-events-none absolute bottom-2.5 left-2.5 z-10 flex items-center gap-2 rounded-md bg-kiviSurface/88 px-2 py-1 font-mono text-[10px] text-kiviSecondary backdrop-blur-sm tabular-nums"
          >
            <span>{formatCompactTick(extremes.min, decimals)}</span>
            <div
              className="h-1.5 w-20 rounded-full border border-kiviBorder/60"
              style={{ background: colorScaleGradient }}
            />
            <span>
              {formatCompactTick(extremes.max, decimals)}
              {unit ? ` ${unit}` : ''}
            </span>
          </div>
        )}

        {/* Bottom-Right Map-Style Vertical Zoom & Reset Capsule */}
        <div
          data-testid="surface3d-zoom-capsule"
          className="kivi-panel-surface absolute right-2.5 bottom-2.5 z-20 flex flex-col overflow-hidden rounded-lg border border-kiviBorder/70 bg-kiviSurface/92 backdrop-blur-sm"
        >
          <button
            type="button"
            aria-label="Zoom In"
            title="Zoom In"
            onClick={() => handleZoom(1.15)}
            className="flex h-7 w-7 items-center justify-center text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
          >
            <ZoomIn className="h-3.5 w-3.5" />
          </button>
          <div className="h-px bg-kiviBorder/60" />
          <button
            type="button"
            aria-label="Zoom Out"
            title="Zoom Out"
            onClick={() => handleZoom(0.85)}
            className="flex h-7 w-7 items-center justify-center text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
          >
            <ZoomOut className="h-3.5 w-3.5" />
          </button>
          <div className="h-px bg-kiviBorder/60" />
          <button
            type="button"
            aria-label="Reset Camera & Zoom"
            title="Reset Camera & Zoom"
            onClick={handleResetCamera}
            className="flex h-7 w-7 items-center justify-center text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
});
