import React, { useCallback, useEffect, useRef, useState } from 'react';

export interface SplitResizeHandleProps {
  orientation: 'vertical' | 'horizontal';
  ariaLabel: string;
  testId?: string;
  onDragStart?: () => void;
  onDragMove: (clientX: number, clientY: number) => void;
  onDragEnd?: () => void;
  onReset?: () => void;
  onStep?: (delta: number) => void;
  collapseWarning?: boolean;
  className?: string;
}

/**
 * Lightweight, pointer-captured split/panel resize handle.
 * Supports smooth pointer drag, double-click reset, keyboard stepping, and collapse preview highlight.
 */
export const SplitResizeHandle: React.FC<SplitResizeHandleProps> = ({
  orientation,
  ariaLabel,
  testId,
  onDragStart,
  onDragMove,
  onDragEnd,
  onReset,
  onStep,
  collapseWarning = false,
  className = '',
}) => {
  const [dragging, setDragging] = useState(false);
  const cleanupRef = useRef<(() => void) | null>(null);
  const callbacksRef = useRef({ onDragStart, onDragMove, onDragEnd });
  callbacksRef.current = { onDragStart, onDragMove, onDragEnd };

  useEffect(() => {
    return () => {
      cleanupRef.current?.();
      cleanupRef.current = null;
    };
  }, []);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();

      const el = e.currentTarget;
      const pointerId = e.pointerId;
      if (typeof el.setPointerCapture === 'function') {
        try {
          el.setPointerCapture(pointerId);
        } catch {
          // Synthetic test events may omit active pointer capture.
        }
      }

      const prevCursor = document.body.style.cursor;
      document.body.style.cursor = orientation === 'vertical' ? 'col-resize' : 'row-resize';
      setDragging(true);
      callbacksRef.current.onDragStart?.();

      const onMove = (ev: PointerEvent) => {
        callbacksRef.current.onDragMove(ev.clientX, ev.clientY);
      };

      const finish = () => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', finish);
        window.removeEventListener('pointercancel', finish);
        document.body.style.cursor = prevCursor;
        setDragging(false);
        cleanupRef.current = null;
        if (typeof el.releasePointerCapture === 'function') {
          try {
            el.releasePointerCapture(pointerId);
          } catch {
            // Pointer capture already released.
          }
        }
        callbacksRef.current.onDragEnd?.();
      };

      cleanupRef.current = finish;
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', finish);
      window.addEventListener('pointercancel', finish);
    },
    [orientation]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (!onStep) return;
      const step = e.shiftKey ? 32 : 12;
      if (orientation === 'vertical') {
        if (e.key === 'ArrowLeft') {
          e.preventDefault();
          onStep(-step);
        } else if (e.key === 'ArrowRight') {
          e.preventDefault();
          onStep(step);
        }
      } else {
        if (e.key === 'ArrowUp') {
          e.preventDefault();
          onStep(-step);
        } else if (e.key === 'ArrowDown') {
          e.preventDefault();
          onStep(step);
        }
      }
      if (e.key === 'Home' && onReset) {
        e.preventDefault();
        onReset();
      }
    },
    [orientation, onStep, onReset]
  );

  const isVertical = orientation === 'vertical';
  const trackTone = collapseWarning
    ? 'bg-kiviRed'
    : dragging
      ? 'bg-kiviAccent'
      : 'bg-kiviBorder/60 group-hover:bg-kiviAccent/80';

  return (
    <div
      role="separator"
      tabIndex={0}
      aria-orientation={orientation}
      aria-label={ariaLabel}
      data-testid={testId}
      onPointerDown={handlePointerDown}
      onDoubleClick={onReset}
      onKeyDown={handleKeyDown}
      title="Drag to resize (double-click to reset)"
      className={`group relative z-20 flex shrink-0 touch-none select-none items-center justify-center focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-kiviAccent ${
        isVertical ? 'w-1.5 cursor-col-resize' : 'h-1.5 w-full cursor-row-resize'
      } ${className}`}
    >
      <div
        className={`transition-colors duration-150 ${
          isVertical
            ? dragging || collapseWarning
              ? 'h-full w-0.5'
              : 'h-full w-px group-hover:w-0.5'
            : dragging || collapseWarning
              ? 'h-0.5 w-full'
              : 'h-px w-full group-hover:h-0.5'
        } ${trackTone}`}
      />
    </div>
  );
};
