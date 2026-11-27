import React, { useEffect, useState } from 'react';
import {
  bridgeWindowClose,
  bridgeWindowIsMaximized,
  bridgeWindowMinimize,
  bridgeWindowToggleMaximize,
  subscribeWindowState,
} from '../../lib/windowControlsBridge';

export interface WindowTrafficLightsProps {
  onClose?: () => void;
  onMinimize?: () => void;
  onToggleMaximize?: (altKey: boolean) => void;
  isMaximized?: boolean;
  isMinimized?: boolean;
  closeLabel?: string;
  minimizeLabel?: string;
  maximizeLabel?: string;
}

/**
 * macOS Tahoe-inspired traffic-light window controls with crisp specular
 * edge rendering, compact height integration, and full keyboard focus rings.
 */
export const WindowTrafficLights: React.FC<WindowTrafficLightsProps> = ({
  onClose,
  onMinimize,
  onToggleMaximize,
  isMaximized: controlledMaximized,
  isMinimized = false,
  closeLabel,
  minimizeLabel,
  maximizeLabel,
}) => {
  const isNativeMaximize = !onToggleMaximize;
  const [focused, setFocused] = useState(
    () => (typeof document === 'undefined' ? true : document.hasFocus())
  );
  const [nativeMaximized, setNativeMaximized] = useState(false);
  const maximized = controlledMaximized ?? nativeMaximized;

  useEffect(() => {
    if (!isNativeMaximize) {
      setFocused(true);
      return;
    }
    let active = true;
    void bridgeWindowIsMaximized().then((state) => {
      if (active) setNativeMaximized(state);
    });
    const unsubscribe = subscribeWindowState((isFocused) => {
      setFocused(isFocused);
      void bridgeWindowIsMaximized().then((state) => {
        if (active) setNativeMaximized(state);
      });
    });
    return () => {
      active = false;
      unsubscribe();
    };
  }, [isNativeMaximize]);

  const handleClose = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (onClose) {
      onClose();
    } else {
      void bridgeWindowClose();
    }
  };

  const handleMinimize = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (onMinimize) {
      onMinimize();
    } else {
      void bridgeWindowMinimize();
    }
  };

  const handleToggleMax = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    if (onToggleMaximize) {
      onToggleMaximize(e.altKey);
    } else {
      void bridgeWindowToggleMaximize(e.altKey).then((nextState) => {
        setNativeMaximized(nextState);
      });
    }
  };

  const effectiveCloseLabel = closeLabel ?? 'Close window';
  const effectiveMinLabel =
    minimizeLabel ?? (isMinimized ? 'Expand window' : 'Minimize window');
  const effectiveMaxLabel =
    maximizeLabel ?? (maximized ? 'Restore window' : 'Maximize window');

  return (
    <div
      className="group/lights flex shrink-0 items-center gap-[7px]"
      role="group"
      aria-label="Window controls"
      onPointerDown={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <button
        type="button"
        aria-label={effectiveCloseLabel}
        title={effectiveCloseLabel}
        onClick={handleClose}
        className={`relative flex h-[11px] w-[11px] items-center justify-center rounded-full border-0 shadow-[inset_0_1px_0_rgba(255,255,255,0.28)] transition-all duration-150 hover:brightness-110 active:scale-90 focus-visible:ring-2 focus-visible:ring-kiviAccent ${
          focused
            ? 'bg-[#FF5F57]'
            : 'bg-kiviMuted/35 group-hover/lights:bg-[#FF5F57]'
        }`}
      >
        <svg
          viewBox="0 0 8 8"
          className="h-[7px] w-[7px] opacity-0 transition-opacity group-hover/lights:opacity-100 group-focus-within/lights:opacity-100"
          aria-hidden="true"
        >
          <path
            d="M1.75 1.75L6.25 6.25M6.25 1.75L1.75 6.25"
            stroke="#4C0002"
            strokeWidth="1.25"
            strokeLinecap="round"
          />
        </svg>
      </button>

      <button
        type="button"
        aria-label={effectiveMinLabel}
        title={effectiveMinLabel}
        onClick={handleMinimize}
        className={`relative flex h-[11px] w-[11px] items-center justify-center rounded-full border-0 shadow-[inset_0_1px_0_rgba(255,255,255,0.28)] transition-all duration-150 hover:brightness-110 active:scale-90 focus-visible:ring-2 focus-visible:ring-kiviAccent ${
          focused
            ? 'bg-[#FEBC2E]'
            : 'bg-kiviMuted/35 group-hover/lights:bg-[#FEBC2E]'
        }`}
      >
        <svg
          viewBox="0 0 8 8"
          className="h-[7px] w-[7px] opacity-0 transition-opacity group-hover/lights:opacity-100 group-focus-within/lights:opacity-100"
          aria-hidden="true"
        >
          <path
            d="M1.5 4H6.5"
            stroke="#5C3B00"
            strokeWidth="1.35"
            strokeLinecap="round"
          />
        </svg>
      </button>

      <button
        type="button"
        aria-label={effectiveMaxLabel}
        title={
          onToggleMaximize
            ? effectiveMaxLabel
            : maximized
              ? 'Restore window (Alt+Click for Fullscreen)'
              : 'Maximize window (Alt+Click for Fullscreen)'
        }
        onClick={handleToggleMax}
        className={`relative flex h-[11px] w-[11px] items-center justify-center rounded-full border-0 shadow-[inset_0_1px_0_rgba(255,255,255,0.28)] transition-all duration-150 hover:brightness-110 active:scale-90 focus-visible:ring-2 focus-visible:ring-kiviAccent ${
          focused
            ? 'bg-[#28C840]'
            : 'bg-kiviMuted/35 group-hover/lights:bg-[#28C840]'
        }`}
      >
        <svg
          viewBox="0 0 8 8"
          className="h-[7px] w-[7px] opacity-0 transition-opacity group-hover/lights:opacity-100 group-focus-within/lights:opacity-100"
          aria-hidden="true"
        >
          {maximized ? (
            <path
              d="M4.2 1.4L4.2 3.8L6.6 3.8M3.8 6.6L3.8 4.2L1.4 4.2"
              fill="#004D0F"
            />
          ) : (
            <path
              d="M1.4 1.4L4.4 1.4L1.4 4.4ZM6.6 6.6L3.6 6.6L6.6 3.6Z"
              fill="#004D0F"
            />
          )}
        </svg>
      </button>
    </div>
  );
};
