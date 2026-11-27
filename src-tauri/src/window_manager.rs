//! Multi-window management for Tauri desktop pop-out tool windows.
//!
//! Spawns frameless, OS-resizable secondary windows (`decorations(false)`) with
//! enforced minimum dimensions and macOS-style traffic-light controls rendered
//! inside [`crate::components::layout::WindowTrafficLights`].

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const HARD_MIN_WIDTH: f64 = 320.0;
const HARD_MIN_HEIGHT: f64 = 240.0;

/// Supported popup/tool window identifiers synced with the TypeScript bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolWindowKind {
    Telemetry,
    Flash,
    Toolkit,
    Catalogue,
    Diff,
    Workbench,
    Autotune,
    Guide,
    Shortcuts,
    About,
    MapProps,
}

impl ToolWindowKind {
    /// Canonical query/URL identifier for this tool window.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolWindowKind::Telemetry => "telemetry",
            ToolWindowKind::Flash => "flash",
            ToolWindowKind::Toolkit => "toolkit",
            ToolWindowKind::Catalogue => "catalogue",
            ToolWindowKind::Diff => "diff",
            ToolWindowKind::Workbench => "workbench",
            ToolWindowKind::Autotune => "autotune",
            ToolWindowKind::Guide => "guide",
            ToolWindowKind::Shortcuts => "shortcuts",
            ToolWindowKind::About => "about",
            ToolWindowKind::MapProps => "map-props",
        }
    }

    /// Tauri window label matching capability pattern `kivi-*`.
    pub fn label(self) -> String {
        format!("kivi-{}", self.as_str())
    }

    /// Default minimum inner dimensions `(width, height)` preventing UI overlap.
    pub fn default_min_size(self) -> (f64, f64) {
        match self {
            ToolWindowKind::Telemetry => (380.0, 440.0),
            ToolWindowKind::Flash => (420.0, 460.0),
            ToolWindowKind::Toolkit => (460.0, 420.0),
            ToolWindowKind::Catalogue => (460.0, 420.0),
            ToolWindowKind::Diff | ToolWindowKind::Workbench | ToolWindowKind::Autotune => {
                (520.0, 440.0)
            }
            ToolWindowKind::MapProps => (400.0, 380.0),
            ToolWindowKind::Guide => (460.0, 420.0),
            ToolWindowKind::Shortcuts => (440.0, 400.0),
            ToolWindowKind::About => (360.0, 340.0),
        }
    }
}

/// Request payload sent by the frontend to spawn or focus a tool window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolWindowSpec {
    pub tool_id: ToolWindowKind,
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub min_width: f64,
    pub min_height: f64,
    pub always_on_top: Option<bool>,
}

/// Sanitizes and clamps `(width, height, min_width, min_height)` so callers can
/// never open degenerate, zero-sized, or NaN OS windows.
pub fn normalize_window_dimensions(spec: &ToolWindowSpec) -> (f64, f64, f64, f64) {
    let (default_min_w, default_min_h) = spec.tool_id.default_min_size();

    let min_w = if spec.min_width.is_finite() {
        spec.min_width.max(default_min_w).max(HARD_MIN_WIDTH)
    } else {
        default_min_w
    };
    let min_h = if spec.min_height.is_finite() {
        spec.min_height.max(default_min_h).max(HARD_MIN_HEIGHT)
    } else {
        default_min_h
    };

    let width = if spec.width.is_finite() {
        spec.width.max(min_w)
    } else {
        min_w
    };
    let height = if spec.height.is_finite() {
        spec.height.max(min_h)
    } else {
        min_h
    };

    (width, height, min_w, min_h)
}

/// Opens a separate frameless OS window for `spec.tool_id`, or focuses the
/// existing window if already open.
#[tauri::command]
pub fn open_tool_window(spec: ToolWindowSpec, app: AppHandle) -> Result<String, String> {
    let label = spec.tool_id.label();
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.unminimize();
        let _ = existing.show();
        existing
            .set_focus()
            .map_err(|e| format!("failed to focus window '{label}': {e}"))?;
        return Ok(label);
    }

    let (width, height, min_w, min_h) = normalize_window_dimensions(&spec);
    let route = format!("index.html?window={}", spec.tool_id.as_str());
    let title = if spec.title.trim().is_empty() {
        "kiviTuner".to_string()
    } else {
        spec.title.trim().to_string()
    };

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(route.into()))
        .title(&title)
        .decorations(false)
        .shadow(false)
        .resizable(true)
        .inner_size(width, height)
        .min_inner_size(min_w, min_h)
        .always_on_top(spec.always_on_top.unwrap_or(false))
        .center()
        .build()
        .map_err(|e| format!("failed to create window '{label}': {e}"))?;

    Ok(label)
}

/// Closes a secondary tool window if it is currently open.
#[tauri::command]
pub fn close_tool_window(tool_id: ToolWindowKind, app: AppHandle) -> Result<bool, String> {
    let label = tool_id.label();
    if let Some(win) = app.get_webview_window(&label) {
        win.close()
            .map_err(|e| format!("failed to close window '{label}': {e}"))?;
        return Ok(true);
    }
    Ok(false)
}

/// Updates whether the calling native window stays pinned on top.
#[tauri::command]
pub fn set_current_window_always_on_top(
    always_on_top: bool,
    window: WebviewWindow,
) -> Result<bool, String> {
    window
        .set_always_on_top(always_on_top)
        .map_err(|e| format!("failed to update always-on-top: {e}"))?;
    Ok(always_on_top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_kivi_capability_prefix() {
        assert_eq!(ToolWindowKind::Telemetry.label(), "kivi-telemetry");
        assert_eq!(ToolWindowKind::Guide.label(), "kivi-guide");
        assert_eq!(ToolWindowKind::MapProps.label(), "kivi-map-props");
        assert_eq!(ToolWindowKind::Flash.label(), "kivi-flash");
        assert_eq!(ToolWindowKind::Toolkit.label(), "kivi-toolkit");
    }

    #[test]
    fn normalize_window_dimensions_clamps_small_and_non_finite_values() {
        let tiny = ToolWindowSpec {
            tool_id: ToolWindowKind::Telemetry,
            title: "Telemetry".into(),
            width: 100.0,
            height: 80.0,
            min_width: 50.0,
            min_height: 40.0,
            always_on_top: None,
        };
        assert_eq!(
            normalize_window_dimensions(&tiny),
            (380.0, 440.0, 380.0, 440.0)
        );

        let nan_spec = ToolWindowSpec {
            tool_id: ToolWindowKind::About,
            title: "About".into(),
            width: f64::NAN,
            height: f64::INFINITY,
            min_width: f64::NEG_INFINITY,
            min_height: 200.0,
            always_on_top: Some(true),
        };
        assert_eq!(
            normalize_window_dimensions(&nan_spec),
            (360.0, 340.0, 360.0, 340.0)
        );
    }

    #[test]
    fn normalize_window_dimensions_respects_larger_caller_bounds() {
        let large = ToolWindowSpec {
            tool_id: ToolWindowKind::Diff,
            title: "ROM Diff".into(),
            width: 980.0,
            height: 760.0,
            min_width: 600.0,
            min_height: 480.0,
            always_on_top: None,
        };
        assert_eq!(
            normalize_window_dimensions(&large),
            (980.0, 760.0, 600.0, 480.0)
        );
    }
}
