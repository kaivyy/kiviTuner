/**
 * Multi-Palette Heatmap Color Ramp Library for ECU Calibration
 * Palettes:
 * - 'classic': Blue (#1e40af) -> Green (#16a34a) -> Red (#dc2626)
 * - 'thermal': Navy (#020617) -> Cyan (#06b6d4) -> Yellow (#eab308) -> Red (#ef4444)
 * - 'spectrum': Violet (#7c3aed) -> Blue (#2563eb) -> Emerald (#10b981) -> Amber (#f59e0b)
 * - 'monochrome': Dark Slate (#0f172a) -> Crisp White (#f8fafc)
 */

export interface RGB {
  r: number;
  g: number;
  b: number;
}

export type ColorPalette = 'classic' | 'thermal' | 'spectrum' | 'monochrome';

export const COLOR_STOPS = {
  MIN_BLUE: { r: 30, g: 64, b: 175 },   // #1e40af
  MID_GREEN: { r: 22, g: 163, b: 74 },  // #16a34a
  MAX_RED: { r: 220, g: 38, b: 38 },    // #dc2626
} as const;

interface StopDef {
  stop: number;
  r: number;
  g: number;
  b: number;
}

const PALETTE_STOPS: Record<ColorPalette, StopDef[]> = {
  classic: [
    { stop: 0.0, r: 30, g: 64, b: 175 },   // #1e40af
    { stop: 0.5, r: 22, g: 163, b: 74 },  // #16a34a
    { stop: 1.0, r: 220, g: 38, b: 38 },  // #dc2626
  ],
  thermal: [
    { stop: 0.0, r: 2, g: 6, b: 23 },      // Navy #020617
    { stop: 0.33, r: 6, g: 182, b: 212 },  // Cyan #06b6d4
    { stop: 0.67, r: 234, g: 179, b: 8 },  // Yellow #eab308
    { stop: 1.0, r: 239, g: 68, b: 68 },   // Red #ef4444
  ],
  spectrum: [
    { stop: 0.0, r: 124, g: 58, b: 237 },  // Violet #7c3aed
    { stop: 0.33, r: 37, g: 99, b: 235 },  // Blue #2563eb
    { stop: 0.67, r: 16, g: 185, b: 129 }, // Emerald #10b981
    { stop: 1.0, r: 245, g: 158, b: 11 },  // Amber #f59e0b
  ],
  monochrome: [
    { stop: 0.0, r: 15, g: 23, b: 42 },    // Dark Slate #0f172a
    { stop: 1.0, r: 248, g: 250, b: 252 }, // Crisp White #f8fafc
  ],
};

function normalizePalette(palette?: string): ColorPalette {
  if (!palette) return 'classic';
  const p = palette.toLowerCase();
  if (p === 'thermal') return 'thermal';
  if (p === 'spectrum') return 'spectrum';
  if (p === 'mono' || p === 'monochrome') return 'monochrome';
  return 'classic';
}

function interpolateRgbStops(ratio: number, stops: StopDef[]): RGB {
  const t = Math.max(0, Math.min(1, ratio));

  for (let i = 0; i < stops.length - 1; i++) {
    const s1 = stops[i];
    const s2 = stops[i + 1];
    if (t >= s1.stop && t <= s2.stop) {
      const segT = (t - s1.stop) / (s2.stop - s1.stop);
      const r = Math.round(s1.r + (s2.r - s1.r) * segT);
      const g = Math.round(s1.g + (s2.g - s1.g) * segT);
      const b = Math.round(s1.b + (s2.b - s1.b) * segT);
      return { r, g, b };
    }
  }

  const last = stops[stops.length - 1];
  return { r: last.r, g: last.g, b: last.b };
}

/**
 * Calculates an RGB tuple for a value normalized between min and max for any supported palette.
 */
export function getPaletteRgb(
  value: number,
  min: number,
  max: number,
  palette?: string
): RGB {
  const activePalette = normalizePalette(palette);
  const stops = PALETTE_STOPS[activePalette];

  if (!Number.isFinite(value) || !Number.isFinite(min) || !Number.isFinite(max) || min === max) {
    return { r: stops[0].r, g: stops[0].g, b: stops[0].b };
  }

  const ratio = Math.max(0, Math.min(1, (value - min) / (max - min)));
  return interpolateRgbStops(ratio, stops);
}

/**
 * Calculates an RGB tuple for a value normalized between min and max (classic or chosen palette).
 */
export function getHeatmapRgb(
  value: number,
  min: number,
  max: number,
  palette?: string
): RGB {
  return getPaletteRgb(value, min, max, palette);
}

/**
 * Returns a CSS rgb(...) string for a value on the chosen heatmap ramp.
 */
export function getHeatmapColor(
  value: number,
  min: number,
  max: number,
  palette?: string
): string {
  if (min === max) {
    return '#1e293b';
  }
  const { r, g, b } = getPaletteRgb(value, min, max, palette);
  return `rgb(${r}, ${g}, ${b})`;
}

/**
 * Returns either high-contrast light or dark text color based on cell luminance.
 */
export function getHeatmapTextColor(
  value: number,
  min: number,
  max: number,
  palette?: string
): string {
  if (min === max) {
    return '#f8fafc';
  }
  const { r, g, b } = getPaletteRgb(value, min, max, palette);
  // ITU-R BT.601 relative luminance formula
  const luminance = 0.299 * r + 0.587 * g + 0.114 * b;
  return luminance > 135 ? '#090d16' : '#f8fafc';
}
