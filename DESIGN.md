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

1. **Top Menu Bar & Window Chrome (`KiviMenuBar`, `WindowTrafficLights` & `native_menu.rs`):** Fixed `32px` (`h-8`) borderless frameless chrome with borderless macOS-style traffic lights (`Close`, `Minimize`, `Maximize / Fullscreen`), native `startDragging` regions, borderless dropdowns and status pills, plus automatic delegation of the 7-menu hierarchy to the native macOS global top bar (`native_menu.rs` + `nativeMenuBridge.ts`) when running on macOS desktop. Outer desktop OS windows and floating tool windows (`FloatingWindow`, `CommandPalette`) use rounded outer corners and soft shadows, while docked workspace drawers and inner panes keep clean flat edges.
2. **Workspace Project & View Tab Strip (`WorkspaceTabBar`):** Switches open ROM documents, scrolls horizontally via wheel/scrollbar/arrows without vertical height jump when opening `Full Waveform` from an empty workspace, manages open table/tool tabs with LRU matrix caching (`useMapStore`), pinning, Split View toggle, and `Ctrl+K` Command Palette trigger.
3. **Left Navigation Rail (`ActivityRail` & `MobileBottomNav`):** Owns view-mode icons (`Map Explorer`, `Discovery Scanner`, `Hex`, `Waveform`, `Workbench`, `Flash`, `Diff`, `Inspector`, `Command Palette`, and single-owner `Theme` toggle) with an exclusive single-active focus resolver (`activityRailModel.ts`) so moving across tools never leaves stale active button backgrounds.
4. **Side Panels (`MapTreeSidebar`, `InspectorSidebar`, `ScannerDrawer`):**
   - All workspace headers share a unified borderless `36px` (`h-9`) height across desktop and small resizes, with overlay drawers anchored cleanly inside the workspace container so the menubar never covers `Map Explorer` search or right drawers, and `ScannerDrawer` / `InspectorSidebar` toggle mutually exclusively.
   - `MapTreeSidebar` displays a nested collapsible folder hierarchy built from `/` category paths (`Expand all` / `Collapse all`, filter pills, ancestor auto-expansion), `+` creating new maps (`MapPropertiesModal`), editing properties, deleting definitions, and opening maps in Split Pane.
   - `InspectorSidebar` displays ROM SHA-256/undo metadata, waveform cursor selection + extract, multi-OEM checksum status + repair, and selected map actions (`Edit Map Properties`, `View in Hex`, `Revert Map to Stock`).
   - `ScannerDrawer` unifies **2D/1D Structural Scan (Desktop)**, **Heuristics**, and **16-bit Window** discovery in one panel.
5. **Viewport (`WorkspaceViewport`, `Surface3DView`, `FloatingWindow`, `StandaloneWindowHost`):** Fixed `36px` (`h-9`) single-row horizontally scrollable view-preset switcher (`2D Table`, `Split 3D`, `Split Wave`, `Full Waveform`, `Full 3D`), non-crushing scrollable 2D grids (`min-w-[56px]`) with fixed-width Analysis Bar buttons, bounded `Full 3D` canvas host preventing `ResizeObserver` feedback loops, camera-aware 3D calibration cage with color-scale legend, GPU-composited open/close/minimize window transitions (`useAnimatedPresence`), low-RAM WebView2 tuning, and async non-blocking desktop pop-out windows synchronized via `workspaceSyncBridge`.
6. **Status Bar (`StatusBar`):** Engine/ECU connection, map/ROM/checksum/undo-redo telemetry, plus compact restore chips whenever in-app floating windows are minimized (zero stray mid-screen bars).

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
