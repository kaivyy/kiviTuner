# Getting Started with kiviTuner

This guide walks through a complete calibration workflow in kiviTuner: opening a ROM image, loading or discovering map definitions, editing 2D/1D/3D and scalar tables safely against a stock baseline, inspecting binary differences, verifying checksums, and saving your modified image.

## 1. Install or Launch kiviTuner

Download the native desktop installer for Windows (`.exe` / `.msi`), Linux (`.deb` / `.rpm` / `.AppImage`), or macOS (`.dmg`) from [GitHub Releases](https://github.com/kaivyy/kiviTuner/releases), or run the browser preview locally with `npm ci && npm run dev`.

Native hardware adapters (SAE J2534 with automatic Win64-to-Win32 bridging, Tactrix OpenPort 2.0 USB-CDC, SocketCAN with software ISO 15765-2 segmentation, ELM327 USB/Bluetooth SPP/Wi-Fi TCP, Raw K-Line USB FTDI/CH340, DoIP Ethernet, Ostrich 2.0, Speeduino/MegaSquirt INI serial) and `blind_v2` multi-brand scanning run in the desktop application. The browser preview supports offline `.bin`/`.rom`/`.hex`/`.s19` editing, Keihin 8-bit checksum repair, heuristic scanning, and project export.

## 2. Open a ROM and Load Map Definitions

1. Press `Ctrl+O` or open `File -> Open ROM...` (on macOS, use the global menu bar next to the Apple icon) to load a firmware image (`.bin`, `.rom`, Intel `.hex`, Motorola `.s19`, or `.vbf`).
2. Press `Ctrl+D` or open `Definition (XDF) -> Load XDF...` to import an existing definition file (`.xdf`, `.a2l`, `.kp`, `.ini`, or `.kivi.json`).
3. Confirm the checksum indicator in the top menu bar reads green before making calibration edits.
4. Open the Command Palette (`Ctrl+K`) at any time to run menu actions without searching through submenus. Any toolbar or tab strip with overflow buttons (`WorkspaceTabBar`, view modes, toolkit tools, and tutorial steps) supports mouse-wheel scrolling, slim horizontal scrollbars, and borderless chevron steppers, and individual studio tabs can be popped out into floating windows or standalone desktop OS windows (`?window=tab`) with live ROM sync.

When a ROM loads, kiviTuner keeps an immutable snapshot of the original bytes (`original_memory` on desktop, `originalRomData` in browser mode) separate from the editable working buffer.

## 3. Browse and Organize Maps in Map Explorer

Use the `Map Explorer` sidebar on the left to locate tables inside your loaded definition or discovery scan:

| Control | Purpose |
|---|---|
| Collapsible Category Folders | Organizes parameters into nested `/` folders (`Fuel / Main Injection`, `Auto-Detected / 2D Tables`, etc.) with `Expand all` and `Collapse all` buttons |
| Search box | Filters tables by parameter name or category and auto-expands matching folders |
| `All` / `2D` / `1D` / `1x1` pills | Narrows the tree to 2D matrices, 1D curves, or single-value scalars/flags |
| `+` button or `Definition (XDF) -> Create New...` | Creates a `Table 2D/3D`, `Axis 1D`, `1x1 Scalar`, or `Bitmask Flag` manually |
| `Candidate Regions` drawer | Lists heuristic and structural map candidates with jump-to-hex and one-click promote |

Right-click or click the Split icon on any map in `Map Explorer` (or toggle Split View in `WorkspaceTabBar`) to inspect two independent maps side by side, or click the Detach icon on a tab to pop it out into a floating window or standalone OS window. Drag the vertical divider edge on `Map Explorer` or the right dock (`Inspector` / `Discovery Scanner`) to resize panels smoothly, drag past the narrow threshold to auto-collapse, or double-click any divider to restore its default width.

## 4. Discover Maps Without a Definition

If you open an undefined binary image, combine `Discovery Scanner`, `Hexdump Pro`, and `KiviWaveform`:

1. Open `Discovery Scanner` from the left `ActivityRail` (`Ctrl+F`) to run `blind_v2` structural 2D/1D discovery (desktop), Heuristic scan, or the 16-bit Window scanner (`Discovery Scanner` and `Inspector` share the unified resizable right dock with smooth zero-overlap switching). Candidate addresses populate `Potential Maps`.
2. Switch to the `Hex` tab from `WorkspaceTabBar` or `ActivityRail` (or detach it into its own window).
3. Toggle `8b` / `16b` word width and `Hex` / `Dec` display, double-click or press `Enter` on any hex cell for inline byte editing without column width shifts, and adjust column alignment with `-` / `+` (or `W` / `M`) until repeating row patterns line up vertically.
4. Enable `Diff` to highlight bytes changed relative to stock in green, and press `F` / `Shift+F` to jump across amber `Potential Maps` blocks.
5. Drag a byte range in the hex view (or click a candidate block) and click `Promote Map` to turn that region into an editable table definition. Check the bottom Data Inspector bar to read `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, and `f32` values in both Big Endian and Little Endian at the cursor.

## 5. Edit Tables, 1D Curves, 3D Surfaces, and Scalars

### 2D Matrices and 1D / 3D Views

1. Select any 2D map or `1xN` / `Nx1` curve from `Map Explorer`.
2. Drag across cells (or use `Arrows` / `Shift+Arrows`) to select a working range.
3. Modify values inline by double-clicking any cell (table column widths stay locked during editing), or with `Enter`, `+` / `-` (step increment), `=` (set constant), or `%` / `*` (percentage/factor scale), or use `Edit & Table Math` in the top menu bar.
4. Smooth transitions with `S` (3x3 Gaussian smooth) or fill between boundary cells with `I` (bilinear), `H` (horizontal), or `V` (vertical).
5. Switch display modes between `Engineering`, `Raw Hex`, `Raw Dec`, `Delta`, and `% Delta` against the immutable stock baseline.
6. Press `Ctrl+\` (or choose `Split 3D` / `Full 3D` in the workspace toolbar) to inspect a calibrated 3D surface mesh with floating click-to-expand Camera angle menu (`Iso` / `Top` / `Front` / `Side` / `Reset`), Layers popover (`Mesh` / `Solid` / `Wire`, `Cage`, `Ghost`, `Color Scale`, `Palette`, `Z-Scale`), vertical zoom capsule, and compact cell telemetry pill; drag split dividers to resize or click `2D Table` to switch right back. Maps sized `1xN` or `Nx1` automatically display the draggable `CurveProfileChart` with a dashed stock reference curve.

### 1x1 Scalars and Bitmask Flags

Select a `1x1` scalar or bitmask entry to open `ScalarFlagEditor`, which keeps calibrated Engineering units, Raw Hex, and Binary bit switches (`Bit 0..Bit 7` or `16-bit`) synchronized. Click `Grid View` in the upper-right corner of the card if you prefer editing in single-cell table mode.

### Undo, Redo, and Revert to Stock

Use `Ctrl+Z` / `Ctrl+Y` to step through edit history. Press `Backspace` or `Delete` inside a selection to revert selected cells to their original stock values, or click `Revert Map to Stock` inside the `Inspector` sidebar to reset an entire map cleanly.

## 6. Inspect Diffs, Repair Checksums, and Save

1. Open `InspectorSidebar` and review `Change Ledger (vs Original)` to inspect clustered modified byte ranges; click any ledger entry to jump straight to that address in Hex.
2. Press `Ctrl+B` or open `Reverse Engineer -> Binary Diff Engine -> Open Diff Modal` to compare modified vs reference images and extract changed blocks as new definitions.
3. Verify and repair integrity blocks from the top status pill or `Reverse Engineer -> Checksum & Integrity -> Verify Checksum & Auto-Fix Checksum` (see [Checksum Engines & JSON Plugins](checksum-plugins.md)).
4. Press `Ctrl+Shift+S` (`File -> Save As...`) to write your tuned file to a separate path so the original stock backup stays untouched.

## 7. Next Steps for Live Telemetry, Diagnostics & Hardware Flashing

1. Open `Acquisition & ECU -> Live Telemetry & Auto-Tune -> Telemetry Dashboard (20Hz)` (press `F9` to record datalogs or load a CSV replay log, or pick `obd2:` serial / Bluetooth SPP `/ obd2_wifi:192.168.0.10:35000` with auto-baud probing).
2. Open `Acquisition & ECU -> Flash ECU` and attach an interface (auto-detect or manual ELM327 USB/BT/Wi-Fi, Raw K-Line FTDI, OpenPort 2.0, SAE J2534, DoIP, or SocketCAN), then use **Probe Clone** on low-cost ELM327 dongles to verify `ATSH`/`ATCAF0`/`ATTP5` support.
3. Inside the **Diagnostics** tab, switch across the 5 sub-tabs:
   - **Overview & DTCs**: Scan multi-ECU addresses (`0x7E0..0x7E7` + 29-bit), read/clear DTCs with offline lookup descriptions, inspect I/M Readiness + Mode `0A` Permanent DTCs + Mode `02` Freeze Frames, or send UDS `0x11` ECU resets.
   - **Live OBD PIDs**: Poll 54+ supported SAE Mode `01` PIDs and Mode `06` on-board monitor tests.
   - **UDS Service & Active Tests**: Switch `0x10` sessions, read `0x22` DIDs, execute `0x31` service resets/routines, or run `0x2F` actuator tests (always click **Return Control to ECU (0x00)** when done).
   - **Motorcycle Lab**: Connect to Honda 4-pin PGM-FI K-Line (`0x72` Table `0x11`/`0x73`/`0x74`/`0x60`), Euro 5 CAN, Yamaha 3/4-pin K-Line, Kawasaki KDS, or Suzuki SDS using the built-in **Modified DLC Pinout Guide**.
   - **Raw Terminal**: Send direct `AT`, OBD, or K-Line hex commands interactively.
4. Before writing any tuned image to physical hardware, install the required interface driver from [Drivers and Hardware Interfaces](../hardware/drivers-and-interfaces.md) and complete every item in [Hardware Bench Validation Guide](hardware-bench-validation.md). Remember that ELM327 adapters are read/diagnostic-only and cannot flash ECU ROMs.

## 8. Keep kiviTuner Up to Date

1. Desktop builds check GitHub Releases asynchronously shortly after startup and display an update pill in the top menu bar when a newer tag is available.
2. Open `Help -> Check for Updates...` at any time to inspect release notes, verify package integrity against `SHA256SUMS.txt`, download the platform installer, and restart straight into the updated release.
