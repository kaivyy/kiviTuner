/**
 * Grouping model for the map sidebar.
 *
 * ## Why a tree
 *
 * Calibration definitions group maps into a category tree rather than a flat list.
 * A flat list of 300 maps is unusable; grouped by category it is navigable.
 * The XDF already carries the category, so the information is present — it was
 * simply not being used.
 *
 * ## Deterministic order
 *
 * Categories sort alphabetically, maps sort by name within a category. A
 * definition that loads twice must present identically, or the user loses their
 * place between sessions.
 */

import type { MapDef } from '../../types/kivi';

/** One category node holding its maps. */
export interface MapTreeGroup {
  /** Category name, or `Uncategorised` when the XDF gave none. */
  category: string;
  maps: MapDef[];
}

/** Label used when a map has no category. */
export const UNCATEGORISED = 'Uncategorised';

/**
 * Group maps by their `category` field.
 *
 * An empty or whitespace-only category is treated as uncategorised rather than
 * becoming a group named `""`.
 */
export function groupMapsByCategory(maps: MapDef[]): MapTreeGroup[] {
  const buckets = new Map<string, MapDef[]>();

  for (const map of maps) {
    const raw = (map.category ?? '').trim();
    const key = raw === '' ? UNCATEGORISED : raw;
    const list = buckets.get(key);
    if (list) {
      list.push(map);
    } else {
      buckets.set(key, [map]);
    }
  }

  const groups: MapTreeGroup[] = [];
  for (const [category, list] of buckets) {
    // Sort a copy so the caller's array is untouched.
    const sorted = [...list].sort((a, b) => a.name.localeCompare(b.name));
    groups.push({ category, maps: sorted });
  }

  groups.sort((a, b) => {
    // Uncategorised always sorts last: it is the residual bucket, not a name.
    if (a.category === UNCATEGORISED) return 1;
    if (b.category === UNCATEGORISED) return -1;
    return a.category.localeCompare(b.category);
  });

  return groups;
}

/**
 * Filter groups by a free-text query.
 *
 * Matches the map name case-insensitively. A group with no surviving map is
 * dropped so the tree does not show empty branches.
 */
export function filterMapTree(groups: MapTreeGroup[], query: string): MapTreeGroup[] {
  const q = query.trim().toLowerCase();
  if (q === '') return groups;

  const out: MapTreeGroup[] = [];
  for (const g of groups) {
    const hits = g.maps.filter((m) => m.name.toLowerCase().includes(q));
    if (hits.length > 0) {
      out.push({ category: g.category, maps: hits });
    }
  }
  return out;
}

/** Parameter dimension filter separating 2D tables, 1D curves, and 1x1 scalars. */
export type MapParamTypeFilter = 'all' | '2d' | '1d' | 'scalar';

/** Classify a `MapDef` into `2d`, `1d`, or `scalar` (`1x1`). */
export function classifyMapParamType(map: MapDef): Exclude<MapParamTypeFilter, 'all'> {
  if (map.rows <= 1 && map.cols <= 1) return 'scalar';
  if (map.rows <= 1 || map.cols <= 1) return '1d';
  return '2d';
}

/** Count maps per parameter type category without mutating the input list. */
export function countMapsByParamType(maps: MapDef[]): Record<MapParamTypeFilter, number> {
  const counts: Record<MapParamTypeFilter, number> = {
    all: maps.length,
    '2d': 0,
    '1d': 0,
    scalar: 0,
  };
  for (const m of maps) {
    counts[classifyMapParamType(m)]++;
  }
  return counts;
}

/** Filters grouped tree by parameter dimension (`all`, `2d`, `1d`, `scalar`), dropping empty branches. */
export function filterMapTreeByParamType(
  groups: MapTreeGroup[],
  kind: MapParamTypeFilter
): MapTreeGroup[] {
  if (kind === 'all') return groups;
  const out: MapTreeGroup[] = [];
  for (const g of groups) {
    const hits = g.maps.filter((m) => classifyMapParamType(m) === kind);
    if (hits.length > 0) {
      out.push({ category: g.category, maps: hits });
    }
  }
  return out;
}

/** Total map count across groups, for the sidebar header. */
export function countMaps(groups: MapTreeGroup[]): number {
  return groups.reduce((n, g) => n + g.maps.length, 0);
}
