import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check } from 'lucide-react';

const FLOATING_ICON_BTN_BASE =
  'flex h-7 w-7 items-center justify-center rounded-md border border-kiviBorder/60 bg-kiviSurface/90 text-kiviSecondary backdrop-blur-sm transition-colors hover:bg-kiviElevated hover:text-kiviInk focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent';

interface AnchoredPopupPos {
  left: number;
  top?: number;
  bottom?: number;
  width: number;
  maxHeight: number;
}

function computeAnchoredPopupPos(
  triggerEl: HTMLElement | null,
  preferredWidth: number,
  estimatedHeight: number,
  placement: 'left' | 'right' = 'left'
): AnchoredPopupPos | null {
  if (typeof window === 'undefined' || !triggerEl) return null;
  const rect = triggerEl.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const margin = 8;
  const gap = 6;

  if (rect.bottom < -4 || rect.top > vh + 4) return null;

  const width = Math.min(vw - margin * 2, preferredWidth);
  const rawLeft =
    placement === 'left'
      ? rect.left - width - gap >= margin
        ? rect.left - width - gap
        : rect.right - width
      : rect.right + gap + width <= vw - margin
        ? rect.right + gap
        : rect.left;
  const left = Math.max(margin, Math.min(rawLeft, vw - margin - width));

  const safeBottom = vh - margin;
  const spaceBelow = safeBottom - rect.top;
  const spaceAbove = rect.bottom - margin;
  const openUpward = spaceBelow < Math.min(estimatedHeight, 190) && spaceAbove > spaceBelow;
  const availVertical = Math.max(120, openUpward ? spaceAbove : spaceBelow);
  const maxHeight = Math.max(96, Math.min(320, availVertical - 24));

  return openUpward
    ? { left, width, bottom: Math.max(margin, vh - rect.bottom), maxHeight }
    : { left, width, top: Math.max(margin, Math.min(rect.top, safeBottom - 120)), maxHeight };
}

export interface ViewportIconToggleProps {
  icon: React.ReactNode;
  ariaLabel: string;
  title?: string;
  active?: boolean;
  onClick: () => void;
  testId?: string;
  className?: string;
}

export const ViewportIconToggle: React.FC<ViewportIconToggleProps> = ({
  icon,
  ariaLabel,
  title,
  active = false,
  onClick,
  testId,
  className = '',
}) => (
  <button
    type="button"
    data-testid={testId}
    onClick={onClick}
    aria-label={ariaLabel}
    title={title ?? ariaLabel}
    aria-pressed={active}
    className={`${FLOATING_ICON_BTN_BASE} ${
      active ? 'border-kiviAccent/60 text-kiviAccent' : ''
    } ${className}`}
  >
    {icon}
  </button>
);

export interface ViewportIconMenuItem<T extends string> {
  value: T;
  label: string;
  badge?: string;
}

export interface ViewportIconMenuProps<T extends string> {
  icon: React.ReactNode;
  ariaLabel: string;
  headerLabel?: string;
  value: T | null;
  items: Array<ViewportIconMenuItem<T>>;
  onSelect: (value: T) => void;
  extraAction?: { label: string; onClick: () => void };
  placement?: 'left' | 'right';
  popupWidth?: number;
  testId?: string;
  className?: string;
}

export function ViewportIconMenu<T extends string>({
  icon,
  ariaLabel,
  headerLabel,
  value,
  items,
  onSelect,
  extraAction,
  placement = 'left',
  popupWidth = 188,
  testId,
  className = '',
}: ViewportIconMenuProps<T>): React.ReactElement {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<AnchoredPopupPos | null>(null);

  const recomputePos = useCallback(() => {
    const next = computeAnchoredPopupPos(
      triggerRef.current,
      popupWidth,
      Math.min(280, items.length * 32 + 64),
      placement
    );
    if (!next) {
      setOpen(false);
      return;
    }
    setPos(next);
  }, [items.length, placement, popupWidth]);

  useLayoutEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    recomputePos();
  }, [open, recomputePos]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        setOpen(false);
        triggerRef.current?.focus({ preventScroll: true });
      }
    };
    const onViewportChange = () => recomputePos();
    document.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('resize', onViewportChange);
    window.addEventListener('scroll', onViewportChange, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('resize', onViewportChange);
      window.removeEventListener('scroll', onViewportChange, true);
    };
  }, [open, recomputePos]);

  const panel =
    open && pos ? (
      <div
        ref={menuRef}
        role="listbox"
        aria-label={ariaLabel}
        data-testid={testId ? `${testId}-panel` : undefined}
        style={{
          position: 'fixed',
          left: pos.left,
          width: pos.width,
          top: pos.top,
          bottom: pos.bottom,
        }}
        className="kivi-menu-flyout kivi-anim-pop-in z-[90] overflow-hidden rounded-lg border border-kiviBorder/80 bg-kiviSurface/95 font-mono text-kiviInk backdrop-blur-md"
      >
        {headerLabel && (
          <div className="border-b border-kiviBorder/60 bg-kiviBg/70 px-2.5 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
            {headerLabel}
          </div>
        )}
        <div style={{ maxHeight: pos.maxHeight }} className="overflow-y-auto py-1">
          {items.map((item) => {
            const selected = item.value === value;
            return (
              <button
                key={item.value}
                type="button"
                role="option"
                aria-selected={selected}
                onClick={() => {
                  onSelect(item.value);
                  setOpen(false);
                }}
                className={`flex w-full items-center justify-between gap-2 px-2.5 py-1.5 text-left text-xs transition-colors ${
                  selected
                    ? 'bg-kiviAccent/15 font-semibold text-kiviAccent'
                    : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                }`}
              >
                <span className="truncate">{item.label}</span>
                <span className="flex shrink-0 items-center gap-1.5">
                  {item.badge && (
                    <span className="rounded border border-kiviBorder/60 px-1 text-[10px] text-kiviMuted">
                      {item.badge}
                    </span>
                  )}
                  {selected && <Check className="h-3.5 w-3.5 text-kiviAccent" aria-hidden="true" />}
                </span>
              </button>
            );
          })}
          {extraAction && (
            <div className="mt-1 border-t border-kiviBorder/60 pt-1">
              <button
                type="button"
                onClick={() => {
                  extraAction.onClick();
                  setOpen(false);
                }}
                className="flex w-full items-center px-2.5 py-1.5 text-left text-xs text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
              >
                {extraAction.label}
              </button>
            </div>
          )}
        </div>
      </div>
    ) : null;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        data-testid={testId}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        title={ariaLabel}
        onClick={() => setOpen((v) => !v)}
        className={`${FLOATING_ICON_BTN_BASE} ${
          open ? 'border-kiviAccent/70 text-kiviAccent' : ''
        } ${className}`}
      >
        {icon}
      </button>
      {panel && (typeof document !== 'undefined' ? createPortal(panel, document.body) : panel)}
    </>
  );
}

export interface ViewportIconPopoverProps {
  icon: React.ReactNode;
  ariaLabel: string;
  headerLabel?: string;
  active?: boolean;
  placement?: 'left' | 'right';
  popupWidth?: number;
  testId?: string;
  className?: string;
  children: React.ReactNode;
}

export const ViewportIconPopover: React.FC<ViewportIconPopoverProps> = ({
  icon,
  ariaLabel,
  headerLabel,
  active = false,
  placement = 'left',
  popupWidth = 232,
  testId,
  className = '',
  children,
}) => {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<AnchoredPopupPos | null>(null);

  const recomputePos = useCallback(() => {
    const next = computeAnchoredPopupPos(triggerRef.current, popupWidth, 260, placement);
    if (!next) {
      setOpen(false);
      return;
    }
    setPos(next);
  }, [placement, popupWidth]);

  useLayoutEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    recomputePos();
  }, [open, recomputePos]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (triggerRef.current?.contains(target) || panelRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        setOpen(false);
        triggerRef.current?.focus({ preventScroll: true });
      }
    };
    const onViewportChange = () => recomputePos();
    document.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('resize', onViewportChange);
    window.addEventListener('scroll', onViewportChange, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('resize', onViewportChange);
      window.removeEventListener('scroll', onViewportChange, true);
    };
  }, [open, recomputePos]);

  const panel =
    open && pos ? (
      <div
        ref={panelRef}
        role="dialog"
        aria-label={ariaLabel}
        data-testid={testId ? `${testId}-panel` : undefined}
        style={{
          position: 'fixed',
          left: pos.left,
          width: pos.width,
          top: pos.top,
          bottom: pos.bottom,
        }}
        className="kivi-menu-flyout kivi-anim-pop-in z-[90] overflow-hidden rounded-lg border border-kiviBorder/80 bg-kiviSurface/95 font-mono text-kiviInk backdrop-blur-md"
      >
        {headerLabel && (
          <div className="border-b border-kiviBorder/60 bg-kiviBg/70 px-2.5 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
            {headerLabel}
          </div>
        )}
        <div style={{ maxHeight: pos.maxHeight }} className="overflow-y-auto p-2.5">
          {children}
        </div>
      </div>
    ) : null;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        data-testid={testId}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={ariaLabel}
        title={ariaLabel}
        onClick={() => setOpen((v) => !v)}
        className={`${FLOATING_ICON_BTN_BASE} ${
          open || active ? 'border-kiviAccent/70 text-kiviAccent' : ''
        } ${className}`}
      >
        {icon}
      </button>
      {panel && (typeof document !== 'undefined' ? createPortal(panel, document.body) : panel)}
    </>
  );
};
