# kiviTuner

ECU binary calibration, structural map discovery, multi-OEM checksum workbench, and hardware flashing studio combining a Rust (`src-tauri` + 12 workspace crates) core engine with a React / TypeScript / Tailwind calibration UI.

## Architecture Overview

- **Backend Workspace (`crates/*`, `src-tauri`):**
  - `kivi-core`: Zero-copy memory map abstraction, IEEE-754 and integer `u8`/`i8`/`u16`/`i16`/`u32`/`i32`/`f32` LE/BE decoders, rounded write-back with non-finite rejection, linear/polynomial scaling, and undo/redo patch stack.
  - `kivi-defs`: Parsers and exporters for `.xdf`, `.a2l`, DAMOS/SPZ/UMP, `.kivi.json`, Speeduino/MegaSquirt `.ini`, and CSV/KP map definitions.
  - `kivi-scanner`: Multi-brand `blind_v2` 2D table & 1D curve structural discovery engine (Geary's C, Bartels RVN, turning-point gates, inline OEM descriptors, pointer-stride snapping, rule classifier), heuristic & 16-bit window detectors, masked signature calibration scripts, and stock ROM SHA-256 registry.
  - `kivi-checksum`: Multi-OEM checksum engine (Keihin, Honda, Bosch EDC15/16/17/ME3/ME7/MG1 GF(2), BMW MS41/MS43/GS20/EM2K, GM P01–E92, Subaru Denso/Hitachi, VAG Simos, generic CRC/sum, XDF checksum blocks, and declarative JSON checksum plugins).
  - `kivi-diff` / `kivi-flashing` / `kivi-j2534-bridge` / `kivi-formats` / `kivi-physics` / `kivi-project` / `kivi-protocol` / `kivi-telemetry`: Address-gated binary diff (BIN/Intel HEX/S-Record/VBF), J2534 (including Win64-to-Win32 bridge IPC), OpenPort 2.0 native USB-CDC, ELM327, DoIP, Linux SocketCAN, Ostrich 2.0 emulator, Speeduino/MegaSquirt INI serial, Bus Flight Recorder (`.jsonl`/`.asc`), default live-hardware Write Lock, engineering calculators, OBD/CAN telemetry correlation, and Auto-Tune review.
  - `src-tauri`: Single-owner multi-document ROM session state (live `memory` plus immutable `original_memory` stock baseline), virtualized 4 KiB hex/waveform paging, and streaming Tauri commands.
- **Frontend Workspace (`src/`):**
  - **2D Heatmap & Interactive 1D Curve Editor (`HeatmapTable`, `CurveProfileChart`):** Cell/selection step increment (`+`/`-`), set constant (`=`), multiply/scale (`%`/`*`), bilinear/horizontal/vertical interpolation, Gaussian 3x3 smoothing, TSV clipboard + Paste Special, live-vs-stock baseline deltas (`Engineering`, `Raw Hex`, `Raw Dec`, `Delta`, `% Delta`), multi-step `Shift+Arrow` selection, revert to stock (`Backspace`/`Delete`), and live hit-tracing dwell overlays.
  - **1D & Row-Slice Profile Chart + 3D Surface Mesh (`CurveProfileChart`, `Surface3DView`):** Interactive point dragging on 1D curves or toggled 2D slices alongside stock baseline overlay and GPU-accelerated 3D surface inspection.
  - **2D Binary Waveform & Virtualized Hex Editor (`KiviWaveform`, `HexdumpView`):** 8/16-bit LE/BE slicing, configurable column pitch, minimap scrubbing, diff/candidate overlays, 4 KiB LRU hex cache, hex/ASCII search, Shift+Click range selection, live Data Inspector strip (`u8`/`i8`/`u16`/`i16`/`u32`/`i32`/`f32`/`bin`/`chr`), and one-click Map Definition creation.
  - **Single-Owner Workspace Controls & Multi-Project Switching:** Unified menu bar (`KiviMenuBar`), `ActivityRail` + mobile bottom nav, multi-ROM project tabs, `WorkspaceTabBar` with split pane, Map Explorer (`MapPropertiesModal`), unified Discovery `ScannerDrawer`, `BinDiffModal`, `CalibrationWorkbenchModal`, `EngineeringToolkit`, `FlashPanel`, `CataloguePanel`, `TelemetryDevPanel`, `AutoTuneReviewModal`, and in-app How-To-Use tutorial modal.

## Documentation

- **Drivers & Hardware Interfaces:** [docs/hardware/drivers-and-interfaces.md](docs/hardware/drivers-and-interfaces.md) — OS vs built-in vs J2534 vendor drivers, Linux/Windows 32+64-bit/macOS setup, safety gates, and troubleshooting.
- **Building & Packaging Installers:** [docs/guides/building-and-packaging.md](docs/guides/building-and-packaging.md) — 5-target CI matrix (`.exe`, `.msi`, `.deb`, `.AppImage`, `.dmg`), local builds with the Win64 32-bit J2534 bridge, and verification commands.
- **Pre-Flash Bench Protocol:** [docs/guides/hardware-bench-validation.md](docs/guides/hardware-bench-validation.md) — Stock-first read/diff/checksum/verify checklist.
- **OBD-II Serial Acceptance Plan:** [docs/hardware/obd_serial_compatibility.md](docs/hardware/obd_serial_compatibility.md) — ELM327 / STN compatibility matrix and test criteria.

## Runtime Modes

- **Desktop App (Tauri + Rust Core):** Full multi-brand `blind_v2`, multi-OEM checksum engines, Seed/Key, hardware J2534 (Win32/Win64 + bridge)/OpenPort2/K-Line/DoIP/SocketCAN flash & dump, Ostrich 2.0 emulation, multi-ROM Rust window cache, and immutable backend `original_memory` baseline.
- **Web Preview (`npm run dev`):** Browser-safe fallback supporting local `.bin`/`.rom`/`.hex`/`.s19` loading, immutable client `originalRomData` baseline, Keihin 8-bit checksum verify/fix, Heuristic scanner, `u8`/`i8`/`u16`/`i16`/`u32`/`i32`/`f32` LE/BE stride decoding, 2D/1D/3D/Waveform/Hex editing, CSV/report/definition export, and Undo/Redo history.

## Development & Verification

```bash
# Rust checks & targeted suites
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p kivi-flashing -p kivi-protocol -p kivi-j2534-bridge -p kivituner
cargo test -p kivi-scanner --release --lib --test map_classifier_test --test multibrand_fast_blind_test

# Frontend typecheck, unit tests, production build & dev server
npm run dev -- --host 0.0.0.0
npx tsc --noEmit
npm test
npm run build
```

## Key Shortcuts

- `F1`: Keyboard Shortcuts reference | `Ctrl+K`: Command Palette
- `Ctrl+O` / `Ctrl+D` / `Ctrl+S` / `Ctrl+Shift+S`: Open ROM / Load XDF / Save / Save As
- `Ctrl+Z` / `Ctrl+Y`: Undo / Redo | `Ctrl+F` / `Ctrl+B` / `Ctrl+\`: Scanner / Diff / Split 3D
- Inside Grid: `Arrows` / `Shift+Arrows` select, `Enter` edit, `+`/`-` step, `=` set constant, `%`/`*` scale, `S` smooth 3x3, `I`/`H`/`V` interpolate, `Backspace`/`Delete` revert selection to stock.

## License

Released under the [MIT License](LICENSE).
