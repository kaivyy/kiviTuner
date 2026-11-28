import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react';
import { ExternalLink, Pin, PinOff } from 'lucide-react';
import {
  bridgeOpenToolWindow,
  bridgeSetWindowAlwaysOnTop,
  type ToolWindowId,
} from '../../lib/windowControlsBridge';
import { isDesktopApp } from '../../lib/tauriBridge';
import { prefersReducedMotion } from '../../hooks/useAnimatedPresence';
import { WindowTrafficLights } from './WindowTrafficLights';
import {
  applyWindowResize,
  clampWindowPosition,
  computeInitialRect,
  computeMaximizedRect,
  fitRectToViewport,
  focusFloatingWindow,
  getOpenFloatingWindowCount,
  getTopFloatingWindowId,
  getWindowZIndex,
  registerFloatingWindow,
  setFloatingWindowMinimized,
  subscribeWindowRestore,
  subscribeWindowStack,
  unregisterFloatingWindow,
  type ResizeEdge,
  type WindowDock,
  type WindowRect,
} from './floatingWindowModel';

export interface FloatingWindowContextValue {
  width: number;
  isCompact: boolean;
  standalone: boolean;
}

const FloatingWindowContext = createContext<FloatingWindowContextValue>({
  width: 800,
  isCompact: false,
  standalone: false,
});

export function useFloatingWindowContext(): FloatingWindowContextValue {
  return useContext(FloatingWindowContext);
}

export interface FloatingWindowProps {
  windowId: string;
  isOpen: boolean;
  onClose: () => void;
  title: string;
  titleId?: string;
  ariaLabel?: string;
  ariaDescribedBy?: string;
  badge?: string;
  icon?: React.ReactNode;
  defaultWidth?: number;
  defaultHeight?: number;
  minWidth?: number;
  minHeight?: number;
  dock?: WindowDock;
  cascadeIndex?: number;
  standalone?: boolean;
  popoutToolId?: ToolWindowId;
  titlebarTone?: 'surface' | 'sidebar';
  closeLabel?: string;
  headerActions?: React.ReactNode;
  footer?: React.ReactNode;
  children: React.ReactNode;
}

const RESIZE_HANDLES: Array<{ edge: ResizeEdge; className: string }> = [
  { edge: 'n', className: 'top-0 left-2 right-2 h-1.5 cursor-ns-resize' },
  { edge: 's', className: 'bottom-0 left-2 right-2 h-1.5 cursor-ns-resize' },
  { edge: 'e', className: 'top-2 bottom-2 right-0 w-1.5 cursor-ew-resize' },
  { edge: 'w', className: 'top-2 bottom-2 left-0 w-1.5 cursor-ew-resize' },
  { edge: 'nw', className: 'top-0 left-0 h-3 w-3 cursor-nwse-resize' },
  { edge: 'ne', className: 'top-0 right-0 h-3 w-3 cursor-nesw-resize' },
  { edge: 'sw', className: 'bottom-0 left-0 h-3 w-3 cursor-nesw-resize' },
  { edge: 'se', className: 'bottom-0 right-0 h-3.5 w-3.5 cursor-nwse-resize' },
];

export const FloatingWindow: React.FC<FloatingWindowProps> = ({
  windowId,
  isOpen,
  onClose,
  title,
  titleId,
  ariaLabel,
  ariaDescribedBy,
  badge,
  icon,
  defaultWidth = 720,
  defaultHeight = 640,
  minWidth = 380,
  minHeight = 360,
  dock = 'center',
  cascadeIndex,
  standalone = false,
  popoutToolId,
  titlebarTone = 'surface',
  closeLabel,
  headerActions,
  footer,
  children,
}) => {
  const [viewportSize, setViewportSize] = useState(() => ({
    w: typeof window === 'undefined' ? 1440 : window.innerWidth,
    h: typeof window === 'undefined' ? 900 : window.innerHeight,
  }));

  const [rect, setRect] = useState<WindowRect>(() =>
    computeInitialRect(
      typeof window === 'undefined' ? 1440 : window.innerWidth,
      typeof window === 'undefined' ? 900 : window.innerHeight,
      defaultWidth,
      defaultHeight,
      minWidth,
      minHeight,
      cascadeIndex ?? getOpenFloatingWindowCount(),
      dock
    )
  );
  const [isMinimized, setIsMinimized] = useState(false);
  const [isMaximized, setIsMaximized] = useState(false);
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [exitAction, setExitAction] = useState<'none' | 'closing' | 'minimizing'>('none');
  const [, setStackTick] = useState(0);

  const restoreRectRef = useRef<WindowRect | null>(null);
  const exitTimerRef = useRef<number | null>(null);
  const prevOpenRef = useRef(false);
  const rectRef = useRef(rect);
  rectRef.current = rect;

  useEffect(() => {
    return () => {
      if (exitTimerRef.current !== null) {
        window.clearTimeout(exitTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (standalone || !isOpen) {
      prevOpenRef.current = isOpen;
      if (exitTimerRef.current !== null) {
        window.clearTimeout(exitTimerRef.current);
        exitTimerRef.current = null;
      }
      setExitAction('none');
      unregisterFloatingWindow(windowId);
      return;
    }
    if (exitTimerRef.current !== null) {
      window.clearTimeout(exitTimerRef.current);
      exitTimerRef.current = null;
    }
    setExitAction('none');
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    setViewportSize({ w: vw, h: vh });

    if (!prevOpenRef.current) {
      const effectiveCascade = cascadeIndex ?? getOpenFloatingWindowCount();
      setIsMinimized(false);
      setIsMaximized(false);
      restoreRectRef.current = null;
      setRect(
        computeInitialRect(
          vw,
          vh,
          defaultWidth,
          defaultHeight,
          minWidth,
          minHeight,
          effectiveCascade,
          dock
        )
      );
    }
    prevOpenRef.current = true;
    registerFloatingWindow(windowId);
    focusFloatingWindow(windowId);
    const unsub = subscribeWindowStack(() => setStackTick((t) => t + 1));
    const unsubRestore = subscribeWindowRestore((restoredId) => {
      if (restoredId === windowId) {
        if (exitTimerRef.current !== null) {
          window.clearTimeout(exitTimerRef.current);
          exitTimerRef.current = null;
        }
        setExitAction('none');
        setIsMinimized(false);
        focusFloatingWindow(windowId);
      }
    });
    return () => {
      unsub();
      unsubRestore();
      unregisterFloatingWindow(windowId);
    };
  }, [
    standalone,
    isOpen,
    windowId,
    defaultWidth,
    defaultHeight,
    minWidth,
    minHeight,
    cascadeIndex,
    dock,
  ]);

  const requestClose = useCallback(() => {
    if (exitAction !== 'none') return;
    if (standalone || prefersReducedMotion()) {
      onClose();
      return;
    }
    setExitAction('closing');
    if (exitTimerRef.current !== null) window.clearTimeout(exitTimerRef.current);
    exitTimerRef.current = window.setTimeout(() => {
      exitTimerRef.current = null;
      setExitAction('none');
      onClose();
    }, 120);
  }, [exitAction, standalone, onClose]);

  useEffect(() => {
    if (!isOpen) return;
    const onResize = () => {
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      setViewportSize({ w: vw, h: vh });
      if (standalone) return;
      setRect((prev) => {
        if (isMaximized) return computeMaximizedRect(vw, vh);
        return fitRectToViewport(prev, { minWidth, minHeight }, vw, vh);
      });
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [isOpen, standalone, isMaximized, minWidth, minHeight]);

  useEffect(() => {
    if (!isOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (!standalone && getTopFloatingWindowId() !== windowId) return;
      e.preventDefault();
      requestClose();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isOpen, standalone, windowId, requestClose]);

  const handleDragStart = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (standalone || e.button !== 0 || isMaximized) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest('button, a, input, select, textarea, [role="button"]')) return;
      e.preventDefault();
      focusFloatingWindow(windowId);
      const startX = e.clientX;
      const startY = e.clientY;
      const origin = rectRef.current;
      const el = e.currentTarget;
      const pointerId = e.pointerId;
      if (typeof el.setPointerCapture === 'function') {
        try {
          el.setPointerCapture(pointerId);
        } catch {
          // Synthetic test events may lack active pointer capture targets.
        }
      }

      const onMove = (ev: PointerEvent) => {
        const next = clampWindowPosition(
          {
            ...origin,
            x: origin.x + (ev.clientX - startX),
            y: origin.y + (ev.clientY - startY),
          },
          window.innerWidth,
          window.innerHeight
        );
        setRect(next);
      };

      const onUp = () => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        window.removeEventListener('pointercancel', onUp);
        if (typeof el.releasePointerCapture === 'function') {
          try {
            el.releasePointerCapture(pointerId);
          } catch {
            // Pointer may already be released.
          }
        }
      };

      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', onUp);
    },
    [standalone, isMaximized, windowId]
  );

  const handleResizeStart = useCallback(
    (edge: ResizeEdge, e: React.PointerEvent<HTMLDivElement>) => {
      if (standalone || e.button !== 0) return;
      e.stopPropagation();
      e.preventDefault();
      focusFloatingWindow(windowId);
      const startX = e.clientX;
      const startY = e.clientY;
      const startRect = rectRef.current;

      const onMove = (ev: PointerEvent) => {
        const next = applyWindowResize(
          startRect,
          edge,
          ev.clientX - startX,
          ev.clientY - startY,
          { minWidth, minHeight },
          window.innerWidth,
          window.innerHeight
        );
        setRect(next);
      };

      const onUp = () => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        window.removeEventListener('pointercancel', onUp);
      };

      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', onUp);
    },
    [standalone, windowId, minWidth, minHeight]
  );

  const handleToggleMinimize = useCallback(() => {
    if (exitAction !== 'none') return;
    if (isMinimized || standalone || prefersReducedMotion()) {
      setIsMinimized((prev) => !prev);
      return;
    }
    setExitAction('minimizing');
    if (exitTimerRef.current !== null) window.clearTimeout(exitTimerRef.current);
    exitTimerRef.current = window.setTimeout(() => {
      exitTimerRef.current = null;
      setExitAction('none');
      setIsMinimized(true);
    }, 120);
  }, [exitAction, isMinimized, standalone]);

  const handleToggleMaximize = useCallback(() => {
    setIsMinimized(false);
    if (!isMaximized) {
      restoreRectRef.current = rectRef.current;
      setRect(computeMaximizedRect(window.innerWidth, window.innerHeight));
      setIsMaximized(true);
    } else {
      if (restoreRectRef.current) {
        setRect(clampWindowPosition(restoreRectRef.current, window.innerWidth, window.innerHeight));
      }
      setIsMaximized(false);
    }
  }, [isMaximized]);

  const handlePopout = useCallback(() => {
    if (!popoutToolId) return;
    void bridgeOpenToolWindow({
      toolId: popoutToolId,
      title,
      width: Math.max(rectRef.current.width, minWidth),
      height: Math.max(rectRef.current.height, minHeight),
      minWidth,
      minHeight,
    }).then((ok) => {
      if (ok) onClose();
    });
  }, [popoutToolId, title, minWidth, minHeight, onClose]);

  const handleToggleAlwaysOnTop = useCallback(() => {
    const next = !alwaysOnTop;
    void bridgeSetWindowAlwaysOnTop(next).then((applied) => {
      setAlwaysOnTop(applied);
    });
  }, [alwaysOnTop]);

  useEffect(() => {
    if (standalone || !isOpen) return;
    setFloatingWindowMinimized(windowId, title, isMinimized);
  }, [standalone, isOpen, windowId, title, isMinimized]);

  if (!isOpen) return null;
  if (!standalone && isMinimized) return null;

  const activeWidth = standalone ? viewportSize.w : rect.width;
  const ctxValue: FloatingWindowContextValue = {
    width: activeWidth,
    isCompact: activeWidth < 660,
    standalone,
  };

  const zIndex = standalone ? 50 : getWindowZIndex(windowId);
  const showDesktopPopout = !standalone && Boolean(popoutToolId) && isDesktopApp();
  const showBadge = Boolean(badge) && activeWidth >= 460;

  return (
    <FloatingWindowContext.Provider value={ctxValue}>
      <div
        role="dialog"
        aria-modal={standalone ? true : undefined}
        aria-labelledby={titleId}
        aria-label={titleId ? undefined : ariaLabel ?? title}
        aria-describedby={ariaDescribedBy}
        data-window-id={windowId}
        data-floating-window={standalone ? 'standalone' : 'true'}
        data-minimized={isMinimized ? 'true' : 'false'}
        onPointerDown={() => {
          if (!standalone) focusFloatingWindow(windowId);
        }}
        style={
          standalone
            ? undefined
            : {
                left: rect.x,
                top: rect.y,
                width: rect.width,
                height: rect.height,
                zIndex,
              }
        }
        className={
          standalone
            ? 'kivi-anim-window-in fixed inset-0 flex h-screen w-screen flex-col overflow-hidden bg-kiviSurface font-mono text-kiviInk'
            : `kivi-floating-window fixed flex flex-col overflow-hidden rounded-2xl bg-kiviSurface font-mono text-kiviInk ${
                exitAction === 'closing'
                  ? 'kivi-anim-window-out'
                  : exitAction === 'minimizing'
                    ? 'kivi-anim-window-minimize'
                    : 'kivi-anim-window-in'
              }`
        }
      >
        {/* Slim Borderless Window Titlebar */}
        <div
          data-testid={`window-titlebar-${windowId}`}
          data-tauri-drag-region={standalone ? 'deep' : undefined}
          onPointerDown={standalone ? undefined : handleDragStart}
          onDoubleClick={standalone ? undefined : handleToggleMaximize}
          className={`flex h-8 shrink-0 select-none items-center justify-between gap-2 px-3 ${
            titlebarTone === 'sidebar' ? 'bg-kiviBg' : 'bg-kiviSurface'
          } ${standalone ? 'cursor-default' : 'touch-none cursor-grab active:cursor-grabbing'}`}
        >
          <div className="flex min-w-0 flex-1 items-center gap-2">
            {standalone ? (
              <WindowTrafficLights
                closeLabel={closeLabel}
                onClose={requestClose}
              />
            ) : (
              <WindowTrafficLights
                onClose={requestClose}
                onMinimize={handleToggleMinimize}
                onToggleMaximize={handleToggleMaximize}
                isMaximized={isMaximized}
                isMinimized={isMinimized}
                closeLabel={closeLabel}
              />
            )}
            {icon && (
              <span className="pointer-events-none flex shrink-0 items-center text-kiviAccent">
                {icon}
              </span>
            )}
            <h2
              id={titleId}
              className="pointer-events-none min-w-0 truncate whitespace-nowrap text-[11px] font-semibold text-kiviInk"
            >
              {title}
            </h2>
            {showBadge && (
              <span className="pointer-events-none shrink-0 whitespace-nowrap rounded bg-kiviElevated/75 px-1.5 py-0.5 text-[9px] text-kiviMuted">
                {badge}
              </span>
            )}
          </div>

          <div
            data-tauri-drag-region={standalone ? 'false' : undefined}
            className="flex shrink-0 items-center gap-1"
            onPointerDown={(e) => e.stopPropagation()}
            onMouseDown={(e) => e.stopPropagation()}
          >
            {headerActions}
            {standalone && (
              <button
                type="button"
                aria-label="Pin window on top"
                aria-pressed={alwaysOnTop}
                title={alwaysOnTop ? 'Unpin from top' : 'Keep window on top'}
                onClick={handleToggleAlwaysOnTop}
                className={`inline-flex h-6 items-center gap-1 rounded px-1.5 text-[10px] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-kiviAccent ${
                  alwaysOnTop
                    ? 'bg-kiviAccent/15 text-kiviAccent'
                    : 'bg-kiviElevated/65 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                }`}
              >
                {alwaysOnTop ? (
                  <PinOff className="h-3 w-3 shrink-0" />
                ) : (
                  <Pin className="h-3 w-3 shrink-0" />
                )}
                <span className="hidden sm:inline">{alwaysOnTop ? 'Pinned' : 'Pin'}</span>
              </button>
            )}
            {showDesktopPopout && (
              <button
                type="button"
                aria-label={`Open ${title} in separate desktop window`}
                title="Open in separate desktop window"
                onClick={handlePopout}
                className="inline-flex h-6 items-center gap-1 rounded bg-kiviElevated/65 px-1.5 text-[10px] text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
              >
                <ExternalLink className="h-3 w-3 shrink-0" />
                <span className="hidden sm:inline">Pop out</span>
              </button>
            )}
          </div>
        </div>

        {/* Body Content (hidden when collapsed to titlebar strip) */}
        {!isMinimized && (
          <>
            <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{children}</div>
            {footer}
          </>
        )}

        {/* 8-Edge & Corner Resize Handles (In-App Floating Mode) */}
        {!standalone && !isMinimized && !isMaximized && (
          <>
            {RESIZE_HANDLES.map(({ edge, className }) => (
              <div
                key={edge}
                data-testid={`floating-resize-${windowId}-${edge}`}
                onPointerDown={(e) => handleResizeStart(edge, e)}
                className={`touch-none absolute z-10 ${className}`}
                aria-hidden="true"
              />
            ))}
          </>
        )}
      </div>
    </FloatingWindowContext.Provider>
  );
};
