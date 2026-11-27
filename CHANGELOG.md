# Changelog

All notable changes to kiviTuner are documented in this file.

## [Unreleased]

## [0.1.0] - 2026-11-27

Initial public open-source release under the MIT License ([v0.1.0 Release](https://github.com/kaivyy/kiviTuner/releases/tag/v0.1.0)).

### Added

- Desktop and browser-safe React 18 workbench (`src/`) with Carbon Dark and Lab Light themes, 2D heatmap grid, 1D curve and row-slice chart, GPU 3D surface view, 2D binary waveform, virtualized 4 KiB LRU hex inspector, and immutable stock baseline isolation (`original_memory` / `originalRomData`).
- Definition and firmware codecs (`crates/kivi-defs`, `crates/kivi-formats`, `crates/kivi-diff`) supporting `.bin`, `.rom`, Intel `.hex`, Motorola `.s19`, `.vbf`, `.xdf`, `.a2l`, DAMOS/SPZ/UMP, `.kp`, `.kivi.json`, Speeduino/MegaSquirt `.ini`, CSV export, and address-gated binary diffing.
- Structural map discovery (`crates/kivi-scanner`) including `blind_v2` 2D/1D statistical discovery, heuristic detector, 16-bit window scanner, pointer-stride snapping, masked calibration signature scripts, and stock SHA-256 index.
- Multi-OEM checksum suite (`crates/kivi-checksum`) covering Keihin, Honda, Yamaha MT, Bosch MSE6/MSE8/MG1/EDC15/EDC16/ME3/ME7/M7.9.7, Siemens BMW MS41/MS43/GS20/SMG2/EMS2000, GM Delco P01 to E92, Subaru Denso/Hitachi, VAG Haldex/Simos, Mitsubishi TCU, XDF checksum blocks, and declarative JSON checksum plugins.
- Hardware flashing, diagnostics, and telemetry stack (`crates/kivi-flashing`, `crates/kivi-j2534-bridge`, `crates/kivi-protocol`, `crates/kivi-telemetry`) supporting SAE J2534 (plus out-of-process 32-bit bridge helper on Windows 64-bit), Tactrix OpenPort 2.0 USB-CDC, ELM327/STN/OBDLink, DoIP ISO 13400-2, VAG TP 2.0, Linux SocketCAN (`CAN_RAW`), Moates Ostrich 2.0, Speeduino/MegaSquirt INI serial, OBD-II CSV datalogging/playback, Auto-Tune review modal, Bus Flight Recorder, and default live-hardware Write Lock.
- Automated 5-target GitHub Actions release pipeline (`.github/workflows/build-desktop.yml`) building Windows x64/x86 (`.exe`, `.msi`), Linux (`.deb`, `.rpm`, `.AppImage`), and macOS ARM64/x64 (`.dmg`, `.app.tar.gz`).
- Public engineering documentation (`README.md`, `DESIGN.md`, `docs/hardware/drivers-and-interfaces.md`, `docs/guides/building-and-packaging.md`, `docs/guides/hardware-bench-validation.md`, `docs/hardware/obd_serial_compatibility.md`).
