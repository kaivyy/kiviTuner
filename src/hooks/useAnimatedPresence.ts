import { useEffect, useState } from 'react';

export type PresenceState = 'open' | 'closing';

export function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return false;
  }
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/**
 * Keeps an overlay, drawer, or modal mounted briefly during its CSS exit
 * transition so both open and close feel smooth without retaining idle DOM.
 */
export function useAnimatedPresence(
  isOpen: boolean,
  exitMs = 120
): { mounted: boolean; state: PresenceState } {
  const [mounted, setMounted] = useState(isOpen);
  const [state, setState] = useState<PresenceState>(isOpen ? 'open' : 'closing');

  useEffect(() => {
    if (isOpen) {
      setMounted(true);
      setState('open');
      return;
    }
    if (!mounted) return;
    if (prefersReducedMotion() || exitMs <= 0) {
      setState('closing');
      setMounted(false);
      return;
    }
    setState('closing');
    const timer = window.setTimeout(() => {
      setMounted(false);
    }, exitMs);
    return () => window.clearTimeout(timer);
  }, [isOpen, exitMs, mounted]);

  return { mounted, state };
}
