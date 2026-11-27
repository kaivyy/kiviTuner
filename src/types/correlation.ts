import type { MapDef } from './kivi.ts';
import type { TelemetryFrame } from './telemetry.ts';

/**
 * Standard telemetry channel identifiers.
 */
export type TelemetryChannel =
  | 'rpm'
  | 'tps'
  | 'map_kpa'
  | 'ect_c'
  | 'iat_c'
  | 'battery_volts'
  | 'ignition_advance_deg'
  | 'injector_pulse_width_ms'
  | 'afr'
  | 'speed_kph'
  | 'gear';

/**
 * Table axis role.
 */
export type AxisRole = 'x_axis' | 'y_axis' | 'z_axis';

/**
 * Axis association mode for linking an axis to telemetry.
 */
export type AxisAssociationMode =
  | { mode: 'NoLink' }
  | { mode: 'ExplicitLink'; target: string }
  | { mode: 'UnitMatch' };

/**
 * Explicit configuration overrides for table axis bindings.
 */
export interface TableBindingConfig {
  x_axis?: AxisAssociationMode;
  y_axis?: AxisAssociationMode;
}

/**
 * Continuous coordinate and discrete index position along an axis.
 */
export interface AxisCoordinate {
  interval_index: number;
  fractional_pos: number;
  nearest_index: number;
  is_exact_match: boolean;
}

/**
 * Error conditions encountered during axis channel resolution.
 */
export type AxisBindingError =
  | { error: 'NoLink' }
  | { error: 'MissingUnit' }
  | { error: 'UnsupportedUnit'; details: { unit: string } }
  | { error: 'AmbiguousMatch'; details: { input: string; matched: TelemetryChannel[] } }
  | { error: 'InvalidExplicitReference'; details: { target: string; reason: string } }
  | { error: 'IncompatibleRole'; details: { channel: TelemetryChannel; expected_role: AxisRole } }
  | { error: 'NonNumericLabels'; details: { label_sample: string; index: number } }
  | { error: 'InvalidDimensions'; details: { length: number; reason: string } };

/**
 * Error conditions during breakpoint coordinate resolution.
 */
export type AxisResolutionError =
  | { error: 'EmptyAxis' }
  | { error: 'SinglePointAxis' }
  | { error: 'NonFiniteBreakpoint'; details: { index: number } }
  | { error: 'DuplicateBreakpoints'; details: { val: number; first_idx: number; second_idx: number } }
  | { error: 'NonMonotonic'; details: { index: number; prev: number; curr: number } }
  | { error: 'OutOfRange'; details: { value: number; min: number; max: number } }
  | { error: 'NonFiniteValue'; details: { value: number } };

/**
 * Status of a telemetry-to-map correlation attempt.
 */
export type CorrelationStatus =
  | { status: 'Valid' }
  | { status: 'StaleSample' }
  | { status: 'CommunicationFault' }
  | { status: 'UnboundChannel'; details: { axis_role: AxisRole; error: AxisBindingError } }
  | { status: 'AxisOutOfRange'; details: { axis_role: AxisRole; error: AxisResolutionError } }
  | { status: 'DimensionMismatch'; details: { reason: string } };

/**
 * Result of correlating a telemetry frame with a calibration table.
 */
export interface CorrelationResult {
  is_valid: boolean;
  status: CorrelationStatus;
  timestamp_ms: number;
  sequence: number;
  is_simulated: boolean;
  selected_row: number | null;
  selected_col: number | null;
  row_coordinate: AxisCoordinate | null;
  col_coordinate: AxisCoordinate | null;
  cell_raw: number | null;
  cell_eng: number | null;
  table_id: string;
}

/**
 * Request payload for `correlate_telemetry`.
 */
export interface CorrelateTelemetryRequest {
  table_id?: string;
  map_def?: MapDef;
  frame?: TelemetryFrame;
  binding_config?: TableBindingConfig;
  source_epoch?: number;
  request_id?: string;
}

/**
 * Response payload for `correlate_telemetry`.
 */
export interface CorrelateTelemetryResponse {
  request_id?: string;
  source_epoch?: number;
  table_id: string;
  source: string;
  result: CorrelationResult;
}

/**
 * Machine-readable errors for telemetry-to-map correlation IPC command.
 */
export type CorrelationCommandError =
  | { error: 'MapNotFound'; details: { table_id: string } }
  | { error: 'InvalidDimensions'; details: { rows: number; cols: number; reason: string } }
  | { error: 'TelemetryUnavailable'; details: { reason: string } }
  | { error: 'RomNotLoaded' }
  | { error: 'InternalError'; details: { message: string } };

/**
 * Identifies the physical quantity represented by a calibration map.
 */
export type TunableParameterType =
  | { type: 'AutoDetect' }
  | { type: 'VolumetricEfficiency' }
  | { type: 'InjectionTimeMs' }
  | { type: 'IgnitionAdvance' }
  | { type: 'TargetAfrTable' }
  | { type: 'Unsupported'; details: string };

/**
 * Source of Target Air-Fuel Ratio values for recommendation calculation.
 */
export type TargetAfrSource =
  | { source: 'Constant'; data: number }
  | { source: 'Table'; data: number[][] };

/**
 * Operational configuration for statistical auto-tune analysis.
 */
export interface AutoTuneConfig {
  target_afr: TargetAfrSource;
  parameter_type: TunableParameterType;
  min_ect_c: number;
  max_tps_delta_per_sec: number;
  min_rpm: number;
  max_rpm: number;
  min_afr: number;
  max_afr: number;
  min_samples_per_cell: number;
  max_cell_delta_pct: number;
  max_afr_std_dev: number;
}

/**
 * Default AutoTune configuration.
 */
export const DEFAULT_AUTOTUNE_CONFIG: AutoTuneConfig = {
  target_afr: { source: 'Constant', data: 14.7 },
  parameter_type: { type: 'AutoDetect' },
  min_ect_c: 70.0,
  max_tps_delta_per_sec: 15.0,
  min_rpm: 800.0,
  max_rpm: 9000.0,
  min_afr: 10.0,
  max_afr: 20.0,
  min_samples_per_cell: 5,
  max_cell_delta_pct: 15.0,
  max_afr_std_dev: 1.5,
};

/**
 * Status of recommendation for an individual table cell.
 */
export type CellRecommendationStatus =
  | 'Ready'
  | 'NoChange'
  | 'InsufficientEvidence'
  | 'HighVarianceWarning';

/**
 * Structured recommendation for an individual cell in a calibration map.
 */
export interface CellRecommendation {
  row: number;
  col: number;
  status: CellRecommendationStatus;
  sample_count: number;
  effective_sample_count: number;
  original_value: number;
  proposed_value: number;
  delta_percentage: number;
  unit: string;
  mean_measured_afr: number | null;
  target_afr: number | null;
  std_dev_afr: number;
  min_measured_afr: number | null;
  max_measured_afr: number | null;
  is_clamped: boolean;
  warning: string | null;
}

/**
 * Breakdown of frames rejected by the eligibility filters.
 */
export interface RejectionSummary {
  rejected_stale: number;
  rejected_fault: number;
  rejected_out_of_bounds: number;
  rejected_cold_engine: number;
  rejected_fuel_cut: number;
  rejected_transient_throttle: number;
  rejected_sensor_out_of_range: number;
  rejected_non_finite: number;
}

/**
 * Complete auto-tune recommendation report across an entire calibration map.
 */
export interface RecommendationReport {
  table_id: string;
  total_frames_scanned: number;
  eligible_frames_used: number;
  cells: CellRecommendation[];
  rejections: RejectionSummary;
  blocker_reason: string | null;
}

/**
 * Request payload for `compute_autotune_recommendations`.
 */
export interface AutoTuneRequest {
  table_id?: string;
  map_def?: MapDef;
  config: AutoTuneConfig;
  frames?: TelemetryFrame[];
  request_id?: string;
}

/**
 * Response payload for `compute_autotune_recommendations`.
 */
export interface AutoTuneResponse {
  request_id?: string;
  table_id: string;
  source_provenance: string;
  report: RecommendationReport;
}

/**
 * Machine-readable errors for Auto-Tune recommendation IPC command.
 */
export type AutoTuneCommandError =
  | { error: 'MapNotFound'; details: { table_id: string } }
  | { error: 'InvalidDimensions'; details: { rows: number; cols: number; reason: string } }
  | { error: 'InvalidConfiguration'; details: { reason: string } }
  | { error: 'NoFramesAvailable' }
  | { error: 'RomNotLoaded' }
  | { error: 'InternalError'; details: { message: string } };
