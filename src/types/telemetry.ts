export interface TelemetryStatusFlags {
  check_engine_light: boolean;
  limp_mode: boolean;
  low_voltage: boolean;
  overheating: boolean;
  stale_sample: boolean;
  communication_fault: boolean;
}

export interface TelemetryFrame {
  timestamp_ms: number;
  sequence: number;
  is_simulated: boolean;
  rpm: number;
  tps: number;
  map_kpa: number;
  ect_c: number;
  iat_c: number;
  battery_volts: number;
  ignition_advance_deg: number;
  injector_pulse_width_ms: number;
  afr: number;
  speed_kph: number;
  gear: number;
  status_flags: TelemetryStatusFlags;
}

export interface EcuIdentity {
  ecu_id: string;
  vin?: string | null;
  software_version: string;
  hardware_version: string;
  protocol_name: string;
  is_simulated: boolean;
}

export type ConnectionState =
  | { state: 'Disconnected' }
  | { state: 'Connecting'; details: { attempt: number } }
  | { state: 'Connected'; details: { identity: EcuIdentity } }
  | { state: 'Reconnecting'; details: { reason: string } }
  | { state: 'Faulted'; details: { error: string } };

export interface DriverDescriptor {
  id: string;
  name: string;
  is_simulator: boolean;
}

export type RecordingState =
  | { state: 'Idle' }
  | { state: 'Recording' }
  | { state: 'Stopping' }
  | { state: 'ReadyToSave' }
  | { state: 'Saved' }
  | { state: 'Interrupted'; details: { reason: string; recoverable_samples: number } };

export interface SessionMetadata {
  session_id: string;
  schema_version: number;
  started_at_iso: string;
  ended_at_iso?: string | null;
  ecu_identity?: EcuIdentity | null;
  sample_rate_hz: number;
  total_samples_recorded: number;
  dropped_samples_count: number;
  notes: string;
}

export interface DatalogRecorderStatus {
  state: RecordingState;
  elapsed_ms: number;
  sample_count: number;
  dropped_count: number;
}

export type PlaybackState = 'Stopped' | 'Playing' | 'Paused';

export interface PlaybackStatus {
  state: PlaybackState;
  current_time_ms: number;
  total_duration_ms: number;
  current_index: number;
  total_samples: number;
  speed: number;
  loop_playback: boolean;
}

export type TelemetrySourceType = 'live' | 'playback' | 'none';

