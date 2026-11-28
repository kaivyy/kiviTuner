# Changelog

All notable changes to kiviTuner are documented in this file.

## [Unreleased]

## [0.1.3] - 2026-11-28

Multi-document detachable studio tabs, resizable panels and split views, floating 3D HUD controls, horizontal scroll strips, and table/hex editing polish ([v0.1.3 Release](https://github.com/kaivyy/kiviTuner/releases/tag/v0.1.3)).

### Added

- Detachable studio tabs (`useWorkspaceTabsStore`, `DetachedWorkspaceWindow`, `WorkspaceTabContent`, `open_workspace_tab_window` in `src-tauri/src/window_manager.rs`, and `?window=tab` support in `StandaloneWindowHost`) allowing individual map, Hex, and Waveform tabs to pop out into floating workspace windows or standalone desktop OS windows with live cross-window map and ROM synchronization (`sync_workspace_maps` / `get_workspace_maps` and echo-guarded `rom-updated` events).
- Draggable panel and split resizers (`SplitResizeHandle`, `resizeModel.ts`) across `Map Explorer` (180 to 440 px, auto-collapse below 140 px), right dock (`Inspector` / `Discovery Scanner`, 220 to 480 px, auto-collapse below 160 px), `Split 3D`, `Split Wave`, and side-by-side `Workspace Split` with double-click reset.
- Reusable `HorizontalScrollStrip` (`useHorizontalStripScroll`) with wheel scrolling, compact slim scrollbar (`.kivi-tab-scrollbar`), and clean borderless, backgroundless prev/next chevron steppers wired across `WorkspaceTabBar`, `WorkspaceViewport`, `HeatmapToolbar`, `HeatmapAnalysisBar`, `HowToUseModal`, `EngineeringToolkit`, `CalibrationWorkbenchModal`, `CataloguePanel`, `FlashPanel`, `FlashDiagnosticsSection`, and `StatusBar`.

### Improved

- Overhauled `Surface3DView` with map-style click-to-expand floating overlays (`ViewportOverlayControls`): compact single-line top-left cell readout pill, top-right Camera menu (`Iso`, `Top`, `Front`, `Side`, `Reset`) and Layers popover (`Mesh`, `Solid`, `Wire`, `Cage`, `Ghost`, `Color Scale`, `Palette`, `Z-Scale`), bottom-right vertical zoom/reset capsule, and bottom-left compact horizontal color scale pill while preserving centered geometry and clear Z-axis elevation ticks across zoom levels and detached split windows.
- Unified right dock state resolver (`rightDockModel.ts`) eliminating alternating toggle glitches and overlap frames when switching between `Inspector` and `ROM Discovery Scanner`.
- Wired live inline hex byte editing (`onPatchByte`) into `HexdumpView` alongside synchronized multi-window updates.
- Updated user and architecture documentation (`README.md`, `DESIGN.md`, `docs/guides/getting-started.md`, and `howToUseData.ts`).

### Fixed

- Fixed desktop menubar and floating window dragging lockups (`data-tauri-drag-region="deep"`, `"core:window:allow-internal-toggle-maximize"` capability, and 4-direction pointer-capture dragging on `FloatingWindow`).
- Fixed 2D table column width expansion during inline cell editing (`HeatmapRow.tsx`) and inline hex byte editing (`HexdumpView.tsx`) by keeping the formatted cell span in flow and mounting `<input size={1}>` as an `absolute inset-0` overlay.
- Fixed `Map Explorer` and right dock auto-collapse visual jump where drawers briefly snapped back to default width during slide-out exit animations.
- Fixed `WorkspaceViewport` secondary split pane rendering the primary selected map instead of its independent split map (`useMapMatrix`, `resolveEffectiveMapAxes`).

## [0.1.2] - 2026-11-28

UI refinement, macOS menu bar integration, responsive layout parity, and WebView memory optimization release ([v0.1.2 Release](https://github.com/kaivyy/kiviTuner/releases/tag/v0.1.2)).

### Added

- Native global macOS top menu bar (`src-tauri/src/native_menu.rs` and `src/lib/nativeMenuBridge.ts`) placing `File`, `Edit`, `XDF`, `View`, `Reverse`, `ECU`, and `Help` in the system bar next to the Apple icon with verified 1-to-1 action parity and race-free listener cleanup, while keeping status pills and window controls inside the app titlebar without menu duplication.
- Horizontal scrollbar, wheel scrolling, and overflow arrow controls in `WorkspaceTabBar` alongside fixed `h-8` ROM strip and `h-10` editor tab bar heights so opening `Full Waveform` in empty mode causes zero vertical layout jump.
- Exclusive single-active navigation state resolver (`activityRailModel.ts`) with accent indicator bar, docked panel indicator dot, and smooth GPU transitions across `ActivityRail`, `MobileBottomNav`, and interactive workspace buttons.

### Improved

- Borderless outer-rounded window chrome across all main, floating, and standalone tool windows (`FloatingWindow`, `KiviMenuBar`, `WindowTrafficLights`, `CommandPalette`, and Windows `DWMWA_WINDOW_CORNER_PREFERENCE` / border-none styling) with pure soft elevation shadows and sharp `rounded-none` docked/overlay workspace drawers (`Map Explorer`, `ROM Discovery Scanner`, `Inspector`).
- Unified `36px` (`h-9`) borderless workspace headers across `Map Explorer`, `Inspector`, `ROM Discovery Scanner`, `Split` pane, and the center view-mode bar plus fixed `32px` (`h-8`) menubar height so small viewport resizes never inflate bar heights or wrap buttons awkwardly.
- Consistent non-wrapping analysis bar controls in `2D Table` and `Split 3D`, fixed minimum widths on loading buttons across tool dialogs to prevent width jitter, softened borders on `HexdumpView`, `KiviWaveform`, `Surface3DView`, `ScalarFlagEditor`, and reduced WebView2 RAM overhead with lazy-loaded heavy tool panels.

### Fixed

- Fixed stuck active background highlights on `ActivityRail` buttons when moving between `Search`, `Hex Viewer`, `Waveform`, and modal tools, and made `Discovery Scanner` and `Inspector` mutually exclusive on the right side.
- Fixed responsive drawer clipping where `Map Explorer` and its filter input were covered by the menubar or blocked clicks on `ActivityRail`, and auto-refitted floating windows cleanly inside narrowed viewports.
- Fixed submenu hover gap and nested `role="menuitem"` ARIA collisions in `KiviMenuBar`, plus GPU-composited open/close/minimize transitions (`useAnimatedPresence`).

## [0.1.1] - 2026-11-28

Maintenance and usability release ([v0.1.1 Release](https://github.com/kaivyy/kiviTuner/releases/tag/v0.1.1)) covering multi-transport diagnostics, structural `blind_v2` discovery upgrades, desktop windowing stability, 3D surface overhaul, and built-in app updates.

### Added

- Built-in desktop update checker and installer (`src-tauri/src/updater.rs`, `UpdateModal`, `Help -> Check for Updates...`) verifying downloaded packages against `SHA256SUMS.txt` and supporting one-click restart, plus optional Tauri updater signature bundles.
- Expanded `blind_v2` structural discovery (`crates/kivi-scanner`) with `u8`-header OEM 2D/1D struct scanners, signed `i16`/`i8` zero-crossing detection, Fisher-Yates Twin-ROM null-control audit benchmarks (`modern_ecu_blind_audit_test`), and enriched starter classifier rules.
- Full 5-tab OBD-II, UDS, and motorcycle diagnostic cockpit (54+ Mode `01` PIDs, Mode `02` freeze frames, Mode `06` monitors, Mode `0A` permanent DTCs, UDS `0x10`/`0x22`/`0x31`/`0x2F` active tests, Honda 4-pin / Euro 5 CAN / Yamaha / Kawasaki KDS / Suzuki SDS, and interactive Modified DLC Pinout Guide).

### Improved

- Upgraded `Surface3DView` with camera-aware 3D wireframe bounding cage, adaptive non-overlapping X/Y/Z axis ticks, right-edge calibrated color-scale legend, Lambertian quad shading, drop skirts, base ROM ghost overlay, bottom Z-Scale bar, and flex-wrapping toolbar controls.
- Replaced flat Map Explorer text entries with recursive collapsible category folders (`Auto-Detected / 2D Tables`, `Auto-Detected / 1D Curves`, plus definition folders), `Expand all` / `Collapse all` actions, and clean parameter naming.
- Added a 32-entry LRU matrix extraction cache (`useMapStore`) and cross-window state synchronization (`workspaceSyncBridge`) with full 10-tool desktop pop-out support and bottom `StatusBar` restore chips for minimized in-app tool windows.

### Fixed

- Fixed Windows WebView2 freeze / Not Responding and missing titlebar dragging when opening standalone tool windows (`How to Use kiviTuner` and siblings) by dispatching `open_tool_window` asynchronously, removing DWM blur layers, eliminating broadcast self-echoes, lazy-loading tool bundles, and adding native `startDragging` coverage across the entire titlebar.
- Fixed crushed/overlapping 2D table columns inside `Split 3D` and `Split Wave` layouts by enforcing readable minimum cell widths and horizontal table scrolling.
- Fixed runaway vertical canvas height / infinite downward scrollbar growth in `Full 3D` mode by isolating `ResizeObserver` to a fixed-bounds host container.
- Fixed layout state lock preventing switching directly back from `Split 3D` or `Full 3D` to `2D Table`.

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
