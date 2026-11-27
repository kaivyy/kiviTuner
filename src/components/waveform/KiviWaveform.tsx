import React, { useRef, useEffect, useState, useCallback, useMemo } from 'react';
import {
  ZoomIn,
  ZoomOut,
  Maximize2,
  Move,
  MousePointer,
} from 'lucide-react';

import type { DiffRegion, DetectedMapCandidate } from '../../types/kivi';
import {
  bridgeGetWaveformBuckets,
  bridgeReadRomWindow,
  type WaveformBuckets,
} from '../../lib/bridge/windowBridge';
import { useThemeStore } from '../../stores/useThemeStore';

export interface KiviWaveformProps {
  data: Uint8Array | null;
  romSize?: number;
  romRevision?: number;
  activeDocId?: string | null;
  pitch?: number;
  is16Bit?: boolean;
  isBigEndian?: boolean;
  onSelectRange?: (startByte: number, endByte: number) => void;
  className?: string;
  height?: number;
  onPitchChange?: (pitch: number) => void;
  diffRegions?: DiffRegion[];
  detectedCandidates?: DetectedMapCandidate[];
  onSelectRegion?: (startAddr: number, endAddr: number) => void;
}

export const KiviWaveform: React.FC<KiviWaveformProps> = ({
  data,
  romSize = 0,
  romRevision = 0,
  activeDocId = null,
  pitch: propPitch = 16,
  is16Bit: propIs16Bit = false,
  isBigEndian: propIsBigEndian = false,
  onSelectRange,
  className = '',
  height = 240,
  onPitchChange,
  diffRegions,
  detectedCandidates,
  onSelectRegion,
}) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const canvasRectRef = useRef<{ left: number; top: number; width: number; height: number } | null>(
    null
  );
  const theme = useThemeStore((s) => s.theme);

  // Controlled/Uncontrolled internal state
  const [pitch, setPitch] = useState<number>(propPitch);
  const [is16Bit, setIs16Bit] = useState<boolean>(propIs16Bit);
  const [isBigEndian, setIsBigEndian] = useState<boolean>(propIsBigEndian);
  const [snapToPitch, setSnapToPitch] = useState<boolean>(true);
  const [interactionMode, setInteractionMode] = useState<'select' | 'pan'>('select');

  // Sync prop changes
  useEffect(() => {
    if (propPitch !== undefined) setPitch(propPitch);
  }, [propPitch]);

  useEffect(() => {
    if (propIs16Bit !== undefined) setIs16Bit(propIs16Bit);
  }, [propIs16Bit]);

  useEffect(() => {
    if (propIsBigEndian !== undefined) setIsBigEndian(propIsBigEndian);
  }, [propIsBigEndian]);

  const updatePitch = useCallback(
    (newPitch: number) => {
      const clamped = Math.max(1, Math.min(128, newPitch));
      setPitch(clamped);
      if (onPitchChange) onPitchChange(clamped);
    },
    [onPitchChange]
  );

  // Layout dimensions from ResizeObserver
  const [canvasDimensions, setCanvasDimensions] = useState<{ width: number; height: number }>({
    width: 800,
    height,
  });

  // Calculate total sample count from either browser buffer or Rust single-owner size
  const byteLen = data && data.length > 0 ? data.length : romSize;
  const stepBytes = is16Bit ? 2 : 1;
  const totalSamples = useMemo(() => {
    return Math.floor(byteLen / stepBytes);
  }, [byteLen, stepBytes]);

  const [windowBuckets, setWindowBuckets] = useState<WaveformBuckets | null>(null);
  const [zoomSlice, setZoomSlice] = useState<{ start: number; bytes: Uint8Array } | null>(null);

  // Viewport navigation state
  const [zoom, setZoom] = useState<number>(() => Math.min(1024, Math.max(16, totalSamples || 1024)));
  const [viewOffset, setViewOffset] = useState<number>(0);

  // Selection state (sample indices)
  const [selectedRange, setSelectedRange] = useState<{ start: number; end: number } | null>(null);

  // Mouse interaction state
  const [dragState, setDragState] = useState<{
    isDragging: boolean;
    mode: 'select' | 'pan';
    startX: number;
    currentX: number;
    initialOffset: number;
  } | null>(null);

  // Hover coordinate readout
  const [hoverInfo, setHoverInfo] = useState<{
    sampleIdx: number;
    byteOffset: number;
    rawValue: number;
    x: number;
    y: number;
  } | null>(null);

  // ResizeObserver to track container dimensions and cache canvas rect for zero-reflow pointer moves
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const refreshRect = () => {
      const cr = canvasRef.current?.getBoundingClientRect();
      if (cr) {
        canvasRectRef.current = {
          left: cr.left,
          top: cr.top,
          width: cr.width,
          height: cr.height,
        };
      }
    };

    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height: h } = entry.contentRect;
        if (width > 0 && h > 0) {
          setCanvasDimensions({ width: Math.floor(width), height: Math.floor(h) });
        }
      }
      refreshRect();
    });

    ro.observe(container);
    refreshRect();
    window.addEventListener('resize', refreshRect, { passive: true });
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', refreshRect);
    };
  }, []);

  // Fetch decimated buckets (and fine window when zoomed in on single-owner desktop ROM)
  useEffect(() => {
    if (byteLen === 0) {
      setWindowBuckets(null);
      setZoomSlice(null);
      return;
    }
    const startByte = Math.max(0, Math.min(byteLen, Math.floor(viewOffset * stepBytes)));
    const endByte = Math.max(
      startByte,
      Math.min(byteLen, Math.ceil((viewOffset + zoom) * stepBytes))
    );
    const width = Math.max(64, canvasDimensions.width);
    const samplesPerPixel = Math.max(1, zoom) / width;

    let cancelled = false;
    void bridgeGetWaveformBuckets(
      startByte,
      endByte,
      Math.min(2048, width),
      data
    ).then((res) => {
      if (!cancelled && res) setWindowBuckets(res);
    });

    if ((!data || data.length === 0) && samplesPerPixel <= 4 && endByte > startByte) {
      const sliceLen = Math.min(16384, endByte - startByte);
      void bridgeReadRomWindow(startByte, sliceLen, null).then((bytes) => {
        if (!cancelled) setZoomSlice({ start: startByte, bytes });
      });
    } else if (data && data.length > 0) {
      setZoomSlice(null);
    }

    return () => {
      cancelled = true;
    };
  }, [byteLen, romRevision, activeDocId, viewOffset, zoom, stepBytes, canvasDimensions.width, data]);

  // Update zoom if totalSamples changes drastically
  useEffect(() => {
    if (totalSamples > 0 && zoom > totalSamples) {
      setZoom(Math.max(16, Math.min(1024, totalSamples)));
      setViewOffset(0);
    }
  }, [totalSamples, zoom]);

  // Read single sample value from local buffer or windowed slice
  const getSample = useCallback(
    (idx: number): number => {
      if (idx < 0 || idx >= totalSamples) return 0;
      const bIdx = idx * stepBytes;
      if (data && data.length > 0) {
        if (is16Bit) {
          if (bIdx + 1 >= data.length) return 0;
          return isBigEndian
            ? (data[bIdx] << 8) | data[bIdx + 1]
            : data[bIdx] | (data[bIdx + 1] << 8);
        }
        return data[bIdx] ?? 0;
      }
      if (zoomSlice) {
        const rel = bIdx - zoomSlice.start;
        if (is16Bit) {
          if (rel >= 0 && rel + 1 < zoomSlice.bytes.length) {
            return isBigEndian
              ? (zoomSlice.bytes[rel] << 8) | zoomSlice.bytes[rel + 1]
              : zoomSlice.bytes[rel] | (zoomSlice.bytes[rel + 1] << 8);
          }
        } else if (rel >= 0 && rel < zoomSlice.bytes.length) {
          return zoomSlice.bytes[rel];
        }
      }
      if (windowBuckets && windowBuckets.buckets.length > 0) {
        const spanBytes = Math.max(1, windowBuckets.endOffset - windowBuckets.startOffset);
        const rel = Math.max(0, Math.min(1, (bIdx - windowBuckets.startOffset) / spanBytes));
        const bPos = Math.min(
          windowBuckets.buckets.length - 1,
          Math.floor(rel * windowBuckets.buckets.length)
        );
        const avg = windowBuckets.buckets[bPos].avg;
        return is16Bit ? avg * 257 : avg;
      }
      return 0;
    },
    [data, zoomSlice, windowBuckets, is16Bit, isBigEndian, totalSamples, stepBytes]
  );

  // Event-driven canvas drawing routine (no idle 60 FPS polling loop)
  const renderFrame = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d', { alpha: false });
    if (!ctx) return;

    const { width, height: h } = canvasDimensions;
    const dpr = window.devicePixelRatio || 1;
    const isLight = theme === 'light';
    const bgHex = isLight ? '#F8FAFC' : '#0A0D12';
    const gridHex = isLight ? '#E2E8F0' : '#1E293B';
    const majorGridHex = isLight ? '#CBD5E1' : '#334155';
    const labelHex = isLight ? '#475569' : '#94A3B8';
    const waveHex = isLight ? '#0369A1' : '#38BDF8';

    // Ensure backing store matches DPR
    const targetW = Math.max(1, Math.floor(width * dpr));
    const targetH = Math.max(1, Math.floor(h * dpr));
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }

    ctx.save();
    ctx.scale(dpr, dpr);

    ctx.fillStyle = bgHex;
    ctx.fillRect(0, 0, width, h);

    const topMargin = 22;
    const bottomMargin = 26;
    const plotHeight = Math.max(10, h - topMargin - bottomMargin);
    const maxVal = is16Bit ? 65535 : 255;

    // Amplitude horizontal guide lines (0%, 25%, 50%, 75%, 100%)
    ctx.strokeStyle = gridHex;
    ctx.lineWidth = 1;
    ctx.font = '10px monospace';
    ctx.fillStyle = labelHex;
    ctx.textBaseline = 'middle';

    const gridSteps = [0, 0.25, 0.5, 0.75, 1.0];
    for (const ratio of gridSteps) {
      const y = topMargin + plotHeight - ratio * plotHeight;
      ctx.beginPath();
      ctx.moveTo(36, y);
      ctx.lineTo(width, y);
      ctx.stroke();

      // Label
      const valLabel = Math.round(ratio * maxVal);
      ctx.fillText(is16Bit ? `0x${valLabel.toString(16).toUpperCase().padStart(4, '0')}` : `${valLabel}`, 2, y);
    }

    if (totalSamples === 0) {
      ctx.fillStyle = labelHex;
      ctx.font = '12px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('No binary ROM data loaded in viewport', width / 2, h / 2);
      ctx.restore();
      return;
    }

    const effectiveZoom = Math.max(1, zoom);
    const step = effectiveZoom / width;

    // 1. Column Pitch Guides (strobe vertical grid)
    if (pitch > 0) {
      const pixelsPerPitch = (pitch / effectiveZoom) * width;
      // Stride pitch lines if too dense
      let stride = 1;
      if (pixelsPerPitch < 4) stride = 16;
      else if (pixelsPerPitch < 12) stride = 8;
      else if (pixelsPerPitch < 24) stride = 4;

      const firstK = Math.floor(viewOffset / pitch);
      const lastK = Math.ceil((viewOffset + effectiveZoom) / pitch);

      ctx.save();
      for (let k = firstK; k <= lastK; k++) {
        if (k % stride !== 0) continue;
        const sampleIdx = k * pitch;
        const x = ((sampleIdx - viewOffset) / effectiveZoom) * width;
        if (x < 0 || x > width) continue;

        const isMajor = k % (stride * 4) === 0;
        ctx.strokeStyle = isMajor ? majorGridHex : gridHex;
        ctx.lineWidth = isMajor ? 1.2 : 0.8;
        ctx.setLineDash(isMajor ? [] : [3, 3]);

        ctx.beginPath();
        ctx.moveTo(x, topMargin);
        ctx.lineTo(x, topMargin + plotHeight);
        ctx.stroke();

        // Top pitch index marker
        if (pixelsPerPitch >= 28 && isMajor) {
          ctx.fillStyle = labelHex;
          ctx.font = '9px monospace';
          ctx.textAlign = 'center';
          ctx.fillText(`R${k}`, x, 12);
        }
      }
      ctx.restore();
    }

    // 2. Waveform Signal Rendering (LoD Virtualization via Rust buckets or browser slice)
    const samplesPerPixel = effectiveZoom / width;

    if (samplesPerPixel > 2) {
      if (windowBuckets && windowBuckets.buckets.length > 0 && (!data || data.length === 0 || windowBuckets.buckets.length >= width * 0.5)) {
        const bCount = windowBuckets.buckets.length;
        ctx.fillStyle = waveHex;
        for (let x = 0; x < width; x++) {
          const bIdx = Math.min(bCount - 1, Math.floor((x / width) * bCount));
          const bucket = windowBuckets.buckets[bIdx];
          const minY = topMargin + plotHeight - (bucket.max / 255) * plotHeight;
          const maxY = topMargin + plotHeight - (bucket.min / 255) * plotHeight;
          const barHeight = Math.max(1.2, maxY - minY);
          ctx.fillRect(x, minY, 1, barHeight);
        }
      } else {
        for (let x = 0; x < width; x++) {
          const sStart = Math.floor(viewOffset + x * samplesPerPixel);
          const sEnd = Math.min(totalSamples, Math.floor(viewOffset + (x + 1) * samplesPerPixel));
          if (sStart >= totalSamples) break;

          let bucketMin = getSample(sStart);
          let bucketMax = bucketMin;

          for (let s = sStart + 1; s < sEnd; s++) {
            const val = getSample(s);
            if (val < bucketMin) bucketMin = val;
            if (val > bucketMax) bucketMax = val;
          }

          const minY = topMargin + plotHeight - (bucketMax / maxVal) * plotHeight;
          const maxY = topMargin + plotHeight - (bucketMin / maxVal) * plotHeight;
          const barHeight = Math.max(1.2, maxY - minY);

          ctx.fillStyle = waveHex;
          ctx.fillRect(x, minY, 1, barHeight);
        }
      }
    } else {
      // Continuous trace line when zoomed in
      ctx.beginPath();
      ctx.strokeStyle = waveHex;
      ctx.lineWidth = 1.4;
      ctx.lineJoin = 'round';

      let started = false;
      for (let x = 0; x < width; x++) {
        const sIdx = Math.floor(viewOffset + x * step);
        if (sIdx >= totalSamples) break;

        const val = getSample(sIdx);
        const y = topMargin + plotHeight - (val / maxVal) * plotHeight;

        if (!started) {
          ctx.moveTo(x, y);
          started = true;
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.stroke();

      // Individual discrete sample points when very zoomed in
      if (samplesPerPixel < 0.3) {
        ctx.fillStyle = '#7dd3fc';
        for (let x = 0; x < width; x++) {
          const sIdx = Math.floor(viewOffset + x * step);
          if (sIdx >= totalSamples) break;

          // Check if this pixel aligns with sample start
          const sampleScreenX = ((sIdx - viewOffset) / effectiveZoom) * width;
          if (Math.abs(sampleScreenX - x) < 0.5) {
            const val = getSample(sIdx);
            const y = topMargin + plotHeight - (val / maxVal) * plotHeight;
            ctx.beginPath();
            ctx.arc(sampleScreenX, y, 2.5, 0, Math.PI * 2);
            ctx.fill();
          }
        }
      }
    }

    // 2.5 Detected Map Candidates Overlay (amber instrument highlight)
    if (detectedCandidates && detectedCandidates.length > 0) {
      ctx.save();
      for (const candidate of detectedCandidates) {
        const sStart = Math.floor(candidate.start_addr / stepBytes);
        const sEnd = Math.floor(candidate.end_addr / stepBytes);
        if (sEnd < viewOffset || sStart > viewOffset + effectiveZoom) continue;

        const x1 = ((sStart - viewOffset) / effectiveZoom) * width;
        const x2 = ((sEnd - viewOffset) / effectiveZoom) * width;
        const boxX = Math.max(0, Math.min(x1, x2));
        const boxW = Math.max(2, Math.min(width - boxX, Math.abs(x2 - x1)));

        ctx.fillStyle = 'rgba(245, 158, 11, 0.20)';
        ctx.fillRect(boxX, topMargin, boxW, plotHeight);
        ctx.strokeStyle = 'rgba(245, 158, 11, 0.75)';
        ctx.lineWidth = 1.2;
        ctx.strokeRect(boxX, topMargin, boxW, plotHeight);

        // Label tag
        const tagText = `${candidate.cols}x${candidate.rows} (${candidate.confidence}%)`;
        ctx.font = '9px monospace';
        const tagW = ctx.measureText(tagText).width + 8;
        if (boxW >= 20) {
          const tagX = Math.max(2, Math.min(width - tagW - 2, boxX + 2));
          ctx.fillStyle = isLight ? 'rgba(255, 255, 255, 0.92)' : 'rgba(17, 22, 31, 0.92)';
          ctx.fillRect(tagX, topMargin + 2, tagW, 14);
          ctx.strokeStyle = isLight ? 'rgba(180, 83, 9, 0.7)' : 'rgba(251, 191, 36, 0.65)';
          ctx.lineWidth = 1;
          ctx.strokeRect(tagX, topMargin + 2, tagW, 14);
          ctx.fillStyle = isLight ? '#92400E' : '#FBBF24';
          ctx.textAlign = 'left';
          ctx.fillText(tagText, tagX + 4, topMargin + 12);
        }
      }
      ctx.restore();
    }

    // 2.6 Binary Diff Regions Overlay (red/orange)
    if (diffRegions && diffRegions.length > 0) {
      ctx.save();
      for (const region of diffRegions) {
        const sStart = Math.floor(region.start_addr / stepBytes);
        const sEnd = Math.floor(region.end_addr / stepBytes);
        if (sEnd < viewOffset || sStart > viewOffset + effectiveZoom) continue;

        const x1 = ((sStart - viewOffset) / effectiveZoom) * width;
        const x2 = ((sEnd - viewOffset) / effectiveZoom) * width;
        const boxX = Math.max(0, Math.min(x1, x2));
        const boxW = Math.max(2, Math.min(width - boxX, Math.abs(x2 - x1)));

        // Transparent red/orange fill & border
        ctx.fillStyle = 'rgba(239, 68, 68, 0.25)';
        ctx.fillRect(boxX, topMargin, boxW, plotHeight);
        ctx.strokeStyle = 'rgba(239, 68, 68, 0.75)';
        ctx.lineWidth = 1.2;
        ctx.strokeRect(boxX, topMargin, boxW, plotHeight);

        // Subtle label tag
        const tagText = `Diff: ${region.byte_len}B (Δ${region.max_abs_delta})`;
        ctx.font = '9px monospace';
        const tagW = ctx.measureText(tagText).width + 8;
        if (boxW >= 20) {
          const tagX = Math.max(2, Math.min(width - tagW - 2, boxX + 2));
          ctx.fillStyle = 'rgba(38, 15, 15, 0.85)';
          ctx.fillRect(tagX, topMargin + 18, tagW, 14);
          ctx.strokeStyle = 'rgba(239, 68, 68, 0.7)';
          ctx.lineWidth = 1;
          ctx.strokeRect(tagX, topMargin + 18, tagW, 14);
          ctx.fillStyle = '#f87171';
          ctx.textAlign = 'left';
          ctx.fillText(tagText, tagX + 4, topMargin + 28);
        }
      }
      ctx.restore();
    }

    // 3. Selection Range Box Overlay
    const activeSelection = (() => {
      if (dragState && dragState.mode === 'select') {
        const s1 = Math.floor(viewOffset + (dragState.startX / width) * effectiveZoom);
        const s2 = Math.floor(viewOffset + (dragState.currentX / width) * effectiveZoom);
        let start = Math.max(0, Math.min(s1, s2));
        let end = Math.min(totalSamples - 1, Math.max(s1, s2));

        if (snapToPitch && pitch > 1) {
          start = Math.floor(start / pitch) * pitch;
          end = Math.min(totalSamples - 1, Math.ceil((end + 1) / pitch) * pitch - 1);
        }
        return { start, end };
      }
      return selectedRange;
    })();

    if (activeSelection && activeSelection.start <= activeSelection.end) {
      const x1 = ((activeSelection.start - viewOffset) / effectiveZoom) * width;
      const x2 = (((activeSelection.end + 1) - viewOffset) / effectiveZoom) * width;

      const boxX = Math.max(0, Math.min(x1, x2));
      const boxW = Math.min(width - boxX, Math.abs(x2 - x1));

      if (boxW > 0 && boxX < width && boxX + boxW > 0) {
        // Selection fill
        ctx.fillStyle = 'rgba(56, 189, 248, 0.2)';
        ctx.fillRect(boxX, topMargin, boxW, plotHeight);

        // Selection borders
        ctx.strokeStyle = '#38bdf8';
        ctx.lineWidth = 1.5;
        ctx.strokeRect(boxX, topMargin, boxW, plotHeight);

        // Selection label badge
        const selSamples = activeSelection.end - activeSelection.start + 1;
        const selBytes = selSamples * stepBytes;
        const selRows = pitch > 0 ? (selSamples / pitch).toFixed(selSamples % pitch === 0 ? 0 : 1) : '-';

        const badgeText = `${selBytes} B (${selRows}×${pitch} cols) [0x${(activeSelection.start * stepBytes).toString(16).toUpperCase()}..0x${((activeSelection.end + 1) * stepBytes).toString(16).toUpperCase()}]`;

        ctx.font = '10px monospace';
        const textMetrics = ctx.measureText(badgeText);
        const badgeW = textMetrics.width + 12;
        const badgeX = Math.max(4, Math.min(width - badgeW - 4, boxX));

        ctx.fillStyle = 'rgba(15, 23, 42, 0.85)';
        ctx.fillRect(badgeX, topMargin + 4, badgeW, 16);
        ctx.strokeStyle = '#38bdf8';
        ctx.lineWidth = 1;
        ctx.strokeRect(badgeX, topMargin + 4, badgeW, 16);

        ctx.fillStyle = '#38bdf8';
        ctx.textAlign = 'left';
        ctx.fillText(badgeText, badgeX + 6, topMargin + 15);
      }
    }

    // 5. Bottom Address Timeline / Ruler
    ctx.strokeStyle = majorGridHex;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, topMargin + plotHeight);
    ctx.lineTo(width, topMargin + plotHeight);
    ctx.stroke();

    ctx.fillStyle = labelHex;
    ctx.font = '9px monospace';
    ctx.textAlign = 'left';

    const tickCount = 6;
    for (let i = 0; i <= tickCount; i++) {
      const tx = (i / tickCount) * width;
      const tSample = Math.floor(viewOffset + (i / tickCount) * effectiveZoom);
      const tByte = tSample * stepBytes;

      ctx.beginPath();
      ctx.moveTo(tx, topMargin + plotHeight);
      ctx.lineTo(tx, topMargin + plotHeight + 4);
      ctx.stroke();

      const addrStr = `0x${tByte.toString(16).toUpperCase().padStart(6, '0')}`;
      const labelX = Math.max(2, Math.min(width - 55, tx - 2));
      ctx.fillText(addrStr, labelX, h - 8);
    }

    ctx.restore();
  }, [
    canvasDimensions,
    data,
    dragState,
    getSample,
    is16Bit,
    pitch,
    selectedRange,
    snapToPitch,
    stepBytes,
    totalSamples,
    viewOffset,
    zoom,
    windowBuckets,
    zoomSlice,
    diffRegions,
    detectedCandidates,
    theme,
  ]);

  // Event-driven render on viewport/data/theme state transitions
  useEffect(() => {
    renderFrame();
  }, [renderFrame]);

  const getCachedRect = () => {
    const cached = canvasRectRef.current;
    if (cached && cached.width > 0) return cached;
    const measured = canvasRef.current?.getBoundingClientRect();
    if (!measured) return null;
    canvasRectRef.current = {
      left: measured.left,
      top: measured.top,
      width: measured.width,
      height: measured.height,
    };
    return canvasRectRef.current;
  };

  // Mouse Wheel: Zoom towards cursor position
  const handleWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    if (totalSamples === 0) return;

    const rect = getCachedRect();
    if (!rect) return;

    const mouseX = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const cursorRatio = mouseX / rect.width;
    const cursorSample = viewOffset + cursorRatio * zoom;

    // Zoom multiplier
    const factor = e.deltaY < 0 ? 0.8 : 1.25;
    const minZoom = Math.min(16, totalSamples);
    const maxZoom = Math.max(16, totalSamples);
    const newZoom = Math.round(Math.max(minZoom, Math.min(maxZoom, zoom * factor)));

    // Keep cursor sample steady
    const newOffset = Math.round(
      Math.max(0, Math.min(totalSamples - newZoom, cursorSample - cursorRatio * newZoom))
    );

    setZoom(newZoom);
    setViewOffset(newOffset);
  };

  // Mouse Down: Start selection or pan
  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const measured = canvasRef.current?.getBoundingClientRect();
    if (measured) {
      canvasRectRef.current = {
        left: measured.left,
        top: measured.top,
        width: measured.width,
        height: measured.height,
      };
    }
    const rect = getCachedRect();
    if (!rect || totalSamples === 0) return;

    const x = e.clientX - rect.left;
    // Determine mode: middle click or active pan mode activates pan
    const isPan = e.button === 1 || e.shiftKey || interactionMode === 'pan';

    setDragState({
      isDragging: true,
      mode: isPan ? 'pan' : 'select',
      startX: x,
      currentX: x,
      initialOffset: viewOffset,
    });
  };

  // Mouse Move: Update selection, pan, and hover DOM crosshair overlay
  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const rect = getCachedRect();
    if (!rect) return;

    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const y = Math.max(0, Math.min(rect.height, e.clientY - rect.top));

    // Update hover information
    if (totalSamples > 0) {
      const sampleIdx = Math.max(
        0,
        Math.min(totalSamples - 1, Math.floor(viewOffset + (x / rect.width) * zoom))
      );
      const byteOffset = sampleIdx * stepBytes;
      const rawValue = getSample(sampleIdx);
      setHoverInfo({ sampleIdx, byteOffset, rawValue, x, y });
    }

    if (!dragState || !dragState.isDragging) return;

    if (dragState.mode === 'pan') {
      const dx = x - dragState.startX;
      const sampleDelta = Math.round((dx / rect.width) * zoom);
      const newOffset = Math.max(
        0,
        Math.min(totalSamples - zoom, dragState.initialOffset - sampleDelta)
      );
      setViewOffset(newOffset);
    } else {
      setDragState((prev) => (prev ? { ...prev, currentX: x } : null));
    }
  };

  // Mouse Up: Finalize selection or pan
  const handleMouseUp = () => {
    if (!dragState) return;

    if (dragState.mode === 'select' && canvasRef.current) {
      const width = canvasRef.current.clientWidth || 800;
      const s1 = Math.floor(viewOffset + (dragState.startX / width) * zoom);
      const s2 = Math.floor(viewOffset + (dragState.currentX / width) * zoom);

      let start = Math.max(0, Math.min(s1, s2));
      let end = Math.min(totalSamples - 1, Math.max(s1, s2));

      // Click without drag clears selection or selects clicked overlay region
      if (Math.abs(dragState.currentX - dragState.startX) < 3) {
        const clickedByte = s1 * stepBytes;
        const hitDiff = diffRegions?.find(
          (r) => clickedByte >= r.start_addr && clickedByte < r.end_addr
        );
        if (hitDiff) {
          onSelectRegion?.(hitDiff.start_addr, hitDiff.end_addr);
          const startSample = Math.floor(hitDiff.start_addr / stepBytes);
          const endSample = Math.max(startSample, Math.floor(hitDiff.end_addr / stepBytes) - 1);
          setSelectedRange({ start: startSample, end: endSample });
          onSelectRange?.(hitDiff.start_addr, hitDiff.end_addr);
          setDragState(null);
          return;
        }

        const hitCandidate = detectedCandidates?.find(
          (c) => clickedByte >= c.start_addr && clickedByte < c.end_addr
        );
        if (hitCandidate) {
          onSelectRegion?.(hitCandidate.start_addr, hitCandidate.end_addr);
          const startSample = Math.floor(hitCandidate.start_addr / stepBytes);
          const endSample = Math.max(startSample, Math.floor(hitCandidate.end_addr / stepBytes) - 1);
          setSelectedRange({ start: startSample, end: endSample });
          onSelectRange?.(hitCandidate.start_addr, hitCandidate.end_addr);
          setDragState(null);
          return;
        }

        setSelectedRange(null);
      } else {
        if (snapToPitch && pitch > 1) {
          start = Math.floor(start / pitch) * pitch;
          end = Math.min(totalSamples - 1, Math.ceil((end + 1) / pitch) * pitch - 1);
        }

        setSelectedRange({ start, end });
        const startByte = start * stepBytes;
        const endByte = (end + 1) * stepBytes;
        if (onSelectRange) {
          onSelectRange(startByte, endByte);
        }
      }
    }

    setDragState(null);
  };

  const handleMouseLeave = () => {
    setHoverInfo(null);
    if (dragState) {
      handleMouseUp();
    }
  };

  // Zoom control helpers
  const handleZoomIn = () => {
    const minZoom = Math.min(16, totalSamples);
    const newZoom = Math.max(minZoom, Math.round(zoom * 0.7));
    const newOffset = Math.min(totalSamples - newZoom, viewOffset + Math.round((zoom - newZoom) / 2));
    setZoom(newZoom);
    setViewOffset(Math.max(0, newOffset));
  };

  const handleZoomOut = () => {
    const maxZoom = Math.max(16, totalSamples);
    const newZoom = Math.min(maxZoom, Math.round(zoom * 1.4));
    const newOffset = Math.max(0, Math.min(totalSamples - newZoom, viewOffset - Math.round((newZoom - zoom) / 2)));
    setZoom(newZoom);
    setViewOffset(newOffset);
  };

  const handleFitAll = () => {
    setZoom(Math.max(16, totalSamples));
    setViewOffset(0);
  };

  // Minimap track click/drag to jump viewport
  const handleMinimapClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (totalSamples <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const clickRatio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const targetCenter = Math.floor(clickRatio * totalSamples);
    const newOffset = Math.max(0, Math.min(totalSamples - zoom, targetCenter - Math.floor(zoom / 2)));
    setViewOffset(newOffset);
  };

  // Scoped keyboard navigation when waveform container is focused
  const handleContainerKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (['INPUT', 'SELECT', 'TEXTAREA'].includes((e.target as HTMLElement)?.tagName)) return;
    const noMod = !e.ctrlKey && !e.metaKey && !e.altKey;
    if (noMod && (e.key === '[' || e.key === '{')) {
      e.preventDefault();
      updatePitch(pitch - 1);
    } else if (noMod && (e.key === ']' || e.key === '}')) {
      e.preventDefault();
      updatePitch(pitch + 1);
    } else if (noMod && (e.key === 'b' || e.key === 'B')) {
      e.preventDefault();
      setIs16Bit((prev) => !prev);
    } else if (noMod && (e.key === 'e' || e.key === 'E')) {
      e.preventDefault();
      setIsBigEndian((prev) => !prev);
    } else if (e.key === 'Escape') {
      setSelectedRange(null);
    }
  };

  return (
    <div
      tabIndex={0}
      onKeyDown={handleContainerKeyDown}
      className={`flex flex-col rounded-lg border border-kiviBorder bg-kiviBg text-kiviInk select-none focus-visible:outline-2 focus-visible:outline-kiviAccent ${className}`}
    >
      {/* Waveform Controls Toolbar */}
      <div className="flex flex-wrap items-center justify-between border-b border-kiviBorder bg-kiviSurface px-3 py-1.5 text-xs gap-2">
        {/* Left Toolbar: Bit depth, Endianness & Pitch */}
        <div className="flex items-center gap-2 flex-wrap">
          {/* Bit depth toggle */}
          <div className="flex items-center rounded bg-kiviSurface p-0.5 border border-kiviBorder">
            <button
              type="button"
              onClick={() => setIs16Bit(false)}
              className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors ${
                !is16Bit
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              8-Bit
            </button>
            <button
              type="button"
              onClick={() => setIs16Bit(true)}
              className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors ${
                is16Bit
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              16-Bit
            </button>
          </div>

          {/* Endianness toggle (only active/visible in 16-bit mode) */}
          {is16Bit && (
            <div className="flex items-center rounded bg-kiviSurface p-0.5 border border-kiviBorder">
              <button
                type="button"
                onClick={() => setIsBigEndian(false)}
                className={`px-2 py-0.5 rounded text-[11px] font-mono transition-colors ${
                  !isBigEndian
                    ? 'bg-kiviAccent/15 text-kiviAccent font-semibold border border-kiviAccent/40'
                    : 'text-kiviMuted hover:text-kiviInk'
                }`}
                title="Little Endian (Intel / ARM / Renesas)"
              >
                LE
              </button>
              <button
                type="button"
                onClick={() => setIsBigEndian(true)}
                className={`px-2 py-0.5 rounded text-[11px] font-mono transition-colors ${
                  isBigEndian
                    ? 'bg-kiviAccent/15 text-kiviAccent font-semibold border border-kiviAccent/40'
                    : 'text-kiviMuted hover:text-kiviInk'
                }`}
                title="Big Endian (Motorola / PowerPC / MPC5xx)"
              >
                BE
              </button>
            </div>
          )}

          <div className="h-3.5 w-px bg-kiviBorder" />

          {/* Column Pitch Guide Controls */}
          <div className="flex items-center gap-1">
            <span className="text-[11px] text-kiviMuted">Pitch:</span>
            <div className="flex items-center rounded border border-kiviBorder bg-kiviSurface font-mono">
              <button
                type="button"
                onClick={() => updatePitch(pitch - 1)}
                className="px-1.5 py-0.5 text-kiviMuted hover:text-kiviInk hover:bg-kiviElevated transition-colors"
                title="Decrease pitch ([)"
              >
                -
              </button>
              <span className="px-1.5 py-0.5 text-[11px] text-kiviAccent min-w-[24px] text-center font-bold">
                {pitch}
              </span>
              <button
                type="button"
                onClick={() => updatePitch(pitch + 1)}
                className="px-1.5 py-0.5 text-kiviMuted hover:text-kiviInk hover:bg-kiviElevated transition-colors"
                title="Increase pitch (])"
              >
                +
              </button>
            </div>

            {/* Quick pitch presets */}
            <div className="flex items-center gap-0.5 text-[10px] font-mono text-kiviMuted">
              {[8, 12, 16, 20, 32].map((pVal) => (
                <button
                  key={pVal}
                  type="button"
                  onClick={() => updatePitch(pVal)}
                  className={`px-1 py-0.5 rounded hover:bg-kiviElevated transition-colors ${
                    pitch === pVal ? 'text-kiviAccent font-bold' : ''
                  }`}
                >
                  {pVal}
                </button>
              ))}
            </div>

            {/* Snap to Pitch toggle */}
            <label className="flex items-center gap-1 ml-1 cursor-pointer text-[10px] text-kiviMuted hover:text-kiviInk">
              <input
                type="checkbox"
                checked={snapToPitch}
                onChange={(e) => setSnapToPitch(e.target.checked)}
                className="rounded border-kiviBorder bg-kiviSurface text-kiviAccent focus:ring-0 h-3 w-3"
              />
              <span>Snap</span>
            </label>
          </div>
        </div>

        {/* Center: Selected range summary */}
        {selectedRange && (
          <div className="flex items-center gap-1.5 rounded bg-kiviAccent/10 border border-kiviAccent/40 px-2 py-0.5 text-[11px] font-mono text-kiviAccent tabular-nums">
            <MousePointer className="h-3 w-3 text-kiviAccent" />
            <span>
              0x{(selectedRange.start * stepBytes).toString(16).toUpperCase()}..0x
              {((selectedRange.end + 1) * stepBytes).toString(16).toUpperCase()} (
              {(selectedRange.end - selectedRange.start + 1) * stepBytes} B)
            </span>
            <button
              type="button"
              onClick={() => setSelectedRange(null)}
              className="ml-1 text-[10px] text-kiviMuted hover:text-kiviInk"
              title="Clear selection"
            >
              &times;
            </button>
          </div>
        )}

        {/* Right Toolbar: Tools, Zoom & Navigation */}
        <div className="flex items-center gap-1.5">
          {/* Interaction Mode Toggle */}
          <div className="flex items-center rounded bg-kiviSurface p-0.5 border border-kiviBorder">
            <button
              type="button"
              onClick={() => setInteractionMode('select')}
              className={`p-1 rounded transition-colors ${
                interactionMode === 'select'
                  ? 'bg-kiviAccent text-kiviAccentFg'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
              title="Select Range Tool (Drag to highlight bytes)"
            >
              <MousePointer className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={() => setInteractionMode('pan')}
              className={`p-1 rounded transition-colors ${
                interactionMode === 'pan'
                  ? 'bg-kiviAccent text-kiviAccentFg'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
              title="Pan Tool (Drag to navigate ROM)"
            >
              <Move className="h-3.5 w-3.5" />
            </button>
          </div>

          <div className="h-3.5 w-px bg-kiviBorder" />

          {/* Zoom Buttons */}
          <button
            type="button"
            onClick={handleZoomIn}
            className="p-1 rounded text-kiviSecondary hover:text-kiviInk hover:bg-kiviElevated transition-colors"
            title="Zoom In"
          >
            <ZoomIn className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={handleZoomOut}
            className="p-1 rounded text-kiviSecondary hover:text-kiviInk hover:bg-kiviElevated transition-colors"
            title="Zoom Out"
          >
            <ZoomOut className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={handleFitAll}
            className="p-1 rounded text-kiviSecondary hover:text-kiviInk hover:bg-kiviElevated transition-colors"
            title="Fit Entire File"
          >
            <Maximize2 className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Main Waveform Canvas Viewport */}
      <div
        ref={containerRef}
        className="relative w-full overflow-hidden bg-kiviBg"
        style={{ height: `${height}px` }}
      >
        <canvas
          ref={canvasRef}
          className={`w-full h-full block ${
            interactionMode === 'select' ? 'cursor-crosshair' : 'cursor-grab active:cursor-grabbing'
          }`}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseLeave}
          onWheel={handleWheel}
        />

        {/* DOM Hover Crosshair Overlay (prevents canvas redraw on mouse move) */}
        {hoverInfo && !dragState && (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute top-[22px] bottom-[26px] left-0 w-px border-l border-dashed border-kiviMuted/70"
            style={{ transform: `translateX(${hoverInfo.x}px)` }}
          />
        )}

        {/* Real-time Hover HUD in corner */}
        {hoverInfo && (() => {
          const matchingDiff = diffRegions?.find(
            (r) => hoverInfo.byteOffset >= r.start_addr && hoverInfo.byteOffset < r.end_addr
          );
          const matchingCandidate = detectedCandidates?.find(
            (c) => hoverInfo.byteOffset >= c.start_addr && hoverInfo.byteOffset < c.end_addr
          );

          return (
            <div className="pointer-events-none absolute bottom-8 right-2 flex min-w-[236px] items-center gap-2 whitespace-nowrap rounded border border-kiviBorderStrong bg-kiviSurface px-2.5 py-1 font-mono text-[10px] text-kiviSecondary shadow-lg tabular-nums">
              <span>
                Addr: <span className="text-kiviAccent">0x{hoverInfo.byteOffset.toString(16).toUpperCase().padStart(6, '0')}</span>
              </span>
              <span className="text-kiviBorderStrong">|</span>
              <span>
                Val:{' '}
                <span className="text-kiviInk">
                  0x{hoverInfo.rawValue.toString(16).toUpperCase().padStart(is16Bit ? 4 : 2, '0')}
                </span>{' '}
                ({hoverInfo.rawValue})
              </span>
              {matchingDiff && (
                <>
                  <span className="text-kiviBorderStrong">|</span>
                  <span className="text-kiviRed font-semibold">
                    Diff: {matchingDiff.byte_len}B (delta {matchingDiff.max_abs_delta})
                  </span>
                </>
              )}
              {matchingCandidate && (
                <>
                  <span className="text-kiviBorderStrong">|</span>
                  <span className="text-kiviWarn font-semibold">
                    Candidate: {matchingCandidate.name_hint} ({matchingCandidate.confidence}%)
                  </span>
                </>
              )}
            </div>
          );
        })()}
      </div>

      {/* Minimap ROM Navigation Scrub Bar */}
      <div
        onClick={handleMinimapClick}
        className="relative h-4 w-full bg-kiviSurface border-t border-kiviBorder cursor-pointer group flex items-center"
        title="ROM Minimap: Click to jump viewport"
      >
        {totalSamples > 0 && (
          <div
            className="absolute h-full bg-kiviAccent/25 border-x border-kiviAccent/60 transition-[left,width] duration-75"
            style={{
              left: `${(viewOffset / totalSamples) * 100}%`,
              width: `${Math.max(1, (zoom / totalSamples) * 100)}%`,
            }}
          />
        )}
        <div className="absolute left-2 text-[9px] font-mono text-kiviMuted pointer-events-none tabular-nums">
          ROM Extent: {byteLen} bytes ({totalSamples} samples)
        </div>
        <div className="absolute right-2 text-[9px] font-mono text-kiviMuted pointer-events-none tabular-nums">
          Viewing {viewOffset}..{Math.min(totalSamples, viewOffset + zoom)}
        </div>
      </div>
    </div>
  );
};

export default KiviWaveform;
