import React from 'react';
import { Percent, ClipboardPaste, ClipboardList, X, Check } from 'lucide-react';

interface HeatmapDialogsProps {
  isPercentModalOpen: boolean;
  percentInput: string;
  selectedCellCount: number;
  onChangePercentInput: (v: string) => void;
  onApplyPercentage: (pct: number) => void;
  onClosePercentModal: () => void;
  isPasteSpecialOpen: boolean;
  pasteSpecialMode: 'add' | 'multiply';
  pasteSpecialValue: string;
  onChangePasteSpecialMode: (mode: 'add' | 'multiply') => void;
  onChangePasteSpecialValue: (v: string) => void;
  onRunPasteSpecial: (mode: 'add' | 'multiply', explicitText?: string) => void;
  onClosePasteSpecial: () => void;
}

export const HeatmapDialogs: React.FC<HeatmapDialogsProps> = ({
  isPercentModalOpen,
  percentInput,
  selectedCellCount,
  onChangePercentInput,
  onApplyPercentage,
  onClosePercentModal,
  isPasteSpecialOpen,
  pasteSpecialMode,
  pasteSpecialValue,
  onChangePasteSpecialMode,
  onChangePasteSpecialValue,
  onRunPasteSpecial,
  onClosePasteSpecial,
}) => (
  <>
    {isPercentModalOpen && (
      <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
        <div className="w-80 rounded-lg border border-kiviBorder bg-kiviSurface p-4 shadow-2xl">
          <div className="flex items-center justify-between border-b border-kiviBorder pb-2 mb-3">
            <h4 className="text-sm font-semibold text-kiviInk flex items-center gap-1.5">
              <Percent className="h-4 w-4 text-kiviWarn" />
              <span>Adjust by Percentage</span>
            </h4>
            <button
              type="button"
              aria-label="Close percentage dialog"
              onClick={onClosePercentModal}
              className="text-kiviMuted hover:text-kiviInk"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <p className="text-xs text-kiviMuted mb-3">
            Scale{' '}
            <span className="font-semibold text-kiviInk">{selectedCellCount} selected cells</span>{' '}
            by a percentage:
          </p>

          <div className="grid grid-cols-4 gap-1.5 mb-3">
            {[-10, -5, +5, +10].map((preset) => (
              <button
                key={preset}
                type="button"
                onClick={() => onChangePercentInput(preset.toString())}
                className="rounded border border-kiviBorder bg-kiviSurface py-1 text-xs font-mono text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
              >
                {preset > 0 ? `+${preset}%` : `${preset}%`}
              </button>
            ))}
          </div>

          <div className="flex items-center gap-2 mb-4">
            <span className="text-xs text-kiviMuted font-medium">Percent:</span>
            <div className="relative flex-1">
              <input
                type="number"
                autoFocus
                aria-label="Adjustment percentage"
                value={percentInput}
                onChange={(e) => onChangePercentInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    onApplyPercentage(parseFloat(percentInput));
                  } else if (e.key === 'Escape') {
                    e.preventDefault();
                    onClosePercentModal();
                  }
                }}
                className="w-full rounded border border-kiviBorder bg-kiviSurface px-3 py-1.5 text-xs text-kiviInk focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent focus:border-kiviAccent"
                placeholder="e.g. 5 or -10"
              />
              <span className="absolute right-3 top-1.5 text-xs text-kiviMuted font-mono">%</span>
            </div>
          </div>

          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={onClosePercentModal}
              className="rounded px-3 py-1.5 text-xs font-medium text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk transition-colors"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => onApplyPercentage(parseFloat(percentInput))}
              className="flex items-center gap-1 rounded bg-kiviAccent px-3 py-1.5 text-xs font-semibold text-kiviAccentFg hover:opacity-90 transition-colors"
            >
              <Check className="h-3.5 w-3.5" />
              <span>Apply</span>
            </button>
          </div>
        </div>
      </div>
    )}

    {isPasteSpecialOpen && (
      <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
        <div className="w-84 rounded-lg border border-kiviBorder bg-kiviSurface p-4 shadow-2xl">
          <div className="flex items-center justify-between border-b border-kiviBorder pb-2 mb-3">
            <h4 className="text-sm font-semibold text-kiviInk flex items-center gap-1.5">
              <ClipboardList className="h-4 w-4 text-kiviAccent" />
              <span>Paste Special</span>
            </h4>
            <button
              type="button"
              aria-label="Close paste special dialog"
              onClick={onClosePasteSpecial}
              className="text-kiviMuted hover:text-kiviInk"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <div className="flex rounded border border-kiviBorder bg-kiviSurface p-0.5 mb-3">
            <button
              type="button"
              onClick={() => onChangePasteSpecialMode('add')}
              className={`flex-1 py-1.5 text-xs font-medium rounded transition-colors ${
                pasteSpecialMode === 'add'
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              Paste &amp; Add (Delta)
            </button>
            <button
              type="button"
              onClick={() => onChangePasteSpecialMode('multiply')}
              className={`flex-1 py-1.5 text-xs font-medium rounded transition-colors ${
                pasteSpecialMode === 'multiply'
                  ? 'bg-kiviAccent text-kiviAccentFg font-semibold'
                  : 'text-kiviMuted hover:text-kiviInk'
              }`}
            >
              Paste &amp; Multiply (Scale)
            </button>
          </div>

          <p className="text-xs text-kiviMuted mb-3">
            {pasteSpecialMode === 'add'
              ? 'Adds clipboard values (or specified delta) directly to selected cells.'
              : 'Multiplies selected cells by clipboard scale factors (or specified multiplier).'}
          </p>

          <div className="flex items-center gap-2 mb-4">
            <span className="text-xs text-kiviMuted font-medium">Constant:</span>
            <input
              type="number"
              step="any"
              aria-label="Paste special constant"
              value={pasteSpecialValue}
              onChange={(e) => onChangePasteSpecialValue(e.target.value)}
              placeholder={pasteSpecialMode === 'add' ? 'e.g. 5 or -2.5' : 'e.g. 1.05 for +5%'}
              className="flex-1 rounded border border-kiviBorder bg-kiviSurface px-3 py-1.5 text-xs text-kiviInk focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent focus:border-kiviAccent"
            />
            <button
              type="button"
              onClick={() => onRunPasteSpecial(pasteSpecialMode, pasteSpecialValue)}
              className="rounded bg-kiviElevated px-3 py-1.5 text-xs text-kiviInk hover:bg-kiviBorderStrong hover:text-kiviInk transition-colors"
            >
              Apply
            </button>
          </div>

          <div className="flex justify-between items-center border-t border-kiviBorder pt-3">
            <button
              type="button"
              onClick={onClosePasteSpecial}
              className="rounded px-3 py-1.5 text-xs font-medium text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk transition-colors"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => onRunPasteSpecial(pasteSpecialMode)}
              className="flex items-center gap-1 rounded bg-kiviAccent px-3 py-1.5 text-xs font-semibold text-kiviAccentFg hover:opacity-90 transition-colors"
            >
              <ClipboardPaste className="h-3.5 w-3.5" />
              <span>Paste from Clipboard</span>
            </button>
          </div>
        </div>
      </div>
    )}
  </>
);
