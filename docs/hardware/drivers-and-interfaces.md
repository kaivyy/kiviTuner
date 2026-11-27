# Hardware Drivers, Cable Interfaces, and Platform Setup

kiviTuner communicates with vehicle ECUs and bench hardware through three distinct driver layers. Knowing which layer owns the driver clarifies what must be installed on your operating system versus what ships inside kiviTuner.

> **Verification Boundary:** All transports and protocol state machines below have passed automated unit tests, loopback framing checks, and virtual ECU tests in CI. Every target in [hardware_matrix.rs](../../crates/kivi-flashing/src/hardware_matrix.rs) remains classified as **`ExperimentalUntested`** until captured against physical bench/vehicle hardware. Live hardware writes are locked by default behind **Bench / Expert Write Unlock**.

---

## 1. The Three-Layer Driver Architecture

| Layer | Provided By | Responsibility |
|---|---|---|
| **Layer 1: OS USB / Kernel Driver** | Operating System (`cdc_acm`, `ftdi_sio`, `can_raw`, Windows INBOX/VCP drivers) or cable USB installer | Enumerates physical hardware as `/dev/ttyACM*`, `/dev/ttyUSB*`, `COMx`, or `can0`. |
| **Layer 2A: Built-in kiviTuner Protocol Drivers** | **Ships inside kiviTuner** (no external `.dll` or `.so` required) | Directly speaks OpenPort 2.0 USB-CDC (`at...`), Linux SocketCAN `AF_CAN` + software ISO 15765-2, ELM327/STN Serial/Bluetooth SPP/Wi-Fi TCP (`192.168.0.10:35000`), Raw K-Line USB-UART (`RawKLineDevice` FTDI/CH340), ISO 13400-2 DoIP TCP, Moates Ostrich 2.0 serial, and Speeduino/MegaSquirt INI serial. |
| **Layer 2B: Vendor SAE J2534 Pass-Thru Library** | **Cable Vendor** (`.dll` on Windows, `.so` on Linux, `.dylib` on macOS) | Implements proprietary USB protocols inside `PassThruOpen`..`PassThruIoctl` (`smj2534*.dll`, `MVCI32.dll`, `op20pt32.dll`, `libj2534_openport2.so`). kiviTuner discovers and calls this library dynamically. |

---

## 2. Supported Interfaces & OS Matrix

| Cable / Interface | Primary Driver Origin | Linux (`x86_64`) | Windows 64-bit | Windows 32-bit | macOS (`arm64`/`x64`) | Capabilities | Verification Tier |
|---|---|---|---|---|---|---|---|
| **Tactrix OpenPort 2.0 (Native USB-CDC)** | Built-in kiviTuner (`LiveOpenPort2Device`) + OS CDC-ACM | Native (`/dev/ttyACM*`) | Yes (`COMx`, or vendor J2534 DLL) | Yes | Native (`/dev/cu.usbmodem*`) | CAN / ISO 15765-2 (`ISO15765_FRAME_PAD`), K-Line (`ISO9141_NO_CHECKSUM`), VBATT (`atr 16`), Read & Write | `ExperimentalUntested` |
| **SAE J2534 Vendor Interfaces** (Scanmatik SM2/SM2 Pro, XHorse Mini-VCI, Mongoose, Godiag, official Tactrix DLLs) | Vendor J2534 `.dll`/`.so` called by kiviTuner | Optional via `~/.passthru/*.json` or `/usr/lib` `.so` | **Yes** (64-bit direct; **32-bit DLLs automatic via `kivi-j2534-bridge-x86.exe`**) | **Yes** (direct in-process 32-bit FFI; 64-bit DLLs unsupported) | Optional `.dylib` | CAN / ISO 15765-2 / K-Line (configurable TX/RX & `NO_CHECKSUM`) Read & Write + VBATT | `ExperimentalUntested` |
| **Linux SocketCAN** (CANable, Peak PCAN-USB, Kvaser, `slcan0`, `vcan0`) | Linux Kernel `AF_CAN` + built-in `SocketCanDevice` | Native (`can0`, etc.) | Use vendor J2534 or serial SLCAN | Use vendor J2534 or serial SLCAN | Not supported (`AF_CAN` is Linux-only) | Classic CAN + software ISO 15765-2 single & multi-frame segmentation Read & Write | `ExperimentalUntested` |
| **ELM327 / STN1110 / OBDLink (USB & Bluetooth SPP)** | Built-in `SerialElmDevice` & `Obd2Driver` + USB-UART/RFCOMM | Yes (`/dev/ttyUSB*`, `/dev/ttyACM*`, `/dev/rfcomm*`) | Yes (`COMx`) | Yes (`COMx`) | Yes (`/dev/cu.*`) | Auto-baud (`38400`/`115200`/`9600`), 13 bus profiles, 54+ Mode 01 PIDs, Mode 02/03/04/06/07/09/0A, Multi-ECU CAN scan, UDS `0x10`/`0x11`/`0x14`/`0x19`/`0x22`/`0x23`/`0x2F`/`0x31`, clone capability probe, Raw Terminal (Flashing disabled) | `ExperimentalUntested` |
| **ELM327 / STN Wi-Fi TCP** | Built-in `WifiElmDevice` & `WifiObdTransport` (`192.168.0.10:35000`) | Yes | Yes | Yes | Yes | Full OBD-II / UDS diagnostics, multi-ECU scan, clone probe, Raw Terminal over TCP/IP (Flashing disabled) | `ExperimentalUntested` |
| **Raw K-Line USB-UART (FTDI / CH340 KKL 409.1)** | Built-in `RawKLineDevice` + OS VCP | Yes (`/dev/ttyUSB*`) | Yes (`COMx`) | Yes (`COMx`) | Yes (`/dev/cu.usbserial*`) | `10400`/`15625`/`62500` bps, `set_break`/`clear_break` 70ms/120ms wakeup pulse, half-duplex echo strip, Honda/Yamaha/KDS/SDS & K-Line Read/Flash | `ExperimentalUntested` |
| **ISO 13400-2 DoIP / ENET Ethernet** | Built-in `DoIpDevice` (TCP/IP port `13400`) | Yes | Yes | Yes | Yes | Routing Activation (`0x0005`), Alive Check (`0x0007`), UDS Read & Write | `ExperimentalUntested` |
| **Moates Ostrich 2.0 EPROM Emulator** | Built-in `OstrichClient` (`921,600` baud 8N1) + FTDI VCP | Yes (`/dev/ttyUSB*`) | Yes (`COMx`) | Yes (`COMx`) | Yes (`/dev/cu.*`) | Bank select `0..7`, `256`-byte SRAM page Read/Write/Verify | `ExperimentalUntested` |
| **Speeduino / MegaSquirt INI Serial ECU** | Built-in `IniSerialDriver` (`'Q'`, `'A'`) + USB-UART VCP | Yes | Yes | Yes | Yes | Live realtime channel streaming (`ochBlockSize = 139` offsets) | `ExperimentalUntested` |

---

## 3. Linux Setup (Native, No Wine Required)

1. **Serial Port Permissions (`/dev/ttyACM*`, `/dev/ttyUSB*`):**
   Add your user to the serial device group (`dialout` on Debian/Ubuntu/Fedora, `uucp` on Arch), then re-login:
   ```bash
   sudo usermod -aG dialout "$USER"
   ```
2. **Optional Tactrix OpenPort 2.0 `udev` Rule (USB VID `0403` / PID `CC4D`):**
   Linux's built-in `cdc_acm` driver exposes OpenPort 2.0 as `/dev/ttyACM*` automatically. If desktop ModemManager probes the port on plug-in, create `/etc/udev/rules.d/99-openport2.rules`:
   ```udev
   SUBSYSTEM=="tty", ATTRS{idVendor}=="0403", ATTRS{idProduct}=="cc4d", MODE="0660", GROUP="dialout", ENV{ID_MM_DEVICE_IGNORE}="1"
   ```
   Reload rules:
   ```bash
   sudo udevadm control --reload-rules && sudo udevadm trigger
   ```
3. **FTDI Latency Timer Tuning (Recommended for ELM327 / STN / Ostrich 2.0):**
   Linux defaults FTDI USB-UART bridges to a `16 ms` receive timer. Lowering it to `1 ms` removes round-trip jitter:
   ```bash
   echo 1 | sudo tee /sys/bus/usb-serial/devices/ttyUSB0/latency_timer
   ```
4. **SocketCAN Setup (`can0` / `vcan0`):**
   Configure the interface bitrate before clicking **Attach SocketCAN** inside kiviTuner:
   ```bash
   sudo ip link set can0 type can bitrate 500000
   sudo ip link set up can0
   ```
   For offline loopback testing on Linux:
   ```bash
   sudo modprobe vcan
   sudo ip link add dev vcan0 type vcan
   sudo ip link set up vcan0
   ```
5. **Optional Native Linux J2534 `.so` Drivers:**
   kiviTuner scans `~/.passthru/*.json`, `/usr/share/passthru/*.json`, `/etc/passthru/*.json`, and standard library directories (`/usr/lib`, `/usr/local/lib`, `/usr/lib64`, `/usr/lib/x86_64-linux-gnu`, `/usr/lib/aarch64-linux-gnu`, `~/.passthru`, `~/.local/lib`). Example descriptor (`~/.passthru/openport2.json`):
   ```json
   {
     "NAME": "OpenPort 2.0 (Native .so)",
     "VENDOR": "Community",
     "FUNCTION_LIB": "/usr/local/lib/libj2534_openport2.so",
     "CAN": true,
     "ISO15765": true,
     "ISO9141": true,
     "ISO14230": true
   }
   ```

---

## 4. Windows Setup (64-bit & 32-bit)

### Automatic 32-bit DLL Bridging on Windows 64-bit
Many vendor J2534 drivers (such as legacy `op20pt32.dll`, `smj2534.dll`, or `MVCI32.dll`) ship only as 32-bit x86 binaries. The `kivituner-windows-x64` installer bundles `binaries/kivi-j2534-bridge-x86.exe` alongside the main application.

- When you scan or attach a **32-bit J2534 DLL** from **kiviTuner 64-bit**, `kiviTuner` spawns `kivi-j2534-bridge-x86.exe` (`CREATE_NO_WINDOW`) and forwards all `Device` calls over bounded NDJSON stdin/stdout pipes.
- In the **Hardware Flash & Dump** transport picker, bridged drivers display a **`32-bit Bridge`** badge.
- **64-bit vendor DLLs** load directly in-process via `LoadLibraryExW(..., LOAD_WITH_ALTERED_SEARCH_PATH)` so sibling runtime DLLs (`ftd2xx.dll`, etc.) resolve from the vendor driver folder.

### Native 32-bit Windows Build (`kivituner-windows-x86`)
If you run a 32-bit edition of Windows, or prefer direct in-process 32-bit FFI without a helper process, install the `windows-x86` build. Note that a 32-bit host process cannot load 64-bit `.dll` drivers.

### Registry & Environment Discovery
On Windows, [passthru_discovery.rs](../../crates/kivi-flashing/src/passthru_discovery.rs) queries both registry views natively via Win32 APIs (without spawning `reg.exe`):
- `HKLM\SOFTWARE\PassThruSupport.04.04` (`KEY_WOW64_64KEY` and `KEY_WOW64_32KEY`)
- `%VAR%` environment placeholders in `FunctionLibrary` paths (such as `%SystemRoot%`, `%ProgramFiles(x86)%`) are expanded automatically.
- Optional overrides:
  - `KIVI_J2534_DIR`: semicolon-separated extra directories to scan for J2534 DLLs.
  - `KIVI_J2534_BRIDGE_32`: explicit path to `kivi-j2534-bridge-x86.exe` during custom local builds.

---

## 5. macOS Setup

- **USB-CDC / VCP Adapters (OpenPort 2.0, ELM327, Ostrich 2.0):** Connect directly via `/dev/cu.usbmodem*` or `/dev/cu.usbserial*` without extra kernel extensions on modern macOS.
- **Optional J2534 `.dylib` Libraries:** Place descriptors or libraries under `~/Library/Application Support/PassThru`, `/Library/Application Support/PassThru`, `~/.passthru`, `/opt/homebrew/lib`, or `/usr/local/lib`.

---

## 6. Protocol Behavior & Safety Gates

1. **Default Live-Hardware Write Lock:**
   Real hardware transports refuse `flash_ecu*` commands until **Bench / Expert Write Unlock** is explicitly toggled on for the session (`allow_experimental_write`), while still requiring a stock read/backup (`has_stock_backup`), checksum verification/repair, and safe battery voltage (`12.00 V ..= 15.00 V` when adapter VBATT reporting is supported).
2. **Verified-Only Live Seed-Key Algorithms:**
   [seed_key_registry.rs](../../crates/kivi-flashing/src/seed_key_registry.rs) tracks `verified_for_live_hardware`. Any algorithm flagged as simulated or unverified is blocked when attempting a live-hardware session.
3. **ELM327 Read-Only Policy:**
   `ElmDevice::supports_flashing()` is hardcoded to `false`. Due to the ~1020 ms AT firmware timeout ceiling and text bridge framing, ELM327/STN interfaces are restricted to telemetry, diagnostics, and read-only UDS (`0x23`/`0x22`) operations.
4. **Bus Flight Recorder:**
   All real hardware transports are wrapped in `RecordingDevice` (`crates/kivi-flashing/src/flight_recorder.rs`), recording bounded (`4096`-entry) TX, RX, control, and error events exportable as `.jsonl` or Vector CAN `.asc`.

---

## 7. Troubleshooting Reference

| Symptom | Likely Cause | Resolution |
|---|---|---|
| OpenPort 2.0 `/dev/ttyACM0` returns `Permission denied` on Linux | User lacks serial group permissions or ModemManager grabs the device | Add user to `dialout`/`uucp`, re-login, and add `ID_MM_DEVICE_IGNORE="1"` udev rule. |
| 32-bit DLL shows `Not loadable` on Windows x64 | `kivi-j2534-bridge-x86.exe` missing in dev build or corrupted | Install official `windows-x64` release bundle or build `kivi-j2534-bridge` for `i686-pc-windows-msvc` and place in `src-tauri/binaries/kivi-j2534-bridge-x86.exe`. |
| J2534 `ERR_NO_FLOW_CONTROL` (`0x17`) on ISO 15765 | Missing/mismatched `TxFlags` or `FLOW_CONTROL_FILTER` CAN IDs | Verify TX (`0x7E0`) and RX (`0x7E8`) IDs match your ECU and `ISO15765_FRAME_PAD` matches channel expectations. |
| SocketCAN `Iso15765` single/multi-frame timeout | Bitrate mismatch or ECU uses non-standard TX/RX IDs | Verify `ip link` CAN bitrate (`500000` vs `250000`) and use Active Transport Protocol Override for custom CAN IDs. |
| Cheap ELM327 v2.1 clone returns `?` on `ATSH` / `ATCAF0` / `ATTP5` | Cut-down clone firmware lacks KWP2000 or custom CAN framing | Run **Probe Clone** in UI; works for standard CAN OBD-II cars/Euro 5 bikes, while older K-Line bikes need `PIC18F25K80` v1.5 ELM, `Raw K-Line` KKL FTDI, or OpenPort 2.0. |
| `Ostrich 2.0` fails version probe (`'V'`) | Incorrect port or baud rate mismatch | Verify FTDI COM port / `/dev/ttyUSB*` and select `921600` baud (`8N1`). |
| Speeduino INI serial shows zero/garbled TPS or Advance | ECU running older firmware with non-standard `[OutputChannels]` layout | Confirm firmware aligns with standard 139-byte `speeduino.ini` offsets (Advance @ `24`, TPS @ `25`, RPM @ `14..15`). |

---

## 8. Motorcycle Modified DLC Cables & Adapter Selection Guide

1. **Euro 5 Motorcycles (6-Pin Red DLC, CAN 11-bit 500 kbps):**
   - Models: Honda ADV/PCX/Vario/Stylo 160, Yamaha R15M/MT-07/R7/Euro5 NMAX, Kawasaki/Suzuki Euro 5.
   - Wiring: DLC `+12V -> OBD Pin 16`, `GND -> OBD Pin 4 & Pin 5`, `CAN-H -> OBD Pin 6`, `CAN-L -> OBD Pin 14`.
   - Adapters: Works with cheap ELM327 USB, Bluetooth SPP (`/dev/rfcomm*` or Windows BT COM), Wi-Fi TCP (`192.168.0.10:35000`), OpenPort 2.0, and J2534.
2. **Yamaha Pre-Euro 5 (3-Pin / 4-Pin DLC), Kawasaki KDS (4-Pin), & Suzuki SDS (6-Pin):**
   - Protocol: ISO 14230-4 KWP2000 Fast Init at `10,400` bps on `OBD Pin 7` (K-Line).
   - Adapters: Requires a genuine `PIC18F25K80` ELM327 v1.5 (supports `ATTP5`), a `Raw K-Line` KKL FTDI cable, or OpenPort 2.0. Cheap `v2.1` ELM clones that reject `ATTP5` will not communicate over Pin 7 K-Line.
   - Modified cable check: Many third-party 3-pin/4-pin adapter harnesses omit OBD Pin 4 or Pin 5; bridge OBD Pin 4 & Pin 5 to battery/DLC ground and verify +12V ignition voltage arrives at OBD Pin 16.
3. **Honda Pre-Euro 5 PGM-FI (4-Pin Red DLC):**
   - Protocol: Honda proprietary K-Line (`0x72`) at `10,400` bps using a `70 ms LOW / 120 ms HIGH` bus wakeup pulse.
   - Adapters: Use **Raw K-Line USB-UART** (`FTDI FT232R` / `CH340` KKL 409.1) or **Tactrix OpenPort 2.0** (`ISO9141_NO_CHECKSUM` enabled automatically). Generic ELM327 dongles cannot guarantee the 70ms/120ms wakeup break timing on Honda 4-pin bikes.
   - Leave the brown SCS pin open during scanning (shorting SCS to ground is only for manual dash blink/reset procedures).
