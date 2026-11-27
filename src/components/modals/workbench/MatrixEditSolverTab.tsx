import React from 'react';
import type { SelectionRange } from '../../editor';
import type { MapDef } from '../../../types/kivi';
import type {
  FactorOffsetSolution,
  RegionInterpAxes,
  RegionInterpMode,
} from '../../../lib/workbenchBridge';

export type EditOpKind = 'SetAbsolute' | 'AddDelta' | 'ScalePercent' | 'ClampRange';

export interface MatrixEditSolverTabProps {
  busy: boolean;
  selectedMap: MapDef | null;
  selection: SelectionRange | null;
  editOpKind: EditOpKind;
  editValue: string;
  clampMin: string;
  clampMax: string;
  interpMode: RegionInterpMode;
  interpAxes: RegionInterpAxes;
  allowExtrapolation: boolean;
  p1x: string;
  p1y: string;
  p2x: string;
  p2y: string;
  useThirdPoint: boolean;
  p3x: string;
  p3y: string;
  swapInputOutput: boolean;
  solverResult: FactorOffsetSolution | null;
  onSetEditOpKind: (op: EditOpKind) => void;
  onSetEditValue: (v: string) => void;
  onSetClampMin: (v: string) => void;
  onSetClampMax: (v: string) => void;
  onSelectionEdit: () => void;
  onSetInterpMode: (m: RegionInterpMode) => void;
  onSetInterpAxes: (a: RegionInterpAxes) => void;
  onSetAllowExtrapolation: (v: boolean) => void;
  onInterpolate: () => void;
  onSetP1x: (v: string) => void;
  onSetP1y: (v: string) => void;
  onSetP2x: (v: string) => void;
  onSetP2y: (v: string) => void;
  onSetUseThirdPoint: (v: boolean) => void;
  onSetP3x: (v: string) => void;
  onSetP3y: (v: string) => void;
  onSetSwapInputOutput: (v: boolean) => void;
  onSolveConversion: () => void;
  onUpdateMapScaling: (mapId: string, equation: string) => void;
  onInfo: (msg: string) => void;
}

export const MatrixEditSolverTab: React.FC<MatrixEditSolverTabProps> = ({
  busy,
  selectedMap,
  selection,
  editOpKind,
  editValue,
  clampMin,
  clampMax,
  interpMode,
  interpAxes,
  allowExtrapolation,
  p1x,
  p1y,
  p2x,
  p2y,
  useThirdPoint,
  p3x,
  p3y,
  swapInputOutput,
  solverResult,
  onSetEditOpKind,
  onSetEditValue,
  onSetClampMin,
  onSetClampMax,
  onSelectionEdit,
  onSetInterpMode,
  onSetInterpAxes,
  onSetAllowExtrapolation,
  onInterpolate,
  onSetP1x,
  onSetP1y,
  onSetP2x,
  onSetP2y,
  onSetUseThirdPoint,
  onSetP3x,
  onSetP3y,
  onSetSwapInputOutput,
  onSolveConversion,
  onUpdateMapScaling,
  onInfo,
}) => (
  <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
    <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
      <div className="font-semibold text-kiviInk">
        Selection Operations ({selectedMap ? selectedMap.name : 'No Map Selected'})
      </div>
      <p className="text-[11px] text-kiviMuted">
        Selection:{' '}
        {selection
          ? `R${selection.minR}..${selection.maxR}, C${selection.minC}..${selection.maxC}`
          : 'Entire active matrix'}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <select
          value={editOpKind}
          onChange={(e) => onSetEditOpKind(e.target.value as EditOpKind)}
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
        >
          <option value="AddDelta">Add Delta</option>
          <option value="SetAbsolute">Set Value (=)</option>
          <option value="ScalePercent">Scale % (+/-)</option>
          <option value="ClampRange">Clamp Min/Max</option>
        </select>
        {editOpKind !== 'ClampRange' ? (
          <input
            type="number"
            step="any"
            value={editValue}
            onChange={(e) => onSetEditValue(e.target.value)}
            className="w-24 rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
        ) : (
          <>
            <input
              type="number"
              step="any"
              value={clampMin}
              onChange={(e) => onSetClampMin(e.target.value)}
              className="w-20 rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
            />
            <input
              type="number"
              step="any"
              value={clampMax}
              onChange={(e) => onSetClampMax(e.target.value)}
              className="w-20 rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
            />
          </>
        )}
        <button
          type="button"
          disabled={busy}
          onClick={onSelectionEdit}
          className="rounded bg-kiviAccent/20 px-3 py-1 text-kiviAccent hover:bg-kiviAccent/30"
        >
          Apply Edit
        </button>
      </div>

      <div className="mt-3 border-t border-kiviBorder pt-2">
        <div className="mb-1.5 font-semibold text-kiviInk">Region Interpolation</div>
        <div className="flex flex-wrap items-center gap-2">
          <select
            value={interpMode}
            onChange={(e) => onSetInterpMode(e.target.value as RegionInterpMode)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
          >
            <option value="Linear">Linear</option>
            <option value="CubicCatmullRom">Cubic Catmull-Rom</option>
          </select>
          <select
            value={interpAxes}
            onChange={(e) => onSetInterpAxes(e.target.value as RegionInterpAxes)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
          >
            <option value="AllFourSides">All Four Sides</option>
            <option value="HorizontalOnly">Horizontal Only</option>
            <option value="VerticalOnly">Vertical Only</option>
          </select>
          <label className="flex items-center gap-1 text-kiviSecondary">
            <input
              type="checkbox"
              checked={allowExtrapolation}
              onChange={(e) => onSetAllowExtrapolation(e.target.checked)}
            />
            <span>Extrapolate</span>
          </label>
          <button
            type="button"
            disabled={busy}
            onClick={onInterpolate}
            className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-3 py-1 text-kiviAccent"
          >
            Interpolate Region
          </button>
        </div>
      </div>
    </div>

    <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
      <div className="font-semibold text-kiviInk">Factor &amp; Offset Conversion Solver</div>
      <div className="grid grid-cols-2 gap-2">
        <input
          type="number"
          step="any"
          aria-label="Point 1 raw"
          value={p1x}
          onChange={(e) => onSetP1x(e.target.value)}
          placeholder="P1 X"
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
        />
        <input
          type="number"
          step="any"
          aria-label="Point 1 physical"
          value={p1y}
          onChange={(e) => onSetP1y(e.target.value)}
          placeholder="P1 Y"
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
        />
        <input
          type="number"
          step="any"
          aria-label="Point 2 raw"
          value={p2x}
          onChange={(e) => onSetP2x(e.target.value)}
          placeholder="P2 X"
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
        />
        <input
          type="number"
          step="any"
          aria-label="Point 2 physical"
          value={p2y}
          onChange={(e) => onSetP2y(e.target.value)}
          placeholder="P2 Y"
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
        />
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-1 text-kiviSecondary">
          <input
            type="checkbox"
            checked={useThirdPoint}
            onChange={(e) => onSetUseThirdPoint(e.target.checked)}
          />
          <span>3-Point Reciprocal</span>
        </label>
        <label className="flex items-center gap-1 text-kiviSecondary">
          <input
            type="checkbox"
            checked={swapInputOutput}
            onChange={(e) => onSetSwapInputOutput(e.target.checked)}
          />
          <span>Swap X/Y</span>
        </label>
      </div>
      {useThirdPoint && (
        <div className="grid grid-cols-2 gap-2">
          <input
            type="number"
            step="any"
            value={p3x}
            onChange={(e) => onSetP3x(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
          <input
            type="number"
            step="any"
            value={p3y}
            onChange={(e) => onSetP3y(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
        </div>
      )}
      <button
        type="button"
        disabled={busy}
        onClick={onSolveConversion}
        className="rounded bg-kiviAccent/20 px-3 py-1 text-kiviAccent hover:bg-kiviAccent/30"
      >
        Solve Equation
      </button>
      {solverResult && (
        <div className="rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[11px] text-kiviInk">
          <div>Equation: {solverResult.equation}</div>
          <div>
            Factor: {solverResult.factor} | Offset: {solverResult.offset}
          </div>
          {selectedMap && (
            <button
              type="button"
              onClick={() => {
                onUpdateMapScaling(selectedMap.id, solverResult.equation);
                onInfo(`Updated ${selectedMap.name} equation to "${solverResult.equation}".`);
              }}
              className="mt-1.5 rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2 py-0.5 text-[10px] text-kiviGreen"
            >
              Apply Equation to {selectedMap.name}
            </button>
          )}
        </div>
      )}
    </div>
  </div>
);
