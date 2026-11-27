//! Offline Diagnostic Trouble Code (DTC) & Motorcycle Blink-Code Dictionary.
//!
//! Covers common SAE J2012 P/C/B/U fault codes, motorcycle EFI codes (Honda, Yamaha,
//! Kawasaki, Suzuki), and Honda PGM-FI MIL blink codes `(main, sub)`.

use serde::{Deserialize, Serialize};

/// Rich explanation for an OBD-II / UDS DTC or motorcycle blink code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtcDescription {
    pub code: String,
    pub system: String,
    pub summary: String,
    pub probable_causes: Vec<String>,
    pub severity: String,
}

fn entry(
    code: &str,
    system: &str,
    summary: &str,
    causes: &[&str],
    severity: &str,
) -> DtcDescription {
    DtcDescription {
        code: code.to_string(),
        system: system.to_string(),
        summary: summary.to_string(),
        probable_causes: causes.iter().map(|s| (*s).to_string()).collect(),
        severity: severity.to_string(),
    }
}

/// Looks up a standard SAE J2012 / OEM DTC (`P0171`, `U0100`, `C0035`, etc.) offline.
pub fn lookup_dtc_description(code: &str) -> DtcDescription {
    let norm = code.trim().to_ascii_uppercase();
    if let Some((sys, sum, causes, sev)) = lookup_exact_j2012(&norm) {
        return entry(&norm, sys, sum, causes, sev);
    }

    let chars: Vec<char> = norm.chars().collect();
    if chars.len() >= 4 {
        let prefix = chars[0];
        let sub = chars[1];
        let grp = chars[2];
        let (system, fallback_summary) = match (prefix, sub, grp) {
            ('P', '0' | '2', '1') => (
                "Powertrain: Fuel & Air Metering",
                "SAE standard fuel/air sensor, MAF/MAP, or mixture fault",
            ),
            ('P', '0' | '2', '2') => (
                "Powertrain: Fuel Injection & Pump",
                "SAE standard injector circuit or fuel pressure regulator fault",
            ),
            ('P', '0' | '2', '3') => (
                "Powertrain: Ignition & Misfire",
                "SAE standard ignition coil, CKP/CMP, or cylinder misfire fault",
            ),
            ('P', '0' | '2', '4') => (
                "Powertrain: Auxiliary Emissions",
                "SAE standard catalyst, EVAP, EGR, or O2 heater monitor fault",
            ),
            ('P', '0' | '2', '5') => (
                "Powertrain: Speed, Idle & Voltage",
                "SAE standard VSS, IACV/ETC idle, or system voltage fault",
            ),
            ('P', '0' | '2', '6') => (
                "Powertrain: ECU Internal & Outputs",
                "SAE standard ECU memory, 5V reference, or actuator driver fault",
            ),
            ('P', '0' | '2', '7' | '8' | '9') => (
                "Powertrain: Transmission & Clutch",
                "SAE standard TCU shift solenoid, pressure, or gear ratio fault",
            ),
            ('P', '1' | '3', _) => (
                "Powertrain: OEM Specific",
                "Manufacturer-specific powertrain or motorcycle EFI fault",
            ),
            ('C', _, _) => (
                "Chassis: ABS / Braking / Steering",
                "Wheel speed sensor, ABS modulator valve, or stability sensor fault",
            ),
            ('B', _, _) => (
                "Body: Immobilizer / Cluster / Lighting",
                "Body module, smart key, or instrument cluster fault",
            ),
            ('U', _, _) => (
                "Network: CAN / K-Line Communication",
                "ECU-to-ECU bus timeout, lost frame, or bus-off condition",
            ),
            _ => ("Unknown Subsystem", "Unclassified diagnostic trouble code"),
        };
        return entry(
            &norm,
            system,
            fallback_summary,
            &[
                "Inspect sensor connector pins and modified diagnostic/harness splices",
                "Verify battery voltage (12.0V-14.5V) and ECU sensor 5V reference",
                "Consult model-specific OEM service manual for exact sub-code",
            ],
            "moderate",
        );
    }

    entry(
        &norm,
        "Unknown",
        "Unrecognized DTC code format",
        &["Verify 5-character J2012 code or Honda blink code pair"],
        "info",
    )
}

/// Looks up Honda PGM-FI MIL blink codes (`main-sub`, e.g. `7-1` ECT, `8-1` TPS, `54-1` Bank Angle).
pub fn lookup_honda_blink_code(main: u8, sub: u8) -> DtcDescription {
    let code_label = format!("Honda {main}-{sub}");
    let (sum, causes, sev): (&str, &[&str], &str) = match (main, sub) {
        (1, 1) => (
            "MAP sensor low voltage (short to ground / open harness)",
            &["MAP sensor unplugged", "Loose 5V ref or broken signal wire"],
            "high",
        ),
        (1, 2) => (
            "MAP sensor high voltage (vacuum hose leak / sensor fault)",
            &["Torn intake vacuum hose", "Faulty MAP sensor"],
            "high",
        ),
        (7, 1) => (
            "ECT/EOT coolant or oil temperature sensor low voltage (hot reading)",
            &["Shorted ECT/EOT harness wire", "Faulty thermistor"],
            "moderate",
        ),
        (7, 2) => (
            "ECT/EOT coolant or oil temperature sensor high voltage (open circuit)",
            &["ECT connector unplugged", "Broken signal/ground wire"],
            "moderate",
        ),
        (8, 1) => (
            "TP (Throttle Position) sensor low voltage",
            &["TPS unplugged or damaged track", "Open 5V reference"],
            "high",
        ),
        (8, 2) => (
            "TP (Throttle Position) sensor high voltage",
            &["Open sensor ground (SG)", "Misadjusted TPS"],
            "high",
        ),
        (9, 1 | 2) => (
            "IAT (Intake Air Temperature) sensor circuit voltage out of range",
            &["Airbox IAT connector loose", "Thermistor open/shorted"],
            "low",
        ),
        (11, 1) => (
            "Vehicle Speed Sensor (VSS) no signal",
            &["Speed sensor connector/gearbox pickup fault", "Cut VSS wire"],
            "moderate",
        ),
        (12..=15, 1) => (
            "Fuel injector primary circuit malfunction",
            &["Injector coil open/short", "Injector coupler pin corrosion"],
            "high",
        ),
        (21, 1 | 2) => (
            "O2 sensor malfunction (lean/rich stuck or circuit open)",
            &["Faulty exhaust O2 sensor", "Exhaust leak before sensor"],
            "moderate",
        ),
        (23, 1 | 2) => (
            "O2 sensor heater element malfunction",
            &["Blown heater element", "Missing +12V heater supply"],
            "moderate",
        ),
        (29, 1) => (
            "IACV (Idle Air Control Valve) circuit malfunction",
            &["Sticky IACV stepper", "Broken IACV harness wire"],
            "moderate",
        ),
        (33, 2) => (
            "ECM EEPROM internal checksum or memory fault",
            &["Incomplete flash write", "ECM hardware fault"],
            "critical",
        ),
        (52, 1) => (
            "CKP (Crankshaft Position) sensor no pulse / intermittent",
            &["Damaged stator pulser coil", "Loose crank sensor coupler"],
            "critical",
        ),
        (54, 1 | 2) => (
            "Bank Angle Sensor (lean/tip-over sensor) circuit fault",
            &["Tip-over sensor inverted or unplugged", "Wiring fault"],
            "high",
        ),
        (86, 1) => (
            "Serial communication malfunction (Meter / Smart Key / ABS link)",
            &["Interrupted K-Line/DLC line", "Speedometer CAN/serial fault"],
            "moderate",
        ),
        _ => (
            "Honda PGM-FI blink fault code (check service manual for blink group)",
            &["Verify sensor connector, 5V rail, and DLC ground"],
            "moderate",
        ),
    };
    entry(&code_label, "Honda PGM-FI", sum, causes, sev)
}

#[allow(clippy::type_complexity)]
fn lookup_exact_j2012(
    code: &str,
) -> Option<(&'static str, &'static str, &'static [&'static str], &'static str)> {
    Some(match code {
        "P0100" => ("Fuel & Air", "Mass or Volume Air Flow (MAF) A Circuit", &["Unplugged/dirty MAF", "Signal/power wiring fault"], "high"),
        "P0101" => ("Fuel & Air", "MAF A Circuit Range/Performance", &["Boost/vacuum leak", "Dirty MAF element"], "moderate"),
        "P0102" => ("Fuel & Air", "MAF A Circuit Low Input", &["Open 5V/12V supply", "Short to ground"], "high"),
        "P0103" => ("Fuel & Air", "MAF A Circuit High Input", &["Short to voltage", "Open ground"], "high"),
        "P0105" => ("Fuel & Air", "MAP / Barometric Pressure Circuit", &["MAP connector loose", "Faulty MAP sensor"], "high"),
        "P0106" => ("Fuel & Air", "MAP / Barometric Pressure Range/Performance", &["Vacuum leak", "Carbon-clogged MAP port"], "moderate"),
        "P0107" => ("Fuel & Air", "MAP / Barometric Pressure Circuit Low", &["Short to ground", "Missing 5V sensor reference"], "high"),
        "P0108" => ("Fuel & Air", "MAP / Barometric Pressure Circuit High", &["Open sensor ground", "Faulty MAP sensor"], "high"),
        "P0110" => ("Fuel & Air", "Intake Air Temperature (IAT) Sensor 1 Circuit", &["IAT connector open", "Wiring fault"], "low"),
        "P0112" => ("Fuel & Air", "IAT Sensor 1 Circuit Low (High Temp)", &["Signal wire shorted to ground"], "low"),
        "P0113" => ("Fuel & Air", "IAT Sensor 1 Circuit High (Open/-40C)", &["Unplugged IAT", "Open signal/ground"], "low"),
        "P0115" => ("Fuel & Air", "Engine Coolant Temperature (ECT) Circuit", &["ECT connector fault", "Wiring open/short"], "moderate"),
        "P0116" => ("Fuel & Air", "ECT Sensor 1 Circuit Range/Performance", &["Stuck-open thermostat", "Air pocket in coolant"], "moderate"),
        "P0117" => ("Fuel & Air", "ECT Sensor 1 Circuit Low (Overtemp reading)", &["Short to ground", "Bad thermistor"], "high"),
        "P0118" => ("Fuel & Air", "ECT Sensor 1 Circuit High (Open/-40C)", &["ECT unplugged", "Broken ground return"], "moderate"),
        "P0120" => ("Throttle", "Throttle/Pedal Position Sensor A Circuit", &["TPS track worn", "Open 5V ref"], "high"),
        "P0121" => ("Throttle", "TPS/Pedal A Circuit Range/Performance", &["TPS drift vs airflow", "Dirty throttle body"], "high"),
        "P0122" => ("Throttle", "TPS/Pedal A Circuit Low Input", &["TPS unplugged", "Short to ground"], "high"),
        "P0123" => ("Throttle", "TPS/Pedal A Circuit High Input", &["Open TPS ground", "Short to +5V"], "high"),
        "P0128" => ("Cooling", "Coolant Thermostat Below Regulating Temp", &["Stuck-open thermostat"], "low"),
        "P0130" => ("O2 & Lambda", "O2 Sensor Circuit (Bank 1 Sensor 1)", &["Exhaust leak", "Aging O2 sensor"], "moderate"),
        "P0131" => ("O2 & Lambda", "O2 Sensor Circuit Low Voltage (B1S1)", &["Air/vacuum leak", "Signal shorted low"], "moderate"),
        "P0132" => ("O2 & Lambda", "O2 Sensor Circuit High Voltage (B1S1)", &["Rich mixture / leaking injector", "Signal shorted high"], "moderate"),
        "P0133" => ("O2 & Lambda", "O2 Sensor Circuit Slow Response (B1S1)", &["Fouled/aged O2 sensor"], "low"),
        "P0134" => ("O2 & Lambda", "O2 Sensor No Activity Detected (B1S1)", &["Open O2 signal wire", "Dead heater/sensor"], "moderate"),
        "P0135" => ("O2 & Lambda", "O2 Sensor Heater Circuit (B1S1)", &["Open heater element", "Blown O2 heater fuse"], "low"),
        "P0136" => ("O2 & Lambda", "O2 Sensor Circuit (B1S2)", &["Wiring damage near exhaust", "Aging rear O2"], "low"),
        "P0141" => ("O2 & Lambda", "O2 Sensor Heater Circuit (B1S2)", &["Open heater circuit", "Blown fuse"], "low"),
        "P0170" => ("Fuel Trim", "Fuel Trim Malfunction (Bank 1)", &["Incorrect fuel pressure", "Vacuum/MAF error"], "moderate"),
        "P0171" => ("Fuel Trim", "System Too Lean (Bank 1)", &["Intake vacuum leak", "Weak fuel pump / clogged injector", "Dirty MAF"], "moderate"),
        "P0172" => ("Fuel Trim", "System Too Rich (Bank 1)", &["High fuel pressure", "Leaking injector", "Purge valve stuck open"], "moderate"),
        "P0174" => ("Fuel Trim", "System Too Lean (Bank 2)", &["Vacuum leak Bank 2", "MAF under-reading"], "moderate"),
        "P0175" => ("Fuel Trim", "System Too Rich (Bank 2)", &["Excessive fuel pressure", "Leaking Bank 2 injector"], "moderate"),
        "P0190" => ("Fuel Rail", "Fuel Rail Pressure Sensor A Circuit", &["FRP sensor unplugged", "High-pressure rail wiring"], "high"),
        "P0191" => ("Fuel Rail", "Fuel Rail Pressure Sensor Range/Performance", &["Failing HPFP", "Rail pressure regulator fault"], "high"),
        "P0200" => ("Injectors", "Injector Circuit/Open", &["Shared injector power supply/relay"], "high"),
        "P0201" => ("Injectors", "Cylinder 1 Injector Circuit/Open", &["Unplugged cyl 1 injector", "Bad coil resistance"], "high"),
        "P0202" => ("Injectors", "Cylinder 2 Injector Circuit/Open", &["Unplugged cyl 2 injector", "Open ECU driver wire"], "high"),
        "P0203" => ("Injectors", "Cylinder 3 Injector Circuit/Open", &["Unplugged cyl 3 injector", "Open ECU driver wire"], "high"),
        "P0204" => ("Injectors", "Cylinder 4 Injector Circuit/Open", &["Unplugged cyl 4 injector", "Open ECU driver wire"], "high"),
        "P0230" => ("Fuel Pump", "Fuel Pump Primary Circuit", &["Fuel pump relay fault", "Blown pump fuse"], "critical"),
        "P0231" => ("Fuel Pump", "Fuel Pump Secondary Circuit Low", &["Failed fuel pump motor", "Open pump ground"], "critical"),
        "P0232" => ("Fuel Pump", "Fuel Pump Secondary Circuit High", &["Stuck fuel pump relay"], "moderate"),
        "P0234" => ("Boost", "Turbocharger/Supercharger Overboost Condition", &["Stuck wastegate/VGT vane", "Faulty boost solenoid"], "high"),
        "P0299" => ("Boost", "Turbocharger/Supercharger Underboost", &["Boost pipe/intercooler leak", "Stuck open BOV/wastegate"], "moderate"),
        "P0300" => ("Ignition", "Random/Multiple Cylinder Misfire Detected", &["Ignition coils/spark plugs", "Low fuel pressure", "Vacuum leak"], "high"),
        "P0301" => ("Ignition", "Cylinder 1 Misfire Detected", &["Cyl 1 spark plug/coil", "Cyl 1 injector", "Low compression"], "high"),
        "P0302" => ("Ignition", "Cylinder 2 Misfire Detected", &["Cyl 2 spark plug/coil", "Cyl 2 injector"], "high"),
        "P0303" => ("Ignition", "Cylinder 3 Misfire Detected", &["Cyl 3 spark plug/coil", "Cyl 3 injector"], "high"),
        "P0304" => ("Ignition", "Cylinder 4 Misfire Detected", &["Cyl 4 spark plug/coil", "Cyl 4 injector"], "high"),
        "P0325" => ("Ignition", "Knock Sensor 1 Circuit (Bank 1)", &["Disconnected knock sensor", "Corroded piezo harness"], "moderate"),
        "P0335" => ("Ignition", "Crankshaft Position (CKP) Sensor A Circuit", &["Failed CKP/pulser pickup", "Broken tone ring tooth"], "critical"),
        "P0336" => ("Ignition", "CKP Sensor A Range/Performance", &["Excessive starter cranking noise", "Misaligned flywheel/pulser"], "critical"),
        "P0340" => ("Ignition", "Camshaft Position (CMP) Sensor A Circuit", &["Failed CMP Hall sensor", "Timing chain slack"], "high"),
        "P0351" => ("Ignition", "Ignition Coil A Primary/Secondary Circuit", &["Failed coil #1", "Broken ignition driver wire"], "high"),
        "P0352" => ("Ignition", "Ignition Coil B Primary/Secondary Circuit", &["Failed coil #2", "Broken ignition driver wire"], "high"),
        "P0353" => ("Ignition", "Ignition Coil C Primary/Secondary Circuit", &["Failed coil #3", "Broken ignition driver wire"], "high"),
        "P0354" => ("Ignition", "Ignition Coil D Primary/Secondary Circuit", &["Failed coil #4", "Broken ignition driver wire"], "high"),
        "P0400" => ("EGR", "Exhaust Gas Recirculation Flow", &["Carbon-clogged EGR valve", "DPFE/vacuum fault"], "low"),
        "P0401" => ("EGR", "EGR Insufficient Flow Detected", &["Blocked EGR passage"], "low"),
        "P0420" => ("Catalyst", "Catalyst System Efficiency Below Threshold (Bank 1)", &["Degraded catalytic converter", "Exhaust leak", "Aftermarket catless downpipe"], "low"),
        "P0430" => ("Catalyst", "Catalyst System Efficiency Below Threshold (Bank 2)", &["Degraded Bank 2 catalyst", "Exhaust leak"], "low"),
        "P0441" => ("EVAP", "EVAP System Incorrect Purge Flow", &["Purge solenoid stuck", "Pinched EVAP hose"], "low"),
        "P0442" => ("EVAP", "EVAP System Small Leak Detected", &["Loose fuel cap", "Cracked purge line"], "low"),
        "P0443" => ("EVAP", "EVAP Purge Control Valve Circuit", &["Purge solenoid unplugged or open coil"], "low"),
        "P0455" => ("EVAP", "EVAP System Large Leak Detected", &["Missing/loose gas cap", "Disconnected charcoal canister line"], "low"),
        "P0500" => ("Speed/Idle", "Vehicle Speed Sensor (VSS) A", &["Disconnected gearbox VSS", "ABS/CAN speed loss"], "moderate"),
        "P0505" => ("Speed/Idle", "Idle Air Control (IAC) System", &["Dirty IACV/throttle body", "Vacuum leak"], "moderate"),
        "P0506" => ("Speed/Idle", "Idle Control RPM Lower Than Expected", &["Carbon buildup in throttle plate"], "low"),
        "P0507" => ("Speed/Idle", "Idle Control RPM Higher Than Expected", &["Unmetered intake leak", "Needs idle/TPS relearn"], "low"),
        "P0560" => ("Electrical", "System Voltage Malfunction", &["Weak battery", "Loose terminal or stator regulator"], "moderate"),
        "P0562" => ("Electrical", "System Voltage Low (<11V)", &["Discharged battery", "Failing alternator/rectifier"], "high"),
        "P0563" => ("Electrical", "System Voltage High (>15.5V)", &["Failed alternator voltage regulator / kiprok"], "critical"),
        "P0601" => ("ECU", "Internal Control Module Memory Check Sum Error", &["ROM checksum mismatch after flash", "Damaged ECU EEPROM"], "critical"),
        "P0606" => ("ECU", "Control Module Processor Fault", &["ECU watchdog reset", "Low-voltage brownout during write"], "critical"),
        "P0685" => ("ECU", "ECM/PCM Power Relay Control Circuit", &["Faulty main relay", "Ignition key switch drop"], "high"),
        "P0700" => ("TCU", "Transmission Control System (MIL Request)", &["Scan TCU CAN ID (0x7E1) for stored shift codes"], "moderate"),
        "P1600" => ("OEM EFI", "ECU Memory / Immobilizer Communication Error", &["Key transponder mismatch", "Keep-alive voltage lost"], "high"),
        "P1610" => ("OEM EFI", "Immobilizer / Smart Key Lockout Active", &["Unpaired smart key FOB", "Immobilizer antenna fault"], "critical"),
        "P2101" => ("DBW", "Throttle Actuator Control Motor Circuit Range/Perf", &["Binding butterfly valve", "Weak battery during sweep"], "high"),
        "P2119" => ("DBW", "Throttle Actuator Control Throttle Body Range", &["Needs ETC throttle relearn after battery disconnect"], "moderate"),
        "P2135" => ("DBW", "TPS 1/2 Switch Voltage Correlation Error", &["Dual-track TPS sum != 5.0V", "Corroded pedal/TB connector"], "critical"),
        "P2138" => ("DBW", "Accelerator Pedal APP 1/2 Correlation Error", &["Faulty pedal hall sensor", "Moisture in pedal harness"], "critical"),
        "C0035" => ("ABS", "Left Front Wheel Speed Sensor Circuit", &["Dirty tone ring", "Cut ABS wheel sensor wire"], "moderate"),
        "C0040" => ("ABS", "Right Front Wheel Speed Sensor Circuit", &["Dirty tone ring", "Cut ABS wheel sensor wire"], "moderate"),
        "C0045" => ("ABS", "Left Rear Wheel Speed Sensor Circuit", &["Rear wheel speed pickup fault"], "moderate"),
        "C0050" => ("ABS", "Right Rear Wheel Speed Sensor Circuit", &["Rear wheel speed pickup fault"], "moderate"),
        "U0001" => ("Network", "High Speed CAN Communication Bus Off", &["Missing 120-ohm termination", "Shorted CAN-H to CAN-L", "Wrong baud rate"], "critical"),
        "U0100" => ("Network", "Lost Communication With ECM/PCM A", &["ECU powered off", "CAN-H/L open wire", "Key off during scan"], "critical"),
        "U0101" => ("Network", "Lost Communication With TCM (Transmission)", &["TCU fuse blown", "TCU CAN stub open"], "high"),
        "U0121" => ("Network", "Lost Communication With ABS Control Module", &["ABS fuse/plug open", "CAN bus interruption"], "high"),
        "U0155" => ("Network", "Lost Communication With Instrument Cluster (IPC)", &["Speedometer unplugged", "CAN bus line open"], "moderate"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_j2012_and_honda_blink_codes() {
        let p0171 = lookup_dtc_description("p0171");
        assert_eq!(p0171.code, "P0171");
        assert!(p0171.summary.contains("Too Lean"));
        assert!(!p0171.probable_causes.is_empty());

        let u0100 = lookup_dtc_description("U0100");
        assert!(u0100.summary.contains("Lost Communication"));

        let fallback = lookup_dtc_description("P0299");
        assert!(fallback.summary.contains("Underboost"));

        let generic = lookup_dtc_description("P0498");
        assert!(generic.system.contains("Emissions"));

        let blink = lookup_honda_blink_code(54, 1);
        assert!(blink.summary.contains("Bank Angle"));
    }
}
