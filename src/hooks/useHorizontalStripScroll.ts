import { useCallback, useEffect, useRef, useState } from 'react';

export interface HorizontalScrollState {
  hasOverflow: boolean;
  canLeft: boolean;
  canRight: boolean;
}

const OVERFLOW_TOLERANCE_PX = 2;

/** Computes horizontal overflow and directional scrollability within a subpixel tolerance. */
export function computeHorizontalScrollState(
  scrollLeft: number,
  clientWidth: number,
  scrollWidth: number
): HorizontalScrollState {
  const safeClientW = Math.max(0, clientWidth);
  const safeScrollW = Math.max(0, scrollWidth);
  const safeLeft = Math.max(0, scrollLeft);
  const hasOverflow = safeScrollW > safeClientW + OVERFLOW_TOLERANCE_PX;
  const canLeft = hasOverflow && safeLeft > OVERFLOW_TOLERANCE_PX;
  const canRight = hasOverflow && safeLeft + safeClientW < safeScrollW - OVERFLOW_TOLERANCE_PX;
  return { hasOverflow, canLeft, canRight };
}

/**
 * Tracks horizontal overflow for a strip container, observes layout/content
 * resizes, and translates vertical mouse-wheel deltas into horizontal scroll.
 */
export function useHorizontalStripScroll<T extends HTMLElement = HTMLDivElement>(depKey: string) {
  const ref = useRef<T | null>(null);
  const [scrollState, setScrollState] = useState<HorizontalScrollState>({
    hasOverflow: false,
    canLeft: false,
    canRight: false,
  });

  const refresh = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const next = computeHorizontalScrollState(el.scrollLeft, el.clientWidth, el.scrollWidth);
    setScrollState((prev) =>
      prev.hasOverflow === next.hasOverflow &&
      prev.canLeft === next.canLeft &&
      prev.canRight === next.canRight
        ? prev
        : next
    );
  }, []);

  useEffect(() => {
    refresh();
    const rafId =
      typeof window !== 'undefined' && typeof window.requestAnimationFrame === 'function'
        ? window.requestAnimationFrame(refresh)
        : null;

    const el = ref.current;
    const ro =
      el && typeof ResizeObserver !== 'undefined' ? new ResizeObserver(refresh) : null;
    if (el && ro) {
      ro.observe(el);
      if (el.firstElementChild) {
        ro.observe(el.firstElementChild);
      }
    }

    const onWindowResize = () => refresh();
    if (typeof window !== 'undefined') {
      window.addEventListener('resize', onWindowResize);
    }

    return () => {
      if (rafId !== null && typeof window !== 'undefined' && typeof window.cancelAnimationFrame === 'function') {
        window.cancelAnimationFrame(rafId);
      }
      ro?.disconnect();
      if (typeof window !== 'undefined') {
        window.removeEventListener('resize', onWindowResize);
      }
    };
  }, [refresh, depKey]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (el.scrollWidth <= el.clientWidth + OVERFLOW_TOLERANCE_PX) return;
      if (Math.abs(e.deltaY) > Math.abs(e.deltaX) && e.deltaY !== 0) {
        e.preventDefault();
        el.scrollLeft += e.deltaY;
        refresh();
      }
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => {
      el.removeEventListener('wheel', onWheel);
    };
  }, [refresh, depKey]);

  const scrollByDelta = useCallback(
    (delta: number) => {
      const el = ref.current;
      if (!el) return;
      if (typeof el.scrollBy === 'function') {
        el.scrollBy({ left: delta, behavior: 'smooth' });
      } else {
        el.scrollLeft += delta;
      }
      refresh();
    },
    [refresh]
  );

  return { ref, refresh, scrollByDelta, ...scrollState };
}
