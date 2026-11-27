import { invoke } from '@tauri-apps/api/core';
import { isDesktopApp } from './romBridge.ts';

// ---------------------------------------------------------------------------
// Wave 8: engineering toolkit
// ---------------------------------------------------------------------------

/** Inputs for a fuel pulse-width calculation. */
export interface FuelPulseRequest {
  air_mass_g: number;
  target_lambda: number;
  battery_volts: number;
  rail_diff_kpa: number;
  cycle_dt_sec: number;
  fuel: string;
}

/** Result of a fuel pulse-width calculation. */
export interface FuelPulseResult {
  gross_ms: number;
  effective_ms: number;
  dead_time_ms: number;
  stoich_afr: number;
}

/** Result of a speed-density air-mass estimate. */
export interface AirMassResult {
  charge_temp_k: number;
  air_mass_g: number;
  implied_ve: number;
}

/** Result of a commanded spark-timing calculation. */
export interface SparkTimingResult {
  commanded_btdc: number;
  knock_retard_deg: number;
  mfb50_atdc: number;
}

/** One physical flash block. */
export interface FlashBlockView {
  start_address: number;
  length: number;
  block_type: string;
  write_safe: boolean;
}

/** A flash chip description. */
export interface FlashTargetView {
  model_name: string;
  total_size_bytes: number;
  blocks_total_bytes: number;
  blocks: FlashBlockView[];
}

/** One histogram cell. */
export interface HistogramCell {
  row: number;
  col: number;
  hits: number;
}

/** Result of histogram hit-tracing. */
export interface HistogramResult {
  row_axis: number[];
  col_axis: number[];
  cells: HistogramCell[];
  total_samples: number;
}

/** Result of a torque-structure calculation. */
export interface TorqueResult {
  driver_torque_nm: number;
  friction_nm: number;
  pumping_nm: number;
  indicated_nm: number;
  fast_path_retard_deg: number;
}

/** Result of a sensor decode. */
export interface SensorDecodeResult {
  value: number;
  unit: string;
}

/** Result of a flex-fuel blend calculation. */
export interface BlendResult {
  stoich_afr: number;
  volume_multiplier: number;
  latent_heat_kj_kg: number;
}

/** Result of a turbo compressor calculation. */
export interface CompressorResult {
  discharge_c: number;
  intercooler_out_c: number;
  egt_c: number;
}

/**
 * Computes injector pulse width from an air charge and lambda target.
 *
 * **Analysis only.** Returns a computed duration; it does not command an
 * injector.
 */
export async function bridgeComputeFuelPulse(
  req: FuelPulseRequest
): Promise<FuelPulseResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<FuelPulseResult>('compute_fuel_pulse', { req });
    } catch (err) {
      console.warn('Tauri compute_fuel_pulse IPC error:', err);
    }
  }
  return null;
}

/** Estimates air mass and charge temperature for one cylinder. */
export async function bridgeComputeAirMass(
  displacementCc: number,
  cylinders: number,
  mapKpa: number,
  veRatio: number,
  iatC: number,
  ectC: number,
  rpm: number
): Promise<AirMassResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<AirMassResult>('compute_air_mass', {
        displacementCc,
        cylinders,
        mapKpa,
        veRatio,
        iatC,
        ectC,
        rpm,
      });
    } catch (err) {
      console.warn('Tauri compute_air_mass IPC error:', err);
    }
  }
  return null;
}

/** Computes commanded spark timing with knock retard applied. */
export async function bridgeComputeSparkTiming(
  baseBtdc: number,
  ectCorrection: number,
  iatCorrection: number,
  knockRetardDeg: number
): Promise<SparkTimingResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<SparkTimingResult>('compute_spark_timing', {
        baseBtdc,
        ectCorrection,
        iatCorrection,
        knockRetardDeg,
      });
    } catch (err) {
      console.warn('Tauri compute_spark_timing IPC error:', err);
    }
  }
  return null;
}

/**
 * Describes a flash chip layout by JEDEC id.
 *
 * **Read-only.** This reports block boundaries and which blocks are safe to
 * write. It does not flash anything.
 */
export async function bridgeDescribeFlashTargetView(
  chipId: number
): Promise<FlashTargetView | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<FlashTargetView>('describe_flash_target_view', { chipId });
    } catch (err) {
      console.warn('Tauri describe_flash_target_view IPC error:', err);
    }
  }
  return null;
}

/** Builds a hit histogram from operating points. */
export async function bridgeComputeHistogram(
  rowAxis: number[],
  colAxis: number[],
  points: Array<[number, number]>
): Promise<HistogramResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<HistogramResult>('compute_histogram', {
        rowAxis,
        colAxis,
        points,
      });
    } catch (err) {
      console.warn('Tauri compute_histogram IPC error:', err);
    }
  }
  return null;
}

/** Computes the torque structure for an operating point. */
export async function bridgeComputeTorque(
  pedalPercent: number,
  rpm: number,
  mapKpa: number,
  ectC: number,
  accessoryLoadNm: number,
  targetTorqueNm: number
): Promise<TorqueResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<TorqueResult>('compute_torque', {
        pedalPercent,
        rpm,
        mapKpa,
        ectC,
        accessoryLoadNm,
        targetTorqueNm,
      });
    } catch (err) {
      console.warn('Tauri compute_torque IPC error:', err);
    }
  }
  return null;
}

/** Decodes a raw sensor reading into engineering units. */
export async function bridgeDecodeSensor(
  kind: string,
  raw: number
): Promise<SensorDecodeResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<SensorDecodeResult>('decode_sensor', { kind, raw });
    } catch (err) {
      console.warn('Tauri decode_sensor IPC error:', err);
    }
  }
  return null;
}

/** Computes flex-fuel blend properties for an ethanol percentage. */
export async function bridgeComputeBlend(
  ethanolPercent: number
): Promise<BlendResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<BlendResult>('compute_blend', { ethanolPercent });
    } catch (err) {
      console.warn('Tauri compute_blend IPC error:', err);
    }
  }
  return null;
}

/** Computes compressor discharge, intercooler outlet, and EGT. */
export async function bridgeComputeCompressor(
  tInC: number,
  pressureRatio: number,
  isentropicEfficiency: number,
  intercoolerEfficiency: number,
  ambientC: number,
  lambda: number,
  sparkRetardDeg: number
): Promise<CompressorResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<CompressorResult>('compute_compressor', {
        tInC,
        pressureRatio,
        isentropicEfficiency,
        intercoolerEfficiency,
        ambientC,
        lambda,
        sparkRetardDeg,
      });
    } catch (err) {
      console.warn('Tauri compute_compressor IPC error:', err);
    }
  }
  return null;
}

/** A parsed diagnostic trouble code. */
export interface DtcView {
  code: string;
  category: string;
  is_standard: boolean;
}

/** Parses and normalises a 5-character DTC. */
export async function bridgeParseDtc(text: string): Promise<DtcView | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<DtcView>('parse_dtc', { text });
    } catch (err) {
      console.warn('Tauri parse_dtc IPC error:', err);
    }
  }
  return null;
}

/**
 * Returns a ROM copy with one DTC switch byte disabled.
 *
 * Nothing is written to disk or to a device; the caller owns the returned buffer.
 */
export async function bridgePatchDtcDisable(
  rom: Uint8Array,
  switchAddress: number
): Promise<Uint8Array | null> {
  if (isDesktopApp()) {
    try {
      const bytes = await invoke<number[]>('patch_dtc_disable', {
        rom: Array.from(rom),
        switchAddress,
      });
      return new Uint8Array(bytes);
    } catch (err) {
      console.warn('Tauri patch_dtc_disable IPC error:', err);
    }
  }
  return null;
}

/** Result of a transient-state evaluation (DFCO, anti-stall, MAF). */
export interface TransientResult {
  dfco_multiplier: number;
  anti_stall_adder_deg: number;
  maf_airflow_gps: number;
}

/** Result of an injector rescale. */
export interface RescaleResult {
  new_pulse_width_ms: number;
}

/** Evaluates DFCO, anti-stall and MAF for one operating point. */
export async function bridgeComputeTransient(
  rpm: number,
  tpsPct: number,
  speedKph: number,
  ectC: number,
  targetIdleRpm: number,
  mafVolts: number
): Promise<TransientResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<TransientResult>('compute_transient', {
        rpm,
        tpsPct,
        speedKph,
        ectC,
        targetIdleRpm,
        mafVolts,
      });
    } catch (err) {
      console.warn('Tauri compute_transient IPC error:', err);
    }
  }
  return null;
}

/** Rescales a pulse width for a different injector flow rate. */
export async function bridgeRescaleInjector(
  pwOldMs: number,
  deadOldMs: number,
  flowOldCc: number,
  flowNewCc: number,
  deadNewMs: number
): Promise<RescaleResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<RescaleResult>('rescale_injector', {
        pwOldMs,
        deadOldMs,
        flowOldCc,
        flowNewCc,
        deadNewMs,
      });
    } catch (err) {
      console.warn('Tauri rescale_injector IPC error:', err);
    }
  }
  return null;
}

/** Resamples a 1D curve onto a new axis. */
export async function bridgeResampleAxis(
  oldAxis: number[],
  oldValues: number[],
  newAxis: number[]
): Promise<number[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<number[]>('resample_axis', { oldAxis, oldValues, newAxis });
    } catch (err) {
      console.warn('Tauri resample_axis IPC error:', err);
    }
  }
  return null;
}

/** Result of a compressible throttle-flow calculation. */
export interface ThrottleFlowResult {
  mass_flow_kg_s: number;
  pressure_ratio: number;
  is_choked: boolean;
  critical_ratio: number;
}

/** Computes compressible mass flow through a throttle body. */
export async function bridgeComputeThrottleFlow(
  effectiveAreaM2: number,
  ambientPa: number,
  ambientK: number,
  manifoldPa: number
): Promise<ThrottleFlowResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<ThrottleFlowResult>('compute_throttle_flow', {
        effectiveAreaM2,
        ambientPa,
        ambientK,
        manifoldPa,
      });
    } catch (err) {
      console.warn('Tauri compute_throttle_flow IPC error:', err);
    }
  }
  return null;
}
