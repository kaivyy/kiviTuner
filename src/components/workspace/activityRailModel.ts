/**
 * Pure single-active focus resolution for ActivityRail and MobileBottomNav.
 * Keeps at most one primary navigation button visibly active at any time so
 * previously clicked tool/sidebar buttons never look stuck highlighted.
 */

export type ActivityRailItemId =
  | 'maps'
  | 'scanner'
  | 'hex'
  | 'waveform'
  | 'workbench'
  | 'flash'
  | 'diff'
  | 'inspector'
  | 'palette';

export interface ActiveRailStateInput {
  focusedItem: ActivityRailItemId | null;
  showMapTree: boolean;
  isScannerOpen: boolean;
  showInspector: boolean;
  activeTabKind: string | null;
  isWorkbenchOpen: boolean;
  isFlashPanelOpen: boolean;
  isDiffModalOpen: boolean;
  isCommandPaletteOpen: boolean;
}

export function resolveActiveRailItem(input: ActiveRailStateInput): ActivityRailItemId | null {
  const {
    focusedItem,
    showMapTree,
    isScannerOpen,
    showInspector,
    activeTabKind,
    isWorkbenchOpen,
    isFlashPanelOpen,
    isDiffModalOpen,
    isCommandPaletteOpen,
  } = input;

  if (isCommandPaletteOpen) return 'palette';

  if (focusedItem === 'workbench' && isWorkbenchOpen) return 'workbench';
  if (focusedItem === 'flash' && isFlashPanelOpen) return 'flash';
  if (focusedItem === 'diff' && isDiffModalOpen) return 'diff';
  if (isWorkbenchOpen) return 'workbench';
  if (isFlashPanelOpen) return 'flash';
  if (isDiffModalOpen) return 'diff';

  if (focusedItem === 'scanner' && isScannerOpen) return 'scanner';
  if (focusedItem === 'inspector' && showInspector) return 'inspector';
  if (focusedItem === 'maps' && showMapTree) return 'maps';
  if (focusedItem === 'hex' && activeTabKind === 'hex') return 'hex';
  if (focusedItem === 'waveform' && activeTabKind === 'waveform') return 'waveform';

  if (activeTabKind === 'hex') return 'hex';
  if (activeTabKind === 'waveform') return 'waveform';

  return null;
}
