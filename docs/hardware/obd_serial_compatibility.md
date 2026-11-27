# OBD-II Serial Hardware Compatibility Matrix & Acceptance Plan

> **Scope:** Milestone 3D — Read-Only OBD-II Serial Transport & Telemetry  
> **Milestone Acceptance Gate:** *"A named combination of ECU, protocol, adapter, and supported operations is documented and tested. Passing simulator tests alone does not satisfy this gate."*

---

## 1. Candidate Hardware Targets & Profiles

The hardware configurations listed below represent **proposed candidate targets** for physical validation. **No physical device or vehicle bus is pre-declared as verified.** Compatibility status is categorized strictly according to demonstrable evidence.

| Dimension | Candidate Profile (Example Target) | Evidence Status | Physical Acceptance Status |
| :--- | :--- | :--- | :--- |
| **Primary Adapter Target** | Scantool.net OBDLink SX USB (STN1130 MCU + FTDI FT230X USB-UART Bridge) | **Candidate Profile** (Architectural proposal only) | **UNTESTED** on physical hardware |
| **Secondary Adapter Target** | Genuine ELM327 v1.4 / v1.5 compatible adapters (38,400 baud default) | **Secondary Candidate** (Subject to clone timing variance) | **UNTESTED** on physical hardware |
| **Diagnostic Protocol** | ISO 15765-4 CAN (11-bit ID, 500 kbps, standard 8-byte CAN frames) | **VERIFIED in Mock & Linux PTY** | **UNTESTED** on physical CAN bus |
| **Target Vehicle / ECU** | Passenger Vehicle OBD-II compliant ECU (e.g. MY2008+ ISO 15765-4 CAN) or dedicated CAN bench ECU simulator | **Planned Candidate Target** | **UNTESTED** on physical ECU |
| **Supported Operations** | Read-Only Mode 01 diagnostic metrics and Mode 09 identity queries | **VERIFIED in Software & PTY Suites** | **PENDING** physical bench run |

---

## 2. Software Capability Matrix

### Mode 01 Parameter Identifiers (Read-Only)

| PID | Description | Raw J1979 Formula / Range | Software Status | Physical Validation |
| :--- | :--- | :--- | :--- | :--- |
| `0x00` | Supported PIDs `0x01..=0x20` | 32-bit bitmap | Verified (Mock & PTY) | Planned |
| `0x05` | Engine Coolant Temperature | $-40^\circ\text{C} \dots +215^\circ\text{C}$ | Verified (Mock) | Planned |
| `0x0B` | Intake Manifold Absolute Pressure | $0 \dots 255\text{ kPa}$ | Verified (Mock) | Planned |
| `0x0C` | Engine Speed (RPM) | $0.0 \dots 16,383.75\text{ RPM}$ | Verified (Mock & PTY) | Planned |
| `0x0D` | Vehicle Speed | $0 \dots 255\text{ km/h}$ | Verified (Mock) | Planned |
| `0x0F` | Intake Air Temperature | $-40^\circ\text{C} \dots +215^\circ\text{C}$ | Verified (Mock) | Planned |
| `0x11` | Absolute Throttle Position | $0.0\% \dots 100.0\%$ | Verified (Mock) | Planned |
| `0x20` | Supported PIDs `0x21..=0x40` | 32-bit bitmap | Verified (Mock) | Planned |
| `0x34` | Wideband O2 Lambda & Current | $\lambda = (A \cdot 256 + B)/32768,\; I = ((C \cdot 256 + D)/256) - 128\text{ mA}$ | Verified (Mock & PTY) | Planned |
| `0x42` | Control Module / Battery Voltage | $0.0 \dots 65.535\text{ V}$ | Verified (Mock) | Planned |

### Mode 09 Metadata (Read-Only)

| Item | Description | Codec Status | Physical Validation |
| :--- | :--- | :--- | :--- |
| `0x02` | Vehicle Identification Number (VIN) | Verified (17-char ASCII) | Planned (Deferred by default in driver) |
| `0x04` | Calibration Identification (Cal ID) | Verified (ASCII string) | Planned |

---

### 2.1 Mode 01 PID 0x34 Equivalence Ratio & Estimated AFR Semantics

#### 1. Reported Raw Data vs Calibrated AFR
SAE J1979 Mode 01 PID 0x34 reports fuel-air equivalence ratio ($\lambda$) and sensor pumping current ($I$), **not a universally calibrated Air-Fuel Ratio (AFR)**. The factory ECU calculates $\lambda$ from wideband sensor current/voltage and internal sensor calibration tables.

- **Equivalence Ratio ($\lambda$):** Scaled as $\lambda = \frac{A \times 256 + B}{32768.0}$ (unitless, valid range $0.0 \dots 1.999$). $\lambda = 1.0$ represents stoichiometry; $\lambda < 1.0$ is fuel-rich; $\lambda > 1.0$ is fuel-lean.
- **Sensor Current ($I$):** Scaled as $I = \frac{C \times 256 + D}{256.0} - 128.0\text{ mA}$ (range $-128.0\text{ mA} \dots +127.996\text{ mA}$).

#### 2. Estimated AFR Conversion Formula
In `WidebandO2Sample::estimated_afr` (`crates/kivi-protocol/src/obd/pid.rs`), the conversion to estimated AFR is strictly linear:
$$\text{AFR}_{\text{est}} = \lambda \times \text{stoichiometric\_ratio}$$

This conversion is controlled by `Obd2DriverConfig::stoichiometric_ratio`:
- If `stoichiometric_ratio: Some(ratio)` is provided, the driver computes `frame.afr = sample.estimated_afr(ratio)`.
- If `stoichiometric_ratio: None` is configured, or if PID 0x34 is not reported/supported by the vehicle ECU, `frame.afr` remains `f32::NAN`.

#### 3. Stoichiometric Ratio Examples (Illustrative, Not Universal Constants)
Appropriate stoichiometric values depend on fuel chemistry, seasonal ethanol blending, and specific calibration goals. The following values are **illustrative examples**, not fixed software constants:
- **Pure Gasoline (E0):** $\approx 14.7:1$
- **Standard Pump Gasoline (E10 blend):** $\approx 14.08:1$
- **High Ethanol Blend (E85 nominal):** $\approx 9.765:1$
- **Pure Methanol (M100):** $\approx 6.4:1$
- **Pure Ethanol (E100):** $\approx 9.0:1$

Whenever non-standard fuels or seasonal ethanol blends are used, the operator must explicitly supply the accurate stoichiometric reference for that specific blend.

#### 4. Distinction from Dedicated Aftermarket Wideband Controllers
- **OBD-II Mode 01 PID 0x34:** Polled from the factory vehicle ECU over the vehicle diagnostic bus (e.g. ISO 15765-4 CAN). Measurement frequency is shared with other polled PIDs.
- **Dedicated Aftermarket Controllers (e.g. Innovate LC-2, AEM UEGO, Zeitronix, PLX):** Typically output either an analog DC voltage ($0\text{ V} \dots 5\text{ V}$) or an asynchronous serial/CAN data stream directly from the wideband controller at high update rates (e.g. 50–100 Hz).
- **No Custom Analog Curves:** Standard OBD-II PID 0x34 decoding does not interface with or process analog 0–5V voltage signals, non-linear lookup tables, or custom controller serial protocols. No custom wideband transfer curves are implemented in the current driver.

#### 5. Missing-Value Sentinel Integrity (`f32::NAN`)
The application must never fabricate a measurement, substitute zero, or extrapolate an unmeasured AFR. When wideband data is unavailable, unpolled, or unsupported by the ECU, `frame.afr` is preserved as `f32::NAN`:
- `validate_telemetry_frame` accepts `f32::NAN` exclusively for `afr` as an unmeasured sentinel, while strictly rejecting non-finite values across all other sensor fields.
- CSV serialization formats unmeasured AFR as `"NaN"`.
- CSV deserialization re-imports `"NaN"` into `f32::NAN` without corruption or fabrication.

---

### 2.2 Telemetry Polling Rate Dynamics & Transport Throughput

The application driver does **not** guarantee a universal 5–20 Hz continuous polling rate across all physical hardware. While `Obd2DriverConfig::default()` sets a scheduling target interval of 50 ms (nominal 20 Hz loop tick), actual telemetry throughput on physical hardware is governed by serial and bus latency dynamics:

1. **ECU Response Processing Time:** Physical ECUs service OBD-II diagnostic requests as low-priority background tasks. Typical ECU response turnaround ranges from 5 ms to 25 ms on modern high-speed CAN gateways, but can reach 50 ms to 100 ms on older or heavily loaded controllers.
2. **Sequential Query Round-Robin Overhead:** Diagnostic requests over standard OBD-II ELM/STN command sets are serialized: the driver sends one request (e.g. `01 0C\r`), waits for the adapter prompt (`'>'`), parses the response, and then sends the next PID. For a list of $N$ PIDs, total cycle duration is $\sum_{i=1}^N (T_{\text{tx}} + T_{\text{bus}} + T_{\text{ecu}} + T_{\text{rx}} + T_{\text{settle}})$.
3. **USB Bridge Latency Timers:** Standard USB-to-UART bridges (e.g. FTDI FT232R/FT230X, Silicon Labs CP2102) buffer incoming bytes in hardware FIFOs until a latency timer expires (default: 16 ms on Linux/Windows). Unless explicitly tuned (e.g. `setserial /dev/ttyUSB0 low_latency` reducing FTDI latency timer to 1 ms), every request-response turnaround incurs an additional 16 ms OS buffering latency.
4. **Transport Settling Delays:** Serial OBD transports introduce small settling pauses (`DEFAULT_SERIAL_SETTLING_DELAY` = 75 ms during initial handshakes) to prevent buffer overflows on microcontrollers with small UART input FIFOs.

*Conclusion:* Achievable polling frequency must be characterized empirically on the named hardware target during Milestone 3D-4 rather than assumed as an architectural constant.

---

## 3. Evidence Categories & Disclaimers

### A. Implemented & Verified Software Evidence
- **Raw Framing & Driver Pipeline:** Fail-closed 8N1 raw UART configuration, single monotonic deadline prompt framing (`'>'`), configurable settling delay (`DEFAULT_SERIAL_SETTLING_DELAY` = 75 ms), fail-closed OS input queue clearing.
- **Protocol Allowlist & State Machine:** Closed typed enum domains (`ObdCommand`, `Mode01Pid`, `AdapterCommand`), strict command classification, single-transaction mutual exclusion (`TransactionGuard`), session transition to `Faulted` on error.
- **Continuous Acquisition Engine:** `Obd2Driver<T>` implementing `EcuDriver` and `TelemetrySource`, background Tokio polling loop with bounded broadcast capacity (`TELEMETRY_BROADCAST_CAPACITY` = 64), non-fabrication of unpolled or unsupported sensor readings (leaves values NaN).
- **Automated Test Evidence (Independently Verified):**
  - **Protocol Test Suite (`cargo test -p kivi-protocol --tests`):** **216 passed, 0 failed, 0 ignored, 0 filtered** across 17 test binaries (including 10 in `obd_driver_test.rs`, 6 in `obd_serial_pty_test.rs`, 1 in `obd_driver_pty_test.rs`, 34 in `obd_transport_test.rs`, 32 in `obd_framing_test.rs`, 27 in `obd_codec_test.rs`, 12 in `datalogger_csv_test.rs`, 21 in `correlation_test.rs`, 16 in `autotune_test.rs`, 15 in `simulator_deterministic_test.rs`, 14 in `datalogger_playback_test.rs`, 7 in `datalogger_session_test.rs`, 5 in `driver_session_test.rs`, 5 in `obd_serial_transport_test.rs`, 9 in `model_validation_test.rs`, and 2 in `rom_isolation_test.rs`).
  - **Full Workspace Suite (`cargo test --workspace`):** **305 passed, 0 failed, 0 ignored, 0 filtered** across 44 test targets (36 integration and unit test targets + 8 doc-test targets).

### B. Unverified Hardware Assumptions (Explicit Disclaimers)
- **Physical Hardware Untested:** The repository currently possesses zero recorded physical bench runs with genuine OBDLink SX, OBDLink EX, STN1130, or ELM327 hardware. All hardware compatibility claims remain unverified architectural proposals.
- **Local Buffer Clearing vs Hardware Synchronization:** Flushing local OS serial buffers, sending `ATZ`, aborting Tokio tasks, or dropping Rust structs does not prove physical transport synchronization with the remote adapter MCU or vehicle bus. Physical synchronization requires verified prompt receipt and baud lock.
- **Clone Adapter Timing Bugs:** Low-cost ELM327 clones frequently exhibit truncated responses, non-standard line endings, dropped characters, and timing lockups under continuous query loads.
- **CAN Arbitration & Multi-Node Traffic:** Pseudo-terminals (PTY) and mock transports cannot emulate CAN multi-node arbitration, bit-stuffing errors, bus-off conditions, or ECU sleep/wake power transients.

---

## 4. Milestone 3D-4 Physical OBD Hardware Acceptance Plan

### 4.1 Candidate Hardware Profile & Identification Fields

During execution of Milestone 3D-4, the testing operator must document the exact physical test hardware using the following record fields. All hardware configurations, adapter models, and chipset designations represent **proposed candidate targets** and are considered verified **only after** physical interrogation and successful test execution on named hardware:

```yaml
# Physical Hardware Acceptance Record (Template)
test_metadata:
  execution_date: "NOT_MEASURED"                  # Format: YYYY-MM-DDTHH:MM:SSZ
  operator_name: "NOT_MEASURED"                   # Name or identifier of testing engineer
  test_environment: "NOT_MEASURED"                # "Standalone CAN Bench ECU Simulator" or "Physical Vehicle"
  overall_verdict: "NOT_MEASURED"                 # "PASS", "FAIL", "BLOCKED", or "NOT_RUN"

hardware_identification:
  adapter_manufacturer: "Scantool.net / OBDLink (Example Candidate)"
  adapter_model: "OBDLink SX USB (Red casing) [Candidate Target]"
  hardware_revision: "NOT_MEASURED"               # Recorded from physical label / STPR query
  firmware_version: "NOT_MEASURED"                # Recorded from ATI query (e.g. STN1130 v4.2.1)
  usb_chipset: "FTDI FT230X Basic UART [Candidate Chipset]"
  usb_vid_pid: "Candidate FT230X default: 0403:6015 (Operator must record actual observed VID:PID)"
  serial_latency_timer_ms: "NOT_MEASURED"         # Configured value (e.g. 1 ms via setserial low_latency)
  host_os: "NOT_MEASURED"                         # e.g. Linux 6.x x86_64 / Debian-based
  serial_device_path: "NOT_MEASURED"              # e.g. /dev/ttyUSB0 or COMx

target_ecu_and_bus:
  target_vehicle_or_bench: "NOT_MEASURED"         # Named test vehicle or CAN bench ECU simulator
  target_ecu: "NOT_MEASURED"                      # Specific ECU family / model (e.g. Bosch MED17, GM E38)
  diagnostic_protocol: "ISO 15765-4 CAN (11-bit ID, 500 kbps) [Configured via internal initialize()]"
  power_arrangement: "NOT_MEASURED"               # Power supply description and measured voltage
  measured_supply_voltage: null                   # Actual measured voltage in Volts (e.g. 13.8)

empirical_measurements:
  initialization_duration_ms: null                # Elapsed time for initialize() handshake
  mean_cycle_time_ms: null                        # Mean round-trip time across polled PIDs
  p95_cycle_time_ms: null                         # 95th percentile round-trip time in ms
  sample_count_60s: null                          # Total valid frames acquired in 60 seconds
  dropped_or_invalid_samples: null                # Count of corrupted, truncated, or dropped frames
  disconnect_detection_latency_ms: null           # Latency between physical pull and Faulted state

test_case_results:
  tc_3d4_01_enumeration: "NOT_MEASURED"          # "PASS", "FAIL", or "BLOCKED"
  tc_3d4_02_handshake: "NOT_MEASURED"
  tc_3d4_03_protocol_discovery: "NOT_MEASURED"
  tc_3d4_04_sustained_telemetry: "NOT_MEASURED"
  tc_3d4_05_timeout_handling: "NOT_MEASURED"
  tc_3d4_06_orderly_shutdown: "NOT_MEASURED"
  tc_3d4_07_cable_disconnect: "NOT_MEASURED"
  tc_3d4_08_reconnect_reinit: "NOT_MEASURED"
  tc_3d4_09_safety_integrity: "NOT_MEASURED"

sanitization_attestation:
  adapter_serial_number_masked: false             # Must be true before committing (STSN masked)
  vehicle_vin_masked: false                       # Must be true before committing (VIN masked)
  raw_unredacted_evidence_path: "LOCAL_ONLY_OUTSIDE_GIT"
```

> [!NOTE]
> **Candidate Identifiers & Empirical VID/PID Verification:** The VID:PID `0403:6015` listed above is a candidate FT230X datasheet default, not an empirically verified device identifier. The operator must record the actual VID/PID observed on the host system (e.g. via `lsusb -v` or OS Device Manager) during TC-3D4-01. Adapter serial numbers (`STSN`) and physical label IDs must be masked in committed logs.

---

### 4.2 Safety Invariants & Read-Only Operational Boundaries

To ensure complete safety of the target vehicle and ECU, the following operational boundaries are mandatory and absolute:

1. **Strictly Permitted Operations:**
   - **Adapter Configuration Handshake:** `ATZ` (reset), `ATE0` (echo off), `ATL0` (linefeeds off), `ATH0` (headers off), `ATS1` (spaces on), `ATSP0` / `ATSP6` (protocol selection), `ATI` (device identification), and optional STN metadata queries (`STDI`, `STI`, `STSN`).
     - *Space Delimiter Requirement:* In `crates/kivi-protocol/src/obd/session.rs`, Step 5 of `session.initialize()` issues `ATS1` (spaces on). The codec response parser (`crates/kivi-protocol/src/obd/codec.rs`) expects space-delimited hexadecimal bytes. Sending `ATS0` (spaces off) is strictly prohibited as it renders payload decoding incompatible with the parser.
     - *Internal Protocol Selection:* Protocol selection (`ATSP6` for the configured ISO 15765-4 CAN 11-bit 500 kbit/s profile, or `ATSP0` for automatic protocol search) is executed internally within `session.initialize()` (Step 6 of handshake), prior to querying ELM identity (`ATI`) and before transitioning to `ObdSessionState::Ready`. The operator must not issue ad hoc `ATSP6` commands after the session becomes `Ready`.
   - **Mode 01 Diagnostics:** Parameter IDs explicitly allowlisted in `Mode01Pid` (`0x00`, `0x05`, `0x0B`, `0x0C`, `0x0D`, `0x0F`, `0x11`, `0x20`, `0x34`, `0x42`).
   - **Mode 09 Metadata:** Mode 09 Items explicitly allowlisted in `Mode09Item` (`0x02` VIN, `0x04` Cal ID).
2. **Prohibited Operations (Zero Tolerance):**
   - **Mode 04:** Prohibit Clearing Diagnostic Trouble Codes (DTCs) or resetting Check Engine Light.
   - **Mode 08:** Prohibit all bidirectional actuator tests and system control commands.
   - **Flashing / Calibration Services:** Prohibit all UDS (ISO 14229) and KWP-2000 write routines (e.g. `0x10`, `0x27`, `0x2E`, `0x34`, `0x35`, `0x36`, `0x37`, `0x3E`, `0x31`).
   - **Raw Command Injection:** No arbitrary string or untyped byte transmission may be dispatched to the transport.
3. **Supervision & Test Environment:**
   - Physical testing must be continuously supervised by a qualified operator.
   - Initial state: Vehicle stationary, transmission in Park/Neutral, parking brake engaged, power supply voltage $\ge 12.2\text{ V}$ (provisional test-plan starting reserve reference value; not a guaranteed operating threshold).
   - If engine is running (KOER), test must be conducted outdoors or with exhaust extraction.
4. **Mandatory Abort Conditions (Emergency Stop Criteria):**
   Testing must be halted immediately if:
   - Any unexpected dash warning lamp (Check Engine, ABS, ESP/TCS, Transmission) illuminates during testing.
   - Any audible ECU anomaly, throttle oscillation, or engine stutter is observed.
   - The adapter returns repeated `CAN ERROR`, `BUS BUSY`, `FB ERROR`, or `DATA ERROR` responses.
   - Three consecutive transaction timeouts occur without automated recovery.
   - Vehicle battery or bench supply voltage drops below $11.8\text{ V}$ (provisional test-plan abort reference value; conservative testing stop condition).

> [!IMPORTANT]
> **Provisional Reference Values & Electrical Safety Responsibility:** The voltage figures (13.8 V nominal bench reference, 12.2 V starting reserve reference, 11.8 V abort reference) and timing target (1000 ms disconnect detection) are **provisional test-plan reference values only**. They do not guarantee ECU protection, do not prevent undervoltage in all cases, and do not represent universal automotive safety limits. The operator is solely responsible for determining and strictly adhering to the specific electrical, voltage, and power supply requirements of the target ECU family, power source, serial adapter, and test bench environment. Continuous real-time human supervision, active vehicle monitoring, and immediate manual abort readiness are required at all times.

---

### 4.3 Physical Acceptance Test Cases

The test suite combines automated software verification with mandatory physical hardware bench runs:

| Test ID | Test Scenario | Execution Mode | Preconditions | Permitted Operation | Expected Observable Behavior | Pass/Fail Criteria |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **TC-3D4-01** | USB Adapter Enumeration | Physical Only | Adapter plugged into host USB; no OBD connection | OS device enumeration query (`lsusb`, `dmesg`, `/dev/ttyUSB*`, or OS Device Manager) | USB-UART bridge enumerates; serial port node `/dev/ttyUSB0` (or `COMx`) created; operator observes and records actual VID:PID (candidate FT230X default is `0403:6015`) | Pass: Device node exists, is accessible with read/write permissions by current user, and actual host-enumerated VID:PID is logged in Hardware Identification Record |
| **TC-3D4-02** | Adapter Reset & Handshake | Physical & PTY | Serial port open at 115,200 baud | Driver/session `initialize()` executing internal handshake: `ATZ`, `ATE0`, `ATL0`, `ATH0`, `ATS1`, `ATSP6` (or `ATSP0`), `ATI`, and optional STN queries (`STDI`, `STI`, `STSN`) | Sequential command/response exchange; adapter returns `OK` to configurations, enables space delimiters (`ATS1`), applies configured protocol (`ATSP6`/`ATSP0`), and reports ELM/STN identity banner ending in `'>'` | Pass: `ObdAdapterIdentity` successfully parsed; session state transitions to `ObdSessionState::Ready` |
| **TC-3D4-03** | Protocol Selection Verification | Physical & PTY | Adapter connected to powered OBD-II vehicle/bench bus | Session initialization under explicit protocol configuration (`ATSP6` for ISO 15765-4 CAN 11/500k) vs fallback (`ATSP0` Auto) | Protocol selection executes internally during Step 6 of `initialize()`; adapter acknowledges `ATSP6` (or `ATSP0`) with `OK` prior to `ATI` and bus queries; no ad hoc protocol command is issued after reaching `Ready` | Pass: Adapter locks to ISO 15765-4 CAN 11/500 (or resolves CAN 11/500 via Auto) during `initialize()`; subsequent Mode 01 queries execute without protocol negotiation errors |
| **TC-3D4-04** | Supported-PID Discovery | Physical & PTY | Protocol locked; ECU powered | Mode 01 PID 0x00 (and 0x20 if continuation set) | Decodes 32-bit bitmap; logs discovered PIDs | Pass: Only discovered PIDs are scheduled for polling |
| **TC-3D4-05** | Baseline Continuous Polling | Physical Only | Discovery complete; driver streaming started | Continuous cyclic Mode 01 polling for 60 seconds | Telemetry frames broadcasted; valid RPM/TPS/CLT | Pass: Stable acquisition; throughput distribution documented |
| **TC-3D4-06** | Transport Timeout Containment | Physical & PTY | Active polling loop running | Inject delay $> \text{command\_timeout}$ (or disconnect bus) | Transaction transitions to `Faulted`; background loop exits | Pass: Session terminates without thread leaks or panics |
| **TC-3D4-07** | Sudden Cable Disconnect & Fault Detection | Physical Only | Active polling stream running | Physical USB cable abruptly unplugged from host PC during active polling loop | OS reports transport I/O error (`ENODEV`, `EIO`, or `BrokenPipe`); driver background acquisition loop detects read/write failure, halts polling loop, and transitions connection state to `ConnectionState::Faulted { error }`. (State transitions to `ConnectionState::Disconnected` only upon subsequent explicit `driver.disconnect()` teardown) | Pass: Driver detects transport fault and transitions to `ConnectionState::Faulted` without hang, spinlock, or panic; acquisition halts promptly (provisional test-plan target $\le 1000\text{ ms}$, subject to empirical timing characterization on named hardware). Explicit `driver.disconnect()` subsequently completes teardown to `Disconnected` |
| **TC-3D4-08** | Adapter Reconnect & Re-init | Physical Only | Following TC-3D4-07 disconnect | Re-plug USB cable; instantiate new `Obd2Driver` | Re-enumerates port; re-runs handshake successfully | Pass: Second session reaches `Connected` state cleanly |
| **TC-3D4-09** | Orderly Shutdown & Resource Drop | Physical & PTY | Active polling stream running | Invoke `driver.disconnect()` and drop driver instance | Task handle aborted; serial port closed cleanly | Pass: Background thread exits; file descriptor released |

*Note on Throughput Acceptance:* Do not enforce an arbitrary 20 Hz or zero dropped samples threshold. The operator must record the empirical mean cycle time and standard deviation on the named hardware target during TC-3D4-05, establishing justified baseline performance metrics.

---

### 4.4 Evidence Collection & Acceptance Gate

To pass the Milestone 3D Acceptance Gate, the following evidence artifacts must be captured and reviewed:

1. **Hardware Identification Record:** Complete hardware configuration YAML detailing observed adapter VID:PID, host OS, and target vehicle/ECU. Adapter serial numbers (`STSN` query output and physical label) must be masked in committed records.
2. **Timestamped Application Log:** Sanitized console output recording state transitions, discovery results, and error handling.
3. **Sanitized Serial Communication Trace:** Hex/ASCII log of all exchanged commands and responses, with vehicle VIN (Mode 09 PID 0x02) and adapter serial numbers (`STSN`) strictly redacted/masked before committing to Git.
4. **Recorded Datalogger Session CSV:** Versioned CSV file (schema v1.0) containing at least 60 seconds of continuous telemetry from TC-3D4-05, with vehicle VIN / metadata sanitized in headers.
5. **Throughput & Latency Analysis:** Calculated empirical mean, median, and 95th-percentile response turnaround times per PID.
6. **Signed Verification Attestation:** Operator sign-off confirming that all safety invariants were respected, zero write operations were issued, and no vehicle warning lamps were triggered.

---

### 4.5 Sensitive Evidence Redaction & Repository Sanitization Policy

To protect owner privacy, vehicle identity, and hardware credentials while maintaining rigorous verification records, the following evidence-handling rules are mandatory:

1. **Separation of Local and Committed Evidence:**
   - **Local Raw Evidence:** Full, unredacted raw logs, hardware serial numbers (`STSN`), physical device photos, and vehicle VINs may be collected locally by authorized testing personnel on the host machine for local verification, debugging, and audit trails. Raw unredacted evidence must be stored strictly outside the Git working tree (or in local, non-committed directories protected by appropriate host access controls).
   - **Committed Repository Artifacts:** Any logs, traces, terminal captures, CSV headers, YAML records, screenshots, or reports committed to the Git repository or shared publicly must be thoroughly sanitized and redacted prior to staging or committing.
2. **Mandatory Redaction Elements:**
   - **Vehicle Identification Number (VIN):** Mode 09 PID 0x02 query responses, 17-character ASCII VIN strings, or partial VIN references must be masked (e.g. `1HGCR2F8*********` or `[VIN-REDACTED]`).
   - **Adapter Serial Number:** `STSN` command responses, barcode/serial strings from physical adapter labels, and MAC/Bluetooth addresses (if applicable) must be masked (e.g. `[STN-SERIAL-REDACTED]`).
   - **Operator & Location Identifiers:** Any internal host usernames, private IP addresses, or vehicle registration/license plate data must be removed.

**Final Gate Policy:** Simulated tests and Linux PTY loopback tests verify software architecture and contract compliance, but **do not** satisfy the Milestone 3D hardware acceptance gate. Milestone 3D cannot be marked complete until the above physical evidence bundle is produced on verified hardware.

---

## 5. Multi-Transport, Extended OBD/UDS & Motorcycle Diagnostic Updates

In addition to the strict read-only telemetry boundary enforced in `crates/kivi-protocol` (`Obd2Driver`), kiviTuner now includes expanded multi-transport hardware support and workshop diagnostics across `crates/kivi-protocol` and `crates/kivi-flashing`:

- **Transport & Clone Resilience (`kivi-protocol`, `kivi-flashing`):**
  - Automatic baud probing across `38,400`, `115,200`, `9,600`, `230,400`, and `500,000` bps for USB VCP and Bluetooth SPP (`/dev/rfcomm*` / BT COM) ELM327 adapters.
  - Wi-Fi TCP ELM327 support (`WifiObdTransport` and `WifiElmDevice`, default `192.168.0.10:35000`).
  - Raw K-Line USB-UART FTDI/CH340 driver (`RawKLineDevice`) supporting `10,400`, `15,625`, and `62,500` bps plus break wake-up pulses (`70 ms / 120 ms`), alongside OpenPort 2.0 / J2534 `ISO9141_NO_CHECKSUM` and SocketCAN ISO 15765-2 codecs.
  - Clone resilience: NUL byte (`0x00`) stripping during UART reset, echo tolerance, `ATS0`/`ATCAF1` fallbacks, and live AT capability probing (`probe_elm_capabilities`).
- **Expanded OBD-II Read-Only Codecs & Diagnostic Cockpit:**
  - `Mode01Pid` expanded to 54+ SAE J1979 sensor PIDs (`0x01..=0x63`) plus `SupportedPids61_80` (`0x60`), while remaining strictly read-only in `kivi-protocol`.
  - Workshop diagnostics in `crates/kivi-flashing` (`quick_diag`, `uds_diagnostics`, `dtc_dictionary`, `moto_diag`) cover Mode `02` Freeze Frames, Mode `03`/`07`/`0A` DTCs with offline lookup, Mode `06` monitors, Multi-ECU CAN scan (`0x7E0..=0x7E7` + 29-bit), UDS `0x10`/`0x11`/`0x14`/`0x19`/`0x22`/`0x2F`/`0x31`, Honda PGM-FI K-Line (`Table 0x11/0x73/0x74/0x60`), Yamaha K-Line & Euro 5, Kawasaki KDS, and Suzuki SDS.
- **Safety Posture Remains Unchanged:** All physical transports and new diagnostic modules remain classified `ExperimentalUntested` in `hardware_matrix.rs` until verified with captured hardware traces; ELM327 adapters cannot flash ECU ROMs (`supports_flashing() == false`).
