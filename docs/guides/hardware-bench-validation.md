# Hardware Bench Validation Guide

## Scope & Verification Boundary
Automated unit tests (`cargo test`) verify wire protocol encoding, seed-key cryptography, checksum verification/repair, kernel upload/command framing, and differential safety gates against deterministic loopback scripts (`MockDevice` / `MockKernelSession`). They **cannot** assert physical silicon electrical behavior without an attached bench harness. Hardware acceptance remains the operator's responsibility.

## Pre-Flash Bench Requirements
1. **Stabilized Bench Power Supply:** Maintain `12.6 V - 13.8 V` DC (`>= 10 A` capability on bench harness). Never flash with battery voltage below `12.0 V` or during engine cranking.
2. **Adapter Verification:**
   - **SAE J2534:** Ensure vendor library (`op20pt32`, `smj2534`, etc.) passes `list_j2534_drivers` with `loadable: true`.
   - **Tactrix OpenPort 2.0 USB-CDC:** Verify `/dev/ttyACM*` or `COMx` appears in `list_serial_ports` (`is_openport2: true`).
   - **SocketCAN (Linux):** Bring up link cleanly before attaching (`sudo ip link set can0 type can bitrate 500000 && sudo ip link set up can0`).
   - **DoIP Gateway:** Confirm TCP connectivity (`host:13400`) and correct tester/ECU logical addresses (`0x0E80` -> `0x1010` or OEM target).
3. **K-Line Adapter Notes (Honda Keihin / Shindengen):**
   - Connect K-Line (`ISO 14230 / ISO 9141`) at `10,400 bps` initial baud before fast-baud handshake (`62,500 bps` where supported).
   - For Shindengen writes, keep the stock read ROM loaded in kiviTuner's active workspace before calling `configure_flash_session(HondaShindengenKLine)` so challenge key extraction can locate `WRITE_KEY_SIG` / `EE` parameters.

## Mandatory Bench Protocol (Stock-First Read-Modify-Verify)
1. **Stock Readback:** Configure session protocol (`UdsStandard`, `HondaKeihinKLine`, `HondaShindengenKLine`, or `RamKernel`), run `dump_ecu_stream` (or `dump_ecu_uds23`), and save a verified stock `.bin` backup to disk. Verify SHA-256 and checksum status in kiviTuner's Checksums tab.
2. **Differential Check:** After calibration edits, open **Binary Diff** / **Differential Flash Plan** to verify only intended sectors changed.
3. **Checksum Repair:** Always run and verify the target ECU's OEM/XDF checksum engine before writing (note: RSA/TPROT digital signatures require OEM private keys and cannot be regenerated offline).
4. **Write & Post-Write Verification:** Run `flash_ecu_workspace_stream` and verify `verified: true` with `0` mismatches before powering down ignition.
5. **Recovery Path:** Keep the stock `.bin` open in a workspace tab; if a write is interrupted while the RAM kernel remains resident or bootloader is in programming mode, re-attach the transport and write the verified stock image first.
