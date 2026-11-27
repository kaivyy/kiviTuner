<div align="center">

<img src="public/icon-192.png" alt="kiviTuner" width="84" height="84" />

# kiviTuner

Cross-platform ECU binary calibration, structural map discovery, multi-OEM checksum workbench, and hardware flashing studio built with Rust, Tauri, and React.

[![Build Desktop Installers](https://github.com/kaivyy/kiviTuner/actions/workflows/build-desktop.yml/badge.svg)](https://github.com/kaivyy/kiviTuner/actions/workflows/build-desktop.yml)
[![Release v0.1.0](https://img.shields.io/badge/release-v0.1.0-0369A1)](https://github.com/kaivyy/kiviTuner/releases)
[![License MIT](https://img.shields.io/badge/license-MIT-166534)](LICENSE)
[![Stack Rust Tauri React](https://img.shields.io/badge/stack-Rust%20%7C%20Tauri%202%20%7C%20React%2018-1E293B)](Cargo.toml)

[Installers](#installers) · [Capabilities](#capabilities) · [Hardware Support](#hardware-support) · [Architecture](#architecture) · [Development](#development) · [Documentation](#documentation)

</div>

---

kiviTuner pairs a Rust calibration and flashing engine (`src-tauri` and 12 workspace crates) with a single-owner React workbench (`src/`) in both Carbon Dark and Lab Light themes. All calibration edits isolate against an immutable stock baseline so live deltas, differential sector flash plans, and one-key rollbacks stay exact.

## Installers

Prebuilt desktop bundles are published on [GitHub Releases](https://github.com/kaivyy/kiviTuner/releases) from tag builds (`v*`) via [build-desktop.yml](.github/workflows/build-desktop.yml).

| Platform | Target Architecture | Package Formats | Notes |
|---|---|---|---|
| Windows 64-bit | `x86_64-pc-windows-msvc` | `.exe` (NSIS), `.msi` (WiX) | Recommended on Windows. Bundles `kivi-j2534-bridge-x86.exe` so 64-bit direct and legacy 32-bit J2534 vendor DLLs both work. |
| Windows 32-bit | `i686-pc-windows-msvc` | `.exe` (NSIS), `.msi` (WiX) | Native 32-bit build for x86 Windows systems or direct in-process 32-bit J2534 DLL loading. |
| Linux | `x86_64-unknown-linux-gnu` | `.deb`, `.rpm`, `.AppImage` | Uses `libwebkit2gtk-4.1`, `libudev`, SocketCAN, and USB-CDC serial transports. |
| macOS (Apple Silicon) | `aarch64-apple-darwin` | `.dmg`, `.app.tar.gz` | Native ARM64 desktop bundle (macOS 10.15+). |
| macOS (Intel) | `x86_64-apple-darwin` | `.dmg`, `.app.tar.gz` | Native x86_64 desktop bundle (macOS 10.15+). |

Running `npm run dev` also provides a browser-safe workspace fallback for `.bin`/`.rom`/`.hex`/`.s19` editing, Keihin 8-bit checksum repair, heuristic map scanning, 2D/1D/3D/Waveform/Hex views, and Undo/Redo without native hardware drivers.

## Capabilities

| Domain | Included Functionality |
|---|---|
| Map & Curve Editing | 2D heatmap grid, interactive `1xN` / `Nx1` curve and row-slice chart, GPU 3D surface view, Engineering / Raw Hex / Raw Dec / Delta / `% Delta` modes, bilinear / H / V interpolation, 3x3 Gaussian smoothing, TSV clipboard + Paste Special, and hit-tracing dwell overlays. |
| Binary & Map Discovery | Virtualized 4 KiB LRU hex inspector with live Data Inspector (`u8`/`i8`/`u16`/`i16`/`u32`/`i32`/`f32` LE/BE), 2D binary waveform scrubber, `blind_v2` structural 2D/1D scanner (Geary's C, Bartels RVN, turning-point gates, pointer-stride snapping, rule classifier), 16-bit window scanner, address-gated binary diff, masked signature calibration scripts, and stock SHA-256 registry. |
| Checksum Engines | Native verify/repair engines for Keihin, Honda, Bosch EDC15/EDC16/EDC17/ME3/ME7/MG1 GF(2), BMW MS41/MS43/GS20/EM2K, GM P01 to E92, Subaru Denso/Hitachi, VAG Simos, generic CRC/sum, XDF checksum blocks, and declarative JSON checksum plugins. |
| Formats & Definitions | Reads and exports `.bin`, `.rom`, Intel `.hex`, Motorola `.s19`, `.vbf`, `.xdf`, `.a2l`, DAMOS/SPZ/UMP, `.kp`, `.kivi.json`, Speeduino/MegaSquirt `.ini`, and CSV map definitions. |
| Telemetry & Flashing | OBD-II and CAN live telemetry, CSV datalogger recording and playback, Auto-Tune table review modal, Bus Flight Recorder (`.jsonl`/`.asc`), default live-hardware Write Lock, differential sector flashing, and pre-flash safety gates. |

## Hardware Support

kiviTuner communicates over SAE J2534 PassThru adapters, native USB serial interfaces, Ethernet DoIP gateways, and Linux SocketCAN. See the [Drivers and Hardware Interfaces Guide](docs/hardware/drivers-and-interfaces.md) for Windows 32/64-bit bridging, Linux `udev`/SocketCAN setup, and vendor driver details.

| Interface / Protocol | Transport Layer | Driver / Setup Requirement | OS Support |
|---|---|---|---|
| SAE J2534 PassThru (`op20pt64`, `op20pt32`, `smj2534`, `mvci`) | `libloading` FFI + Win64-to-Win32 NDJSON IPC bridge | Vendor PassThru DLL installed on host | Windows 64-bit, Windows 32-bit |
| Tactrix OpenPort 2.0 USB-CDC | Built-in `serialport` protocol stack | Linux kernel `cdc_acm` inbox (`/dev/ttyACM*`); Tactrix Windows INF or USB serial driver | Windows, Linux, macOS |
| ELM327 / STN11xx / OBDLink | Async `tokio-serial` (`115,200` / `38,400` bps) | Standard OS FTDI/CP210x/CH340 USB-UART driver | Windows, Linux, macOS |
| DoIP (ISO 13400-2) | Native TCP/UDP sockets (port `13400`) | Standard OS TCP/IP networking | Windows, Linux, macOS |
| Linux SocketCAN (`CAN_RAW`) | Kernel `AF_CAN` sockets via `libc` (`can0`, `vcan0`) | Linux SocketCAN kernel modules (`ip link set can0 up`) | Linux |
| Moates Ostrich 2.0 | High-speed USB serial emulator | Standard FTDI VCP serial driver | Windows, Linux, macOS |
| Speeduino / MegaSquirt INI Serial | Typed binary page telemetry/tuning serial driver | Standard USB-UART serial port | Windows, Linux, macOS |

## Architecture

```mermaid
flowchart LR
    UI["React 18 + Zustand Workbench (src/)"] --> IPC["Tauri 2 Commands & ROM Session State (src-tauri/)"]
    IPC --> Core["Core Calibration (kivi-core, kivi-defs, kivi-scanner, kivi-checksum, kivi-diff, kivi-formats, kivi-physics, kivi-project)"]
    IPC --> Runtime["Protocol & Flashing (kivi-protocol, kivi-telemetry, kivi-flashing)"]
    Runtime --> Bridge["Out-of-Process Win32 J2534 Bridge (kivi-j2534-bridge)"]
    Runtime --> Hardware["J2534 DLLs / OpenPort 2.0 / ELM327 / DoIP / SocketCAN / Ostrich"]
    Bridge --> Hardware
```

| Crate / Module | Responsibility |
|---|---|
| `src/` | React 18 UI: `KiviMenuBar`, `ActivityRail`, `WorkspaceTabBar`, `HeatmapTable`, `CurveProfileChart`, `Surface3DView`, `KiviWaveform`, `HexdumpView`, `ScannerDrawer`, `BinDiffModal`, `CalibrationWorkbenchModal`, `FlashPanel`, `CataloguePanel`, `TelemetryDevPanel`, `AutoTuneReviewModal`, and in-app help modal. |
| `src-tauri/` | Single-owner multi-document ROM session state (`memory` + immutable `original_memory`), 4 KiB hex/waveform paging, and streaming Tauri IPC commands. |
| `crates/kivi-core` | Zero-copy memory map, signed/unsigned int & `f32` LE/BE decoders, rounded write-back, affine scaling, SHA-256 hashing, and undo/redo patch stack. |
| `crates/kivi-defs` | Parsers and exporters for `.xdf`, `.a2l`, DAMOS/SPZ/UMP, `.ini`, `.kivi.json`, CSV, interpolation, and smoothing math. |
| `crates/kivi-scanner` | `blind_v2` multi-brand structural map/curve discovery, 16-bit window scanner, heuristic detector, rule classifier, signature scripts, and stock ROM index. |
| `crates/kivi-checksum` | Multi-OEM checksum verification/repair, XDF checksum evaluator, and declarative JSON checksum plugins. |
| `crates/kivi-diff` | Address-gated firmware binary diff engine across raw BIN, Intel HEX, S-Record, and VBF images. |
| `crates/kivi-formats` | VBF, Intel HEX/S-Record containers, and GM E92 RLE compression codecs. |
| `crates/kivi-physics` & `crates/kivi-telemetry` | Engine/injector/VE calculators, OBD/CAN correlation, hit tracing, and Auto-Tune recommendation filtering. |
| `crates/kivi-project` & `crates/kivi-protocol` | Project workspace serialization, read-only OBD-II codec/session state machine, Speeduino/MegaSquirt INI serial driver, and CSV datalog playback. |
| `crates/kivi-flashing` & `crates/kivi-j2534-bridge` | Unified `Device` / `KernelSession` pipeline, J2534 FFI + 32-bit IPC bridge executable, OpenPort 2.0 USB-CDC, ELM327, DoIP, TP 2.0, SocketCAN, Bus Flight Recorder, differential flash safety gates, and Write Lock. |

## Development

Prerequisites: **Node.js 24+** (or Node 22.18+ for native TypeScript test execution) and the **Rust stable** toolchain. On Linux, desktop Tauri builds also require the GTK/WebKit system packages listed in the [Building and Packaging Guide](docs/guides/building-and-packaging.md).

```bash
# 1. Install frontend dependencies
npm ci

# 2. Run browser preview (Vite dev server)
npm run dev

# 3. Verify Rust workspace & native hardware/protocol crates
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p kivi-core -p kivi-checksum -p kivi-formats -p kivi-flashing -p kivi-protocol -p kivi-j2534-bridge -p kivituner

# 4. Verify frontend types, unit tests, and production bundle
npx tsc --noEmit
npm test
npm run build

# 5. Build native desktop installer on current host OS
npx --yes @tauri-apps/cli@2 build
```

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `F1` / `Ctrl+K` | Keyboard Shortcuts modal / Command Palette |
| `Ctrl+O` / `Ctrl+D` | Open ROM image / Load definition file |
| `Ctrl+S` / `Ctrl+Shift+S` | Save ROM / Save As |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo edit |
| `Ctrl+F` / `Ctrl+B` / `Ctrl+\` | Discovery Scanner / Binary Diff / Split 3D view |
| `Arrows` / `Shift+Arrows` / `Enter` | Move grid cursor, extend selection, edit cell |
| `+` / `-` / `=` / `%` / `*` | Step cell values, set constant, or scale selection |
| `S` / `I` / `H` / `V` | Smooth 3x3 / Bilinear, Horizontal, Vertical interpolate |
| `Backspace` / `Delete` | Revert selected cells to stock baseline |

## Documentation

| Document | Contents |
|---|---|
| [Getting Started Guide](docs/guides/getting-started.md) | End-to-end ROM opening, definition loading, discovery scanning, table editing, diffing, checksums, and saving. |
| [Checksum Engines & JSON Plugins](docs/guides/checksum-plugins.md) | Built-in OEM checksum algorithms and declarative JSON custom checksum plugin schema and examples. |
| [Design System & Workspace Architecture](DESIGN.md) | Visual tokens (Carbon Dark / Lab Light AA ratios), single-owner shell, and store concurrency model. |
| [Drivers and Hardware Interfaces](docs/hardware/drivers-and-interfaces.md) | OS vs built-in vs vendor J2534 drivers, Linux `udev` / SocketCAN, Win64 32-bit bridge, and troubleshooting. |
| [Building and Packaging Installers](docs/guides/building-and-packaging.md) | 5-target GitHub Actions release matrix, local Windows/Linux/macOS build commands, and CI cache/CLI setup. |
| [Release Process](docs/guides/releasing.md) | Version-bump checklist across workspace manifests, tagged auto-publish flow, and manual draft workflow runs. |
| [Hardware Bench Validation Guide](docs/guides/hardware-bench-validation.md) | Mandatory stock-first read/diff/checksum/verify checklist and bench power supply bounds. |
| [OBD-II Serial Compatibility Matrix](docs/hardware/obd_serial_compatibility.md) | ELM327 / STN compatibility matrix, PID decoding rules, and physical acceptance test protocol. |
| [Contributing Guide](CONTRIBUTING.md) | Development prerequisites, required Rust and TypeScript test suite commands, and engineering invariants. |
| [Changelog](CHANGELOG.md) | Version history and release summary starting from `v0.1.0`. |

## Safety Notice

Automated software tests validate protocol encoding, seed-key math, checksum repair, and safety interlocks against mock transports and local harnesses, not physical silicon on your vehicle. Always verify voltage with a stabilized `12.6 V` to `13.8 V` bench supply (`>= 10 A`), capture and hash a stock ECU readback before making edits, run checksum verification prior to flashing, and follow the [Hardware Bench Validation Guide](docs/guides/hardware-bench-validation.md). Hardware operations remain the operator's responsibility.

## License

Released under the [MIT License](LICENSE).
