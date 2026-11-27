export interface ShortcutItem {
  id: string;
  category:
    | 'Navigation & Workspace'
    | '2D Heatmap Matrix'
    | '3D Surface View'
    | '2D Waveform'
    | 'Acquisition';
  keys: string[];
  action: string;
  description: string;
}

export type CategoryFilter =
  | 'All'
  | 'Navigation & Workspace'
  | '2D Heatmap Matrix'
  | '3D Surface View'
  | '2D Waveform'
  | 'Acquisition';

export const SHORTCUTS_REGISTRY: ShortcutItem[] = [
  // 1. Navigation & Workspace
  {
    id: 'nav-open-rom',
    category: 'Navigation & Workspace',
    keys: ['Ctrl', 'O'],
    action: 'Open Binary ROM',
    description: 'Browse and load binary calibration file (.bin, .rom)',
  },
  {
    id: 'nav-save-rom',
    category: 'Navigation & Workspace',
    keys: ['Ctrl', 'S'],
    action: 'Save Modified ROM',
    description: 'Export and download modified binary calibration image',
  },
  {
    id: 'nav-save-as',
    category: 'Navigation & Workspace',
    keys: ['Ctrl', 'Shift', 'S'],
    action: 'Save As...',
    description: 'Save calibration under a new filename or revision',
  },
  {
    id: 'nav-load-xdf',
    category: 'Navigation & Workspace',
    keys: ['Ctrl', 'D'],
    action: 'Load XDF Definition',
    description: 'Import definition file with table, axis, and scalar addresses',
  },
  {
    id: 'nav-open-diff',
    category: 'Navigation & Workspace',
    keys: ['Ctrl', 'B'],
    action: 'Binary Diff Engine',
    description: 'Open diff modal to compare against base or reference ROM',
  },
  {
    id: 'nav-open-scan',
    category: 'Navigation & Workspace',
    keys: ['Ctrl', 'F'],
    action: 'Heuristic Map Scanner',
    description: 'Run automatic table and axis detector across binary data',
  },
  {
    id: 'nav-split-view',
    category: 'Navigation & Workspace',
    keys: ['Ctrl', '\\'],
    action: 'Toggle Split 2D+3D View',
    description: 'Switch between single table, split 2D+3D, or full surface view',
  },
  {
    id: 'nav-shortcuts',
    category: 'Navigation & Workspace',
    keys: ['F1'],
    action: 'Keyboard Shortcuts',
    description: 'Open this keyboard shortcuts quick reference modal',
  },
  {
    id: 'nav-escape',
    category: 'Navigation & Workspace',
    keys: ['Esc'],
    action: 'Close / Cancel',
    description: 'Close active modal, flyout menu, or clear cell selection',
  },

  // 2. 2D Heatmap Matrix
  {
    id: 'mat-increment',
    category: '2D Heatmap Matrix',
    keys: ['+'],
    action: 'Increment Value',
    description: 'Add 1 to all cells within the current selection',
  },
  {
    id: 'mat-decrement',
    category: '2D Heatmap Matrix',
    keys: ['-'],
    action: 'Decrement Value',
    description: 'Subtract 1 from all cells within the current selection',
  },
  {
    id: 'mat-scale',
    category: '2D Heatmap Matrix',
    keys: ['%'],
    action: 'Scale Percentage',
    description: 'Multiply selected cells by a percentage factor',
  },
  {
    id: 'mat-set',
    category: '2D Heatmap Matrix',
    keys: ['='],
    action: 'Set Constant',
    description: 'Assign a fixed value directly to all selected cells',
  },
  {
    id: 'mat-smooth',
    category: '2D Heatmap Matrix',
    keys: ['S'],
    action: 'Smooth 3x3',
    description: 'Apply Gaussian smoothing filter across selected region',
  },
  {
    id: 'mat-interp-2d',
    category: '2D Heatmap Matrix',
    keys: ['I'],
    action: 'Bilinear 2D Interpolation',
    description: 'Interpolate cell values bilinearly between selection corners',
  },
  {
    id: 'mat-interp-h',
    category: '2D Heatmap Matrix',
    keys: ['H'],
    action: 'Horizontal Interpolate',
    description: 'Interpolate each row linearly between left and right bounds',
  },
  {
    id: 'mat-interp-v',
    category: '2D Heatmap Matrix',
    keys: ['V'],
    action: 'Vertical Interpolate',
    description: 'Interpolate each column linearly between top and bottom bounds',
  },
  {
    id: 'mat-select-all',
    category: '2D Heatmap Matrix',
    keys: ['Ctrl', 'A'],
    action: 'Select All Cells',
    description: 'Select the entire active 2D table grid',
  },
  {
    id: 'mat-copy',
    category: '2D Heatmap Matrix',
    keys: ['Ctrl', 'C'],
    action: 'Copy Selection',
    description: 'Copy selected cell matrix to clipboard buffer',
  },
  {
    id: 'mat-paste',
    category: '2D Heatmap Matrix',
    keys: ['Ctrl', 'V'],
    action: 'Paste Cells',
    description: 'Paste clipboard values starting at active cursor',
  },
  {
    id: 'mat-arrows',
    category: '2D Heatmap Matrix',
    keys: ['Arrow Keys'],
    action: 'Navigate Cursor',
    description: 'Move active cell selection up, down, left, or right',
  },
  {
    id: 'mat-shift-arrows',
    category: '2D Heatmap Matrix',
    keys: ['Shift', 'Arrow Keys'],
    action: 'Extend Selection',
    description: 'Expand or contract rectangular multi-cell selection block',
  },

  // 3. 3D Surface View
  {
    id: 'sur-rotate',
    category: '3D Surface View',
    keys: ['Left Click', 'Drag'],
    action: 'Rotate Camera',
    description: 'Orbit camera around calibration mesh center (Yaw & Pitch)',
  },
  {
    id: 'sur-pan',
    category: '3D Surface View',
    keys: ['Right Click', 'Drag'],
    action: 'Pan View',
    description: 'Translate camera perspective across X and Y axes',
  },
  {
    id: 'sur-zoom',
    category: '3D Surface View',
    keys: ['Scroll Wheel'],
    action: 'Zoom In / Out',
    description: 'Smoothly zoom camera distance toward mesh focus',
  },
  {
    id: 'sur-reset',
    category: '3D Surface View',
    keys: ['R'],
    action: 'Reset Camera Preset',
    description: 'Restore default 3D isometric calibration perspective',
  },
  {
    id: 'sur-top',
    category: '3D Surface View',
    keys: ['T'],
    action: 'Top-Down Preset',
    description: 'Switch camera to orthographic 2D contour overhead angle',
  },
  {
    id: 'sur-side',
    category: '3D Surface View',
    keys: ['S'],
    action: 'Side Profile Preset',
    description: 'View calibration surface curvature along the RPM axis',
  },
  {
    id: 'sur-front',
    category: '3D Surface View',
    keys: ['F'],
    action: 'Front Profile Preset',
    description: 'View calibration surface curvature along the load/TPS axis',
  },

  // 4. 2D Waveform
  {
    id: 'wav-pitch-dec',
    category: '2D Waveform',
    keys: ['['],
    action: 'Decrease Pitch',
    description: 'Decrease bytes per row scanline in waveform viewer',
  },
  {
    id: 'wav-pitch-inc',
    category: '2D Waveform',
    keys: [']'],
    action: 'Increase Pitch',
    description: 'Increase bytes per row scanline in waveform viewer',
  },
  {
    id: 'wav-select-range',
    category: '2D Waveform',
    keys: ['Shift', 'Drag'],
    action: 'Select Binary Range',
    description: 'Highlight address span for map candidate extraction',
  },
  {
    id: 'wav-zoom',
    category: '2D Waveform',
    keys: ['Ctrl', 'Scroll'],
    action: 'Waveform Zoom',
    description: 'Zoom waveform byte amplitude resolution',
  },
  {
    id: 'wav-inspect',
    category: '2D Waveform',
    keys: ['Double Click'],
    action: 'Inspect Address',
    description: 'Pin binary offset and inspect hex/dec values in status bar',
  },

  // 5. Acquisition & ECU
  {
    id: 'acq-toggle-record',
    category: 'Acquisition',
    keys: ['F9'],
    action: 'Toggle Datalog Recording',
    description: 'Start or stop high-speed ECU datalog recording session',
  },
];
