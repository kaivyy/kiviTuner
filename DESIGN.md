# kiviTuner Design System & Workspace Architecture

## 1. Visual Dials & Aesthetic Direction

- **Dial:** `ENERGY 1 / RHYTHM 2 / MOTION 1` (calm technical instrument, medium structured density, crisp instant state feedback with zero decorative bounce).
- **Typography:** Global monospace workbench font stack (`JetBrains Mono`, `Fira Code`, `Cascadia Mono`, `ui-monospace`, `monospace`) with `tabular-nums` aligned numerics across menus, tables, hex, and calculators.
- **Primary Themes:**
  - **Carbon Instrument (Dark, `data-theme="dark"`):** High-legibility technical surface tuned for dark workshops and long calibration sessions.
  - **Lab Alabaster (Light, `data-theme="light"`):** Clean high-contrast daylight workbench without washed-out grays or harsh glare.

| Semantic Token | CSS Variable | Carbon Dark | Lab Light | Min AA Ratio Checked |
|---|---|---|---|---|
| Background | `--kivi-bg` | `#0A0D12` | `#F4F6F9` | `17.76:1` / `16.49:1` (`ink` on `bg`) |
| Surface | `--kivi-surface` | `#11161F` | `#FFFFFF` | `16.55:1` / `17.85:1` (`ink` on `surface`) |
| Elevated | `--kivi-elevated` | `#181F2B` | `#E9EEF5` | `15.10:1` / `15.31:1` (`ink` on `elevated`) |
| Border / Strong | `--kivi-border` / `--kivi-border-strong` | `#243044` / `#334155` | `#CBD5E1` / `#64748B` | Explicit structural hairlines |
| Primary Ink | `--kivi-ink` | `#F1F5F9` | `#0F172A` | AAA on all 3 surface levels |
| Secondary / Muted | `--kivi-secondary` / `--kivi-muted` | `#B6C2D4` / `#94A3B8` | `#334155` / `#475569` | `>= 6.45:1` dark, `>= 6.50:1` light |
| Accent / Accent Fg | `--kivi-accent` / `--kivi-accent-fg` | `#38BDF8` / `#06131F` | `#0369A1` / `#FFFFFF` | `8.75:1` dark, `5.93:1` light |
| OK / Warn / Danger | `--kivi-ok` / `--kivi-warn` / `--kivi-danger` | `#4ADE80` / `#FBBF24` / `#F87171` | `#166534` / `#92400E` / `#B91C1C` | `>= 6.47:1` on surface in both themes |

## 2. Single-Owner Workspace Shell (Zero Control Duplication)

1. **Top Menu Bar & Window Chrome (`KiviMenuBar` & `WindowTrafficLights`):** Frameless native window chrome with integrated macOS-style traffic light window controls (`Close`, `Minimize`, `Maximize / Fullscreen`), draggable titlebar region (`data-tauri-drag-region`), full 7-menu bar (`File`, `Edit`, `Definition`, `View`, `Reverse`, `ECU`, `Help`), real `Recent ROMs` history, dynamic checksum status/repair indicator, delta counter, and active ROM readout. Every action maps to a concrete handler in `App.tsx` / `HeatmapTableHandle`.
2. **Workspace Project & View Tab Strip (`WorkspaceTabBar`):** Switches open ROM documents, manages open table/tool tabs with pinning and Split View toggle, plus `Ctrl+K` Command Palette trigger (no duplicate Hex/Waveform/Scanner shortcuts).
3. **Left Navigation Rail (`ActivityRail` & `MobileBottomNav`):** Owns view-mode icons (`Map Explorer`, `Inspector`, `Discovery Scanner`, `Hex`, `Waveform`, `Workbench`, `Flash`, `Diff`, `Command Palette`, and single-owner `Theme` toggle).
4. **Side Panels (`MapTreeSidebar`, `InspectorSidebar`, `ScannerDrawer`):**
   - `MapTreeSidebar` focuses solely on browsing/filtering categories, `+` creating new maps (`MapPropertiesModal`), editing properties, deleting definitions, and opening maps in Split Pane.
   - `InspectorSidebar` displays ROM SHA-256/undo metadata, waveform cursor selection + extract, multi-OEM checksum status + repair, and selected map actions (`Edit Map Properties`, `View in Hex`, `Revert Map to Stock`).
   - `ScannerDrawer` unifies **Blind V2 (Desktop)**, **Heuristics**, and **16-bit Window** discovery in one panel.
5. **Viewport (`WorkspaceViewport`):** Clean layout switcher (`2D Table`, `Split 3D`, `Split Wave`, `Full Waveform`, `Full 3D`) plus primary/split panes (respecting active preset without locking `map-table`).
6. **Status Bar (`StatusBar`):** Read-only engine/ECU connection/map/ROM/checksum/undo-redo telemetry without duplicate toggles.

## 3. Editor Capabilities

- **True Stock Baseline Isolation:** Backend (`original_memory`) and store (`originalRomData`) snapshot the freshly loaded ROM immutably so delta views (`Delta vs Base`, `% Delta`) and `Revert to Stock` remain accurate across arbitrary edits, Undo/Redo cycles, and map switches.
- **Full Cell & Stride Support:** `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `f32` (big/little endian, row/column major, `major_stride_bits` / `minor_stride_bits`), affine equation solver/evaluator, and XDF/A2L engineering axis breakpoints.
- **1D Curve & 2D Slice Editor (`CurveProfileChart`):** Renders automatically on `1xN` / `Nx1` maps and toggleable on 2D rows with draggable vertices and dashed stock reference curve.
- **Virtualized Hex Inspector (`HexdumpView`):** 4 KiB LRU page cache with monotonic sequence guards, byte range selection (`Shift+Click`), `Create Map` extraction, and multi-format Data Inspector (`u8`/`i8`/`u16`/`i16`/`u32`/`i32`/`f32`/`bin`/`chr`, BE/LE toggle).

## 4. Accessibility & Concurrency Guarantees

- Zero bare `outline-none`; global `:focus-visible` outlines (`2px solid var(--kivi-accent)`).
- Modal dialogs enforce `role="dialog"`, `aria-modal="true"`, and `Escape` dismissal.
- Waveform keyboard shortcuts (`B`, `E`, `[`, `]`) are scoped cleanly to the focused canvas container with modifier guards so `Ctrl+B` never toggles bit depth.
- All store writes (`updateMatrix`, `patchActiveMapCells`, `patchRomBytes`, `revertMapToOriginal`, `undoEdit`, `redoEdit`, `fixChecksum`) serialize through an ordered promise queue guarded by monotonic document/extract/checksum sequence counters.
