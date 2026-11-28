export type RightDockPanel = 'inspector' | 'scanner';
export type RightDockAnimState = 'open' | 'closing' | 'swap';

/** Toggles a panel inside the shared right-hand dock slot. */
export function toggleRightDockPanel(
  current: RightDockPanel | null,
  target: RightDockPanel
): RightDockPanel | null {
  return current === target ? null : target;
}

/** Resolves animation mode so direct panel swaps never fight a simultaneous close animation. */
export function resolveRightDockAnimState(
  presenceState: 'open' | 'closing',
  wasSwapped: boolean
): RightDockAnimState {
  if (presenceState === 'closing') return 'closing';
  return wasSwapped ? 'swap' : 'open';
}
