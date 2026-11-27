//! Native macOS global menu bar builder and canonical menu-action registry.
//!
//! On macOS (`#[cfg(target_os = "macos")]`), attaches the application menu to the
//! OS top bar next to the Apple icon and dispatches leaf action IDs to the active
//! workspace via [`NATIVE_MENU_EVENT`]. Windows, Linux, and Web builds continue to
//! render menus inside [`KiviMenuBar`].

use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Wry,
};

pub const NATIVE_MENU_EVENT: &str = "kivituner:native-menu";

pub const NATIVE_MENU_IDS: &[&str] = &[
    "app.about",
    "app.check-updates",
    "help.guide",
    "help.shortcuts",
    "help.reset-settings",
    "file.open-rom",
    "file.save",
    "file.save-as",
    "file.export-csv",
    "file.export-report",
    "file.close-rom",
    "edit.undo",
    "edit.redo",
    "edit.cell-math",
    "edit.smooth-3x3",
    "edit.interp-2d",
    "edit.revert",
    "def.load-xdf",
    "def.import-json",
    "def.save-json",
    "def.new-table",
    "def.new-scalar",
    "view.table",
    "view.split-3d",
    "view.split-wave",
    "view.waveform",
    "view.full-3d",
    "view.toggle-split",
    "view.command-palette",
    "view.toggle-theme",
    "view.reset-layout",
    "rev.scanner",
    "rev.diff",
    "rev.verify-checksum",
    "rev.fix-checksum",
    "rev.workbench",
    "ecu.connect",
    "ecu.disconnect",
    "ecu.flash",
    "ecu.telemetry",
    "ecu.autotune",
    "ecu.catalogue",
];

/// Returns `true` when `id` corresponds to a frontend-handled menu command.
pub fn is_known_menu_id(id: &str) -> bool {
    NATIVE_MENU_IDS.contains(&id)
}

fn action_item(
    app: &AppHandle,
    id: &str,
    label: &str,
    accel: Option<&str>,
) -> tauri::Result<tauri::menu::MenuItem<Wry>> {
    let mut b = MenuItemBuilder::with_id(id, label);
    if let Some(key) = accel {
        b = b.accelerator(key);
    }
    b.build(app)
}

/// Builds the full macOS native application menu hierarchy for the OS global top bar.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn build_macos_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let app_menu = SubmenuBuilder::new(app, "kiviTuner")
        .item(&action_item(app, "app.about", "About kiviTuner", None)?)
        .item(&action_item(app, "app.check-updates", "Check for Updates...", None)?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&action_item(app, "file.open-rom", "Open ROM...", Some("CmdOrCtrl+O"))?)
        .separator()
        .item(&action_item(app, "file.save", "Save ROM", Some("CmdOrCtrl+S"))?)
        .item(&action_item(app, "file.save-as", "Save ROM As...", Some("CmdOrCtrl+Shift+S"))?)
        .separator()
        .item(&action_item(app, "file.export-csv", "Export CSV / Deltas...", None)?)
        .item(&action_item(app, "file.export-report", "Export Calibration Report...", None)?)
        .separator()
        .item(&action_item(app, "file.close-rom", "Close ROM", None)?)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&action_item(app, "edit.undo", "Undo Calibration Edit", Some("CmdOrCtrl+Z"))?)
        .item(&action_item(app, "edit.redo", "Redo Calibration Edit", Some("CmdOrCtrl+Shift+Z"))?)
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&action_item(app, "edit.cell-math", "Cell Math / Percentage...", None)?)
        .item(&action_item(app, "edit.smooth-3x3", "Smooth 3x3 Selection", None)?)
        .item(&action_item(app, "edit.interp-2d", "Bilinear Interpolate Selection", None)?)
        .item(&action_item(app, "edit.revert", "Revert to Stock Baseline", None)?)
        .build()?;

    let def_menu = SubmenuBuilder::new(app, "Definition")
        .item(&action_item(app, "def.load-xdf", "Load XDF / XML...", None)?)
        .item(&action_item(app, "def.import-json", "Import .kivi.json...", None)?)
        .item(&action_item(app, "def.save-json", "Save .kivi.json...", None)?)
        .separator()
        .item(&action_item(app, "def.new-table", "Create New Table 2D/3D", None)?)
        .item(&action_item(app, "def.new-scalar", "Create 1x1 Scalar", None)?)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .item(&action_item(app, "view.table", "Standard 2D", None)?)
        .item(&action_item(app, "view.split-3d", "Split 2D+3D", Some("CmdOrCtrl+\\"))?)
        .item(&action_item(app, "view.split-wave", "Split 2D+Waveform", None)?)
        .item(&action_item(app, "view.waveform", "Full Waveform", None)?)
        .item(&action_item(app, "view.full-3d", "Full 3D", None)?)
        .separator()
        .item(&action_item(app, "view.toggle-split", "Toggle Split Pane", None)?)
        .item(&action_item(app, "view.command-palette", "Command Palette...", Some("CmdOrCtrl+K"))?)
        .item(&action_item(app, "view.toggle-theme", "Toggle Carbon Dark / Lab Light", None)?)
        .item(&action_item(app, "view.reset-layout", "Reset Workspace Layout", None)?)
        .build()?;

    let reverse_menu = SubmenuBuilder::new(app, "Reverse")
        .item(&action_item(app, "rev.scanner", "Heuristic Auto-Scan", None)?)
        .item(&action_item(app, "rev.diff", "Binary Diff...", Some("CmdOrCtrl+B"))?)
        .separator()
        .item(&action_item(app, "rev.verify-checksum", "Verify Checksum", None)?)
        .item(&action_item(app, "rev.fix-checksum", "Auto-Fix Checksum", None)?)
        .separator()
        .item(&action_item(app, "rev.workbench", "Calibration Workbench", Some("CmdOrCtrl+Shift+W"))?)
        .build()?;

    let ecu_menu = SubmenuBuilder::new(app, "ECU")
        .item(&action_item(app, "ecu.connect", "Connect ECU", None)?)
        .item(&action_item(app, "ecu.disconnect", "Disconnect ECU", None)?)
        .separator()
        .item(&action_item(app, "ecu.flash", "Read / Write Flash...", None)?)
        .item(&action_item(app, "ecu.telemetry", "Telemetry Dashboard (20Hz)", None)?)
        .item(&action_item(app, "ecu.autotune", "AFR Auto-Tune Assistant", None)?)
        .item(&action_item(app, "ecu.catalogue", "Reference Catalogue...", None)?)
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .fullscreen()
        .separator()
        .close_window()
        .build()?;

    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(&action_item(app, "help.guide", "How to Use", None)?)
        .item(&action_item(app, "help.shortcuts", "Keyboard Shortcuts", Some("F1"))?)
        .item(&action_item(app, "app.check-updates", "Check for Updates...", None)?)
        .separator()
        .item(&action_item(app, "help.reset-settings", "Reset UI Settings", None)?)
        .item(&action_item(app, "app.about", "About kiviTuner", None)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[
            &app_menu,
            &file_menu,
            &edit_menu,
            &def_menu,
            &view_menu,
            &reverse_menu,
            &ecu_menu,
            &window_menu,
            &help_menu,
        ])
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn native_menu_ids_are_unique_non_empty_and_recognized() {
        let mut seen = HashSet::new();
        for &id in NATIVE_MENU_IDS {
            assert!(!id.trim().is_empty(), "menu ID must not be empty");
            assert!(seen.insert(id), "duplicate menu ID: {id}");
            assert!(is_known_menu_id(id));
        }
        assert!(!is_known_menu_id("unknown.menu.action"));
    }
}
