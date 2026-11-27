import React from 'react';
import type { MapDef } from '../../../types/kivi';
import { blindTableToMapDef, type ClassifiedBlindEntry } from '../calibrationWorkbenchController';

export interface BlindScanTabProps {
  busy: boolean;
  includeCurves: boolean;
  include8bit: boolean;
  include32bit: boolean;
  maxTables: number;
  blindResults: ClassifiedBlindEntry[];
  onSetIncludeCurves: (v: boolean) => void;
  onSetInclude8bit: (v: boolean) => void;
  onSetInclude32bit: (v: boolean) => void;
  onSetMaxTables: (v: number) => void;
  onRunScan: () => void;
  onAddMaps: (newMaps: MapDef[], selectFirst?: boolean) => void;
  onSelectMap: (mapId: string) => void;
  onInfo: (msg: string) => void;
}

export const BlindScanTab: React.FC<BlindScanTabProps> = ({
  busy,
  includeCurves,
  include8bit,
  include32bit,
  maxTables,
  blindResults,
  onSetIncludeCurves,
  onSetInclude8bit,
  onSetInclude32bit,
  onSetMaxTables,
  onRunScan,
  onAddMaps,
  onSelectMap,
  onInfo,
}) => (
  <div className="space-y-3">
    <div className="flex flex-wrap items-center gap-3 rounded border border-kiviBorder bg-kiviSurface p-3">
      <label className="flex items-center gap-1.5 text-kiviSecondary">
        <input
          type="checkbox"
          checked={includeCurves}
          onChange={(e) => onSetIncludeCurves(e.target.checked)}
        />
        <span>1D Curves</span>
      </label>
      <label className="flex items-center gap-1.5 text-kiviSecondary">
        <input
          type="checkbox"
          checked={include8bit}
          onChange={(e) => onSetInclude8bit(e.target.checked)}
        />
        <span>8-Bit Tables</span>
      </label>
      <label className="flex items-center gap-1.5 text-kiviSecondary">
        <input
          type="checkbox"
          checked={include32bit}
          onChange={(e) => onSetInclude32bit(e.target.checked)}
        />
        <span>32-Bit Tables</span>
      </label>
      <label className="flex items-center gap-1.5 text-kiviSecondary">
        <span>Max Tables</span>
        <input
          type="number"
          min={1}
          max={512}
          value={maxTables}
          onChange={(e) => onSetMaxTables(Number(e.target.value) || 64)}
          className="w-16 rounded border border-kiviBorder bg-kiviBg px-2 py-0.5 font-mono text-kiviInk"
        />
      </label>
      <button
        type="button"
        disabled={busy}
        onClick={onRunScan}
        className="rounded bg-kiviAccent/20 px-3 py-1 font-medium text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
      >
        {busy ? 'Scanning...' : 'Run Auto-Scan + Classifier'}
      </button>
      {blindResults.length > 0 && (
        <button
          type="button"
          onClick={() => {
            onAddMaps(
              blindResults.map((entry, i) => blindTableToMapDef(entry, i)),
              true
            );
            onInfo(`Added ${blindResults.length} auto-detected map(s) to Map Explorer folders.`);
          }}
          className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-kiviAccent hover:bg-kiviAccent/15"
        >
          Add All ({blindResults.length})
        </button>
      )}
    </div>

    {blindResults.length === 0 ? (
      <p className="text-kiviMuted">
        Run Auto-Scan + Classifier on a loaded ROM to detect non-overlapping 2D/1D tables and group
        them into structured Map Explorer folders.
      </p>
    ) : (
      <div className="overflow-x-auto rounded border border-kiviBorder">
        <table className="w-full text-left font-mono text-[11px]">
          <thead className="bg-kiviSurface text-kiviMuted">
            <tr>
              <th className="px-2 py-1.5">Offset</th>
              <th className="px-2 py-1.5">Shape</th>
              <th className="px-2 py-1.5">Type</th>
              <th className="px-2 py-1.5">Classification Proposal</th>
              <th className="px-2 py-1.5">Action</th>
            </tr>
          </thead>
          <tbody className="text-kiviInk">
            {blindResults.map((entry, idx) => {
              const t = entry.table;
              const dtypeStr =
                t.is_float && t.cell_bits === 32
                  ? 'f32'
                  : `${t.is_signed ? 'i' : 'u'}${t.cell_bits}`;
              const dimKind = t.is_curve || t.rows <= 1 ? '1D' : '2D';
              return (
              <tr key={idx} className="border-t border-kiviBorder">
                <td className="px-2 py-1.5 text-kiviAccent">
                  0x{t.address.toString(16).toUpperCase()}
                </td>
                <td className="px-2 py-1.5">
                  {t.rows}x{t.cols}{' '}
                  <span className="ml-1 rounded bg-kiviElevated px-1 py-0.5 text-[9px] text-kiviMuted">
                    {dimKind}
                  </span>
                </td>
                <td className="px-2 py-1.5">
                  {dtypeStr} {t.little_endian ? 'LE' : 'BE'}
                  {t.detector_source && (
                    <div className="text-[9px] text-kiviMuted">{t.detector_source}</div>
                  )}
                </td>
                <td className="px-2 py-1.5">
                  {entry.classification ? (
                    <div>
                      <span className="rounded border border-kiviWarn/40 bg-kiviWarn/15 px-1.5 py-0.5 text-[10px] text-kiviWarn">
                        {entry.classification.role_name}
                      </span>{' '}
                      <span className="text-kiviMuted">({entry.classification.unit})</span>
                    </div>
                  ) : (
                    <span className="text-kiviMuted">{t.label}</span>
                  )}
                </td>
                <td className="px-2 py-1.5">
                  <button
                    type="button"
                    onClick={() => {
                      const m = blindTableToMapDef(entry, idx);
                      onAddMaps([m], true);
                      onSelectMap(m.id);
                    }}
                    className="rounded border border-kiviBorderStrong bg-kiviElevated px-2 py-0.5 text-[10px] text-kiviInk hover:border-kiviAccent"
                  >
                    Open Map
                  </button>
                </td>
              </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    )}
  </div>
);
