/**
 * Bridge to the unified reference super-catalogue (15,752 entries across five
 * offline tables extracted cleanly into static Rust data).
 *
 * Keeps identity tables (ECU families, vehicle models, memory chips, and
 * extracted binary module metadata) strictly separate from protocol
 * capability flags so no UI implies unverified flashing support.
 */

import { invoke } from '@tauri-apps/api/core';
import { isDesktopApp } from './tauriBridge.ts';

export interface CatalogueStats {
  ecuFamilyCount: number;
  vehicleModelCount: number;
}

export interface SuperCatalogueStats {
  ecu_family_count: number;
  vehicle_model_count: number;
  chip_catalogue_count: number;
  flasher_capability_count: number;
  reference_module_count: number;
  total_entries: number;
}

export interface EcuFamilyRow {
  category: string;
  manufacturer: string;
  family: string;
  variant: string;
}

export interface VehicleModelRow {
  class: string;
  make: string;
  model: string;
  variant: string;
  engine: string;
}

export interface ChipCatalogueRow {
  manufacturer: string;
  chip_name: string;
  package: string;
  programmer_id: number | null;
  jedec_ids: Array<[number, number]>;
}

export interface FlasherCapabilityRow {
  description: string;
  direct_read: boolean;
  virtual_read: boolean;
  write_flash: boolean;
  checksum_correct: boolean;
  no_checksum: boolean;
}

export interface ReferenceModuleRow {
  file_index: number;
  vendor_number: number;
  coverage: string;
  file_size: number;
  encrypted_blobs: number;
}

/** Table sizes. Returns `null` when no backend is present. */
export async function bridgeCatalogueStats(): Promise<CatalogueStats | null> {
  if (isDesktopApp()) {
    try {
      const raw = await invoke<Record<string, number>>('catalogue_stats');
      return {
        ecuFamilyCount: raw.ecuFamilyCount ?? raw.ecu_family_count ?? 0,
        vehicleModelCount: raw.vehicleModelCount ?? raw.vehicle_model_count ?? 0,
      };
    } catch (err) {
      console.warn('Tauri catalogue_stats IPC error:', err);
    }
  }
  return null;
}

/** Unified super-catalogue sizes across all five offline tables. */
export async function bridgeSuperCatalogueStats(): Promise<SuperCatalogueStats | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<SuperCatalogueStats>('super_catalogue_stats');
    } catch (err) {
      console.warn('Tauri super_catalogue_stats IPC error:', err);
    }
  }
  return null;
}

/** Search ECU families. Returns `null` when no backend is present. */
export async function bridgeListEcuFamilies(query: string): Promise<EcuFamilyRow[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<EcuFamilyRow[]>('list_ecu_families', { query });
    } catch (err) {
      console.warn('Tauri list_ecu_families IPC error:', err);
    }
  }
  return null;
}

/** Search vehicle models. Returns `null` when no backend is present. */
export async function bridgeListVehicleModels(
  query: string
): Promise<VehicleModelRow[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<VehicleModelRow[]>('list_vehicle_models', { query });
    } catch (err) {
      console.warn('Tauri list_vehicle_models IPC error:', err);
    }
  }
  return null;
}

/** Search embedded memory/flash chips by manufacturer, chip name, or package. */
export async function bridgeListChips(query: string): Promise<ChipCatalogueRow[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<ChipCatalogueRow[]>('list_chip_catalogue', { query });
    } catch (err) {
      console.warn('Tauri list_chip_catalogue IPC error:', err);
    }
  }
  return null;
}

/** Lookup memory chips by numeric JEDEC manufacturer + device ID pair. */
export async function bridgeLookupChipByJedec(
  manufacturerId: number,
  deviceId: number
): Promise<ChipCatalogueRow[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<ChipCatalogueRow[]>('lookup_chip_by_jedec', {
        manufacturerId,
        deviceId,
      });
    } catch (err) {
      console.warn('Tauri lookup_chip_by_jedec IPC error:', err);
    }
  }
  return null;
}

/** Search shipped flasher capability rows (RD/VR/WR/CK flags). */
export async function bridgeListFlasherCapabilities(
  query: string
): Promise<FlasherCapabilityRow[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<FlasherCapabilityRow[]>('list_flasher_capabilities', { query });
    } catch (err) {
      console.warn('Tauri list_flasher_capabilities IPC error:', err);
    }
  }
  return null;
}

/** Search extracted reference module descriptors. */
export async function bridgeListReferenceModules(
  query: string
): Promise<ReferenceModuleRow[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<ReferenceModuleRow[]>('list_reference_modules', { query });
    } catch (err) {
      console.warn('Tauri list_reference_modules IPC error:', err);
    }
  }
  return null;
}
