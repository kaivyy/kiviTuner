import type {
  TelemetryStatusFlags,
  TelemetryFrame,
  SessionMetadata,
} from '../../types/telemetry.ts';

export function flagsToHex(flags: TelemetryStatusFlags): string {
  let byte = 0;
  if (flags.check_engine_light) byte |= 0x01;
  if (flags.limp_mode) byte |= 0x02;
  if (flags.low_voltage) byte |= 0x04;
  if (flags.overheating) byte |= 0x08;
  if (flags.stale_sample) byte |= 0x10;
  if (flags.communication_fault) byte |= 0x20;
  return `0x${byte.toString(16).padStart(2, '0').toUpperCase()}`;
}

export function flagsFromHex(s: string): TelemetryStatusFlags {
  const trimmed = s.trim();
  const hexPart =
    trimmed.startsWith('0x') || trimmed.startsWith('0X')
      ? trimmed.slice(2)
      : trimmed;
  const byte = parseInt(hexPart, 16);
  if (isNaN(byte)) {
    throw new Error(`Invalid status flags hex '${s}'`);
  }
  return {
    check_engine_light: (byte & 0x01) !== 0,
    limp_mode: (byte & 0x02) !== 0,
    low_voltage: (byte & 0x04) !== 0,
    overheating: (byte & 0x08) !== 0,
    stale_sample: (byte & 0x10) !== 0,
    communication_fault: (byte & 0x20) !== 0,
  };
}

export function serializeDatalogSessionToCsv(session: {
  metadata: SessionMetadata;
  frames: TelemetryFrame[];
}): string {
  const lines: string[] = [
    '# kiviTuner Datalog Session v1.0',
    `# session_id: ${session.metadata.session_id}`,
    `# started_at: ${session.metadata.started_at_iso}`,
    `# ended_at: ${session.metadata.ended_at_iso ?? ''}`,
    `# ecu_id: ${session.metadata.ecu_identity?.ecu_id ?? ''}`,
    `# protocol: ${session.metadata.ecu_identity?.protocol_name ?? ''}`,
    `# sample_rate_hz: ${session.metadata.sample_rate_hz.toFixed(1)}`,
    `# total_samples: ${session.metadata.total_samples_recorded}`,
    `# dropped_samples: ${session.metadata.dropped_samples_count}`,
    `# notes: ${session.metadata.notes}`,
    'timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags',
  ];

  for (const f of session.frames) {
    lines.push(
      `${f.timestamp_ms},${f.sequence},${f.rpm.toFixed(1)},${f.tps.toFixed(1)},${f.map_kpa.toFixed(1)},${f.ect_c.toFixed(1)},${f.iat_c.toFixed(1)},${f.battery_volts.toFixed(2)},${f.ignition_advance_deg.toFixed(1)},${f.injector_pulse_width_ms.toFixed(2)},${f.afr.toFixed(2)},${f.speed_kph.toFixed(1)},${f.gear},${flagsToHex(f.status_flags)}`
    );
  }

  return lines.join('\n') + '\n';
}

export function parseDatalogSessionFromCsv(csv: string): {
  metadata: SessionMetadata;
  frames: TelemetryFrame[];
} {
  let text = csv;
  if (text.startsWith('\uFEFF')) {
    text = text.slice(1);
  }
  const lines = text.split(/\r?\n/);
  if (lines.length === 0 || lines[0].trim() !== '# kiviTuner Datalog Session v1.0') {
    throw new Error('Unsupported CSV schema version: expected v1.0');
  }

  const metadata: SessionMetadata = {
    session_id: `ses-${Date.now()}`,
    schema_version: 1,
    started_at_iso: new Date().toISOString(),
    ended_at_iso: null,
    ecu_identity: null,
    sample_rate_hz: 20.0,
    total_samples_recorded: 0,
    dropped_samples_count: 0,
    notes: '',
  };

  let parsedEcuId: string | null = null;
  let parsedProtocol: string | null = null;
  let headerFound = false;
  const frames: TelemetryFrame[] = [];

  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line) continue;

    if (!headerFound) {
      if (line.startsWith('#')) {
        const commentBody = line.slice(1).trim();
        const colonIdx = commentBody.indexOf(':');
        if (colonIdx !== -1) {
          const key = commentBody.slice(0, colonIdx).trim();
          const val = commentBody.slice(colonIdx + 1).trim();
          switch (key) {
            case 'session_id':
              metadata.session_id = val;
              break;
            case 'started_at':
              metadata.started_at_iso = val;
              break;
            case 'ended_at':
              metadata.ended_at_iso = val ? val : null;
              break;
            case 'ecu_id':
              if (val) parsedEcuId = val;
              break;
            case 'protocol':
              if (val) parsedProtocol = val;
              break;
            case 'sample_rate_hz':
              metadata.sample_rate_hz = parseFloat(val) || 20.0;
              break;
            case 'total_samples':
              metadata.total_samples_recorded = parseInt(val, 10) || 0;
              break;
            case 'dropped_samples':
              metadata.dropped_samples_count = parseInt(val, 10) || 0;
              break;
            case 'notes':
              metadata.notes = val;
              break;
          }
        }
        continue;
      }

      if (
        line ===
        'timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags'
      ) {
        headerFound = true;
        continue;
      } else {
        throw new Error(`Line ${i + 1}: invalid CSV column header`);
      }
    }

    if (line.startsWith('#')) continue;

    const cols = line.split(',').map((s) => s.trim());
    if (cols.length !== 14) {
      throw new Error(`Line ${i + 1}: expected 14 columns, found ${cols.length}`);
    }

    const timestamp_ms = parseInt(cols[0], 10);
    const sequence = parseInt(cols[1], 10);
    const rpm = parseFloat(cols[2]);
    const tps = parseFloat(cols[3]);
    const map_kpa = parseFloat(cols[4]);
    const ect_c = parseFloat(cols[5]);
    const iat_c = parseFloat(cols[6]);
    const battery_volts = parseFloat(cols[7]);
    const ignition_advance_deg = parseFloat(cols[8]);
    const injector_pulse_width_ms = parseFloat(cols[9]);
    const afr = parseFloat(cols[10]);
    const speed_kph = parseFloat(cols[11]);
    const gear = parseInt(cols[12], 10);
    const status_flags = flagsFromHex(cols[13]);

    if (isNaN(timestamp_ms)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for timestamp_ms ('${cols[0]}')`);
    }
    if (isNaN(sequence)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for sequence ('${cols[1]}')`);
    }
    if (isNaN(rpm)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for rpm ('${cols[2]}')`);
    }
    if (isNaN(tps)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for tps ('${cols[3]}')`);
    }
    if (isNaN(map_kpa)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for map_kpa ('${cols[4]}')`);
    }
    if (isNaN(ect_c)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for ect_c ('${cols[5]}')`);
    }
    if (isNaN(iat_c)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for iat_c ('${cols[6]}')`);
    }
    if (isNaN(battery_volts)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for battery_volts ('${cols[7]}')`);
    }
    if (isNaN(ignition_advance_deg)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for ignition_advance_deg ('${cols[8]}')`);
    }
    if (isNaN(injector_pulse_width_ms)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for injector_pulse_width_ms ('${cols[9]}')`);
    }
    if (isNaN(afr)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for afr ('${cols[10]}')`);
    }
    if (isNaN(speed_kph)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for speed_kph ('${cols[11]}')`);
    }
    if (isNaN(gear)) {
      throw new Error(`Line ${i + 1}: invalid numeric data for gear ('${cols[12]}')`);
    }

    frames.push({
      timestamp_ms,
      sequence,
      is_simulated: false,
      rpm,
      tps,
      map_kpa,
      ect_c,
      iat_c,
      battery_volts,
      ignition_advance_deg,
      injector_pulse_width_ms,
      afr,
      speed_kph,
      gear,
      status_flags,
    });
  }

  if (parsedEcuId || parsedProtocol) {
    metadata.ecu_identity = {
      ecu_id: parsedEcuId ?? 'UNKNOWN',
      vin: null,
      software_version: '',
      hardware_version: '',
      protocol_name: parsedProtocol ?? 'Unknown',
      is_simulated: false,
    };
  }

  metadata.total_samples_recorded = frames.length;

  return { metadata, frames };
}

