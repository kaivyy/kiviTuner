import React, { useEffect } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useHorizontalStripScroll } from '../../hooks/useHorizontalStripScroll';

export interface HorizontalScrollStripProps {
  children: React.ReactNode;
  depKey: string;
  as?: 'div' | 'nav';
  role?: React.AriaRole;
  ariaLabel?: string;
  className?: string;
  stripClassName?: string;
  stripTestId?: string;
  containerTestId?: string;
  activeSelector?: string;
  showArrows?: boolean;
  arrowSize?: 'xs' | 'sm';
  scrollStepPx?: number;
  scrollLeftLabel?: string;
  scrollRightLabel?: string;
}

/**
 * Fixed-height horizontal button/tab strip with thin styled scrollbar,
 * vertical-wheel horizontal scrolling, optional overflow arrow steppers,
 * and strict height locking so surrounding containers never jump vertically.
 */
export const HorizontalScrollStrip: React.FC<HorizontalScrollStripProps> = ({
  children,
  depKey,
  as = 'div',
  role,
  ariaLabel,
  className = 'h-10',
  stripClassName = 'gap-1',
  stripTestId,
  containerTestId,
  activeSelector,
  showArrows = true,
  arrowSize = 'sm',
  scrollStepPx = 160,
  scrollLeftLabel = 'Scroll items left',
  scrollRightLabel = 'Scroll items right',
}) => {
  const { ref, refresh, scrollByDelta, hasOverflow, canLeft, canRight } =
    useHorizontalStripScroll<HTMLDivElement>(depKey);

  useEffect(() => {
    const el = ref.current;
    if (!el || !activeSelector) {
      refresh();
      return;
    }
    const activeEl = el.querySelector<HTMLElement>(activeSelector);
    if (activeEl && typeof activeEl.scrollIntoView === 'function') {
      activeEl.scrollIntoView({ inline: 'nearest', block: 'nearest' });
    }
    refresh();
  }, [activeSelector, depKey, ref, refresh]);

  const btnSizeClass = arrowSize === 'xs' ? 'h-6 w-5' : 'h-7 w-5';
  const iconSizeClass = 'h-3.5 w-3.5';
  const arrowBtnClass = `flex ${btnSizeClass} shrink-0 items-center justify-center rounded border-0 bg-transparent text-kiviMuted transition-colors hover:text-kiviInk active:scale-95 disabled:pointer-events-none disabled:opacity-25`;
  const StripTag = as;

  return (
    <div
      data-testid={containerTestId}
      className={`flex shrink-0 items-center gap-0.5 overflow-hidden ${className}`}
    >
      {showArrows && hasOverflow && (
        <button
          type="button"
          aria-label={scrollLeftLabel}
          disabled={!canLeft}
          onClick={() => scrollByDelta(-scrollStepPx)}
          className={arrowBtnClass}
        >
          <ChevronLeft className={iconSizeClass} />
        </button>
      )}

      <StripTag
        ref={ref as React.RefObject<HTMLDivElement>}
        role={role}
        aria-label={ariaLabel}
        data-testid={stripTestId}
        onScroll={refresh}
        className={`kivi-tab-scrollbar flex h-full min-w-0 flex-1 items-center overflow-x-auto overflow-y-hidden ${stripClassName}`}
      >
        {children}
      </StripTag>

      {showArrows && hasOverflow && (
        <button
          type="button"
          aria-label={scrollRightLabel}
          disabled={!canRight}
          onClick={() => scrollByDelta(scrollStepPx)}
          className={arrowBtnClass}
        >
          <ChevronRight className={iconSizeClass} />
        </button>
      )}
    </div>
  );
};
