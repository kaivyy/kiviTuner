/**
 * Reference super-catalogue controller.
 *
 * Keeps `invoke` out of components and enforces distinct handling for
 * no-backend, input validation errors, empty search matches, and valid rows.
 */

import {
  bridgeCatalogueStats,
  bridgeListChips,
  bridgeListEcuFamilies,
  bridgeListFlasherCapabilities,
  bridgeListReferenceModules,
  bridgeListVehicleModels,
  bridgeLookupChipByJedec,
  bridgeSuperCatalogueStats,
  type CatalogueStats,
  type ChipCatalogueRow,
  type EcuFamilyRow,
  type FlasherCapabilityRow,
  type ReferenceModuleRow,
  type SuperCatalogueStats,
  type VehicleModelRow,
} from '../../lib/catalogueBridge.ts';

export type CatalogueState<T> =
  | { kind: 'ok'; data: T }
  | { kind: 'no-backend'; message: string }
  | { kind: 'error'; message: string };

export type CatalogueSearchKind = 'ecu' | 'vehicle' | 'chip' | 'capability' | 'module';

const NO_BACKEND =
  'The reference catalogue needs the desktop application. The browser build has no backend.';

/** Minimum characters before a text search runs. */
export const MIN_QUERY_LEN = 2;

async function runTextQuery<T>(
  query: string,
  searchFn: (q: string) => Promise<T[] | null>
): Promise<CatalogueState<T[]>> {
  const trimmed = query.trim();
  if (trimmed.length < MIN_QUERY_LEN) {
    return {
      kind: 'error',
      message: `Type at least ${MIN_QUERY_LEN} characters.`,
    };
  }
  let rows: T[] | null;
  try {
    rows = await searchFn(trimmed);
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (rows === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: rows };
}

/** Load legacy ECU + vehicle table sizes. */
export async function runCatalogueStats(
  statsFn: () => Promise<CatalogueStats | null> = bridgeCatalogueStats
): Promise<CatalogueState<CatalogueStats>> {
  let stats: CatalogueStats | null;
  try {
    stats = await statsFn();
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (stats === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: stats };
}

/** Load unified super-catalogue counts across all 5 tables. */
export async function runSuperCatalogueStats(
  statsFn: () => Promise<SuperCatalogueStats | null> = bridgeSuperCatalogueStats
): Promise<CatalogueState<SuperCatalogueStats>> {
  let stats: SuperCatalogueStats | null;
  try {
    stats = await statsFn();
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (stats === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: stats };
}

/** Search ECU families. */
export async function runEcuSearch(
  query: string,
  searchFn: (q: string) => Promise<EcuFamilyRow[] | null> = bridgeListEcuFamilies
): Promise<CatalogueState<EcuFamilyRow[]>> {
  return runTextQuery(query, searchFn);
}

/** Search vehicle models. */
export async function runVehicleSearch(
  query: string,
  searchFn: (q: string) => Promise<VehicleModelRow[] | null> = bridgeListVehicleModels
): Promise<CatalogueState<VehicleModelRow[]>> {
  return runTextQuery(query, searchFn);
}

/** Search flash/EEPROM chips by name, vendor, or package. */
export async function runChipSearch(
  query: string,
  searchFn: (q: string) => Promise<ChipCatalogueRow[] | null> = bridgeListChips
): Promise<CatalogueState<ChipCatalogueRow[]>> {
  return runTextQuery(query, searchFn);
}

/** Parse hexadecimal unsigned integer IDs such as `EF`, `0xEF`, or `0x4018`. */
export function parseHexUint(input: string): number | null {
  const cleaned = input.trim().replace(/^0x/i, '');
  if (!cleaned || !/^[0-9a-fA-F]{1,8}$/.test(cleaned)) {
    return null;
  }
  const value = Number.parseInt(cleaned, 16);
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
    return null;
  }
  return value;
}

/** Lookup memory chips by JEDEC manufacturer + device ID hex values. */
export async function runJedecLookup(
  manufacturerHex: string,
  deviceHex: string,
  lookupFn: (mfr: number, dev: number) => Promise<ChipCatalogueRow[] | null> = bridgeLookupChipByJedec
): Promise<CatalogueState<ChipCatalogueRow[]>> {
  const mfr = parseHexUint(manufacturerHex);
  const dev = parseHexUint(deviceHex);
  if (mfr === null || dev === null) {
    return {
      kind: 'error',
      message: 'Enter valid hexadecimal JEDEC Manufacturer and Device IDs (e.g. EF and 4018).',
    };
  }
  let rows: ChipCatalogueRow[] | null;
  try {
    rows = await lookupFn(mfr, dev);
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (rows === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: rows };
}

/** Search protocol capability flags (RD/VR/WR/CK). */
export async function runFlasherCapabilitySearch(
  query: string,
  searchFn: (q: string) => Promise<FlasherCapabilityRow[] | null> = bridgeListFlasherCapabilities
): Promise<CatalogueState<FlasherCapabilityRow[]>> {
  return runTextQuery(query, searchFn);
}

/** Search reference ECU module descriptors. */
export async function runReferenceModuleSearch(
  query: string,
  searchFn: (q: string) => Promise<ReferenceModuleRow[] | null> = bridgeListReferenceModules
): Promise<CatalogueState<ReferenceModuleRow[]>> {
  return runTextQuery(query, searchFn);
}

/** Legacy ECU + vehicle scope description (retained for compatibility). */
export function describeCatalogueScope(stats: CatalogueStats): string {
  return (
    `${stats.ecuFamilyCount} ECU families and ${stats.vehicleModelCount} vehicles ` +
    `from the static ECU identity catalogue. This records what exists, not what can be flashed.`
  );
}

/** Super-catalogue scope description across all five offline tables. */
export function describeSuperCatalogueScope(stats: SuperCatalogueStats): string {
  return (
    `${stats.total_entries.toLocaleString()} reference entries ` +
    `(${stats.ecu_family_count} ECU families, ${stats.vehicle_model_count} vehicles, ` +
    `${stats.chip_catalogue_count} chips, ${stats.flasher_capability_count} reference capabilities, ` +
    `${stats.reference_module_count} reference modules). Identity and reference coverage only — ` +
    `flashing still requires verified kivi-flashing protocol & adapter support.`
  );
}

/** One line summarising search counts. */
export function summariseSearch(count: number, kind: CatalogueSearchKind): string {
  const labels: Record<CatalogueSearchKind, [string, string]> = {
    ecu: ['ECU family', 'ECU families'],
    vehicle: ['vehicle', 'vehicles'],
    chip: ['memory chip', 'memory chips'],
    capability: ['capability entry', 'capability entries'],
    module: ['reference module', 'reference modules'],
  };
  const [singular, plural] = labels[kind];
  if (count === 0) {
    return `No matching ${singular}.`;
  }
  return `${count} ${count === 1 ? singular : plural}.`;
}
