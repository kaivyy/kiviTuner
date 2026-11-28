/** Pure clamping and auto-collapse threshold calculations for resizable panels and split views. */

export interface PanelResizeInput {
  rawWidth: number;
  minWidth: number;
  maxWidth: number;
  collapseThreshold?: number;
  minPreviewWidth?: number;
}

export interface PanelResizeOutcome {
  width: number;
  willCollapse: boolean;
}

export function computePanelResize({
  rawWidth,
  minWidth,
  maxWidth,
  collapseThreshold,
  minPreviewWidth,
}: PanelResizeInput): PanelResizeOutcome {
  if (!Number.isFinite(rawWidth)) {
    return { width: minWidth, willCollapse: false };
  }
  const safeMax = Math.max(minWidth, maxWidth);
  if (collapseThreshold !== undefined && rawWidth < collapseThreshold) {
    const floorW = Math.max(96, Math.min(minPreviewWidth ?? collapseThreshold, minWidth));
    return {
      width: Math.max(floorW, Math.round(rawWidth)),
      willCollapse: true,
    };
  }
  return {
    width: Math.round(Math.max(minWidth, Math.min(safeMax, rawWidth))),
    willCollapse: false,
  };
}

export interface SplitRatioInput {
  pointerPos: number;
  containerStart: number;
  containerSize: number;
  minRatio?: number;
  maxRatio?: number;
}

export function computeSplitRatio({
  pointerPos,
  containerStart,
  containerSize,
  minRatio = 0.25,
  maxRatio = 0.75,
}: SplitRatioInput): number {
  if (!Number.isFinite(containerSize) || containerSize <= 0) {
    return 0.5;
  }
  if (!Number.isFinite(pointerPos) || !Number.isFinite(containerStart)) {
    return 0.5;
  }
  const lo = Math.max(0.1, Math.min(minRatio, maxRatio));
  const hi = Math.min(0.9, Math.max(minRatio, maxRatio));
  const raw = (pointerPos - containerStart) / containerSize;
  return Number(Math.max(lo, Math.min(hi, raw)).toFixed(4));
}
