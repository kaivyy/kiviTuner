import React from 'react';
import type { MapDef } from '../../../types/kivi';
import type { SimilarMapHit } from '../../../lib/workbenchBridge';
import { similarHitToMapDef } from '../calibrationWorkbenchController';
import { CalibrationScriptCard } from './CalibrationScriptCard';

export interface SimilarityTabProps {
  busy: boolean;
  rom?: Uint8Array | null;
  simMode: 'in-rom' | 'cross-rom';
  simAddrText: string;
  simRows: number;
  simCols: number;
  simBits: number;
  simLittleEndian: boolean;
  minPearsonR: number;
  maxNrmse: number;
  targetRomName: string | null;
  simHits: SimilarMapHit[];
  onSetSimMode: (m: 'in-rom' | 'cross-rom') => void;
  onSetSimAddrText: (v: string) => void;
  onSetSimRows: (n: number) => void;
  onSetSimCols: (n: number) => void;
  onSetSimBits: (n: number) => void;
  onSetSimLittleEndian: (v: boolean) => void;
  onSetMinPearsonR: (v: number) => void;
  onSetMaxNrmse: (v: number) => void;
  onSelectTargetRom: (buf: Uint8Array, name: string) => void;
  onRunSimilarity: () => void;
  onAddMaps: (newMaps: MapDef[], selectFirst?: boolean) => void;
  onSelectMap: (mapId: string) => void;
  onInfo?: (msg: string) => void;
}

export const SimilarityTab: React.FC<SimilarityTabProps> = ({
  busy,
  rom = null,
  simMode,
  simAddrText,
  simRows,
  simCols,
  simBits,
  simLittleEndian,
  minPearsonR,
  maxNrmse,
  targetRomName,
  simHits,
  onSetSimMode,
  onSetSimAddrText,
  onSetSimRows,
  onSetSimCols,
  onSetSimBits,
  onSetSimLittleEndian,
  onSetMinPearsonR,
  onSetMaxNrmse,
  onSelectTargetRom,
  onRunSimilarity,
  onAddMaps,
  onSelectMap,
  onInfo,
}) => (
  <div className="space-y-3">
    <div className="grid grid-cols-2 gap-2 rounded border border-kiviBorder bg-kiviSurface p-3 md:grid-cols-4">
      <label className="flex flex-col gap-1 text-kiviMuted">
        <span>Mode</span>
        <select
          value={simMode}
          onChange={(e) => onSetSimMode(e.target.value as 'in-rom' | 'cross-rom')}
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
        >
          <option value="in-rom">Find Similar in ROM</option>
          <option value="cross-rom">Cross-ROM Migration</option>
        </select>
      </label>
      <label className="flex flex-col gap-1 text-kiviMuted">
        <span>Source Address</span>
        <input
          type="text"
          value={simAddrText}
          onChange={(e) => onSetSimAddrText(e.target.value)}
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
        />
      </label>
      <label className="flex flex-col gap-1 text-kiviMuted">
        <span>Rows x Cols</span>
        <div className="flex gap-1">
          <input
            type="number"
            min={1}
            value={simRows}
            onChange={(e) => onSetSimRows(Number(e.target.value) || 1)}
            className="w-1/2 rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
          <input
            type="number"
            min={1}
            value={simCols}
            onChange={(e) => onSetSimCols(Number(e.target.value) || 1)}
            className="w-1/2 rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
        </div>
      </label>
      <label className="flex flex-col gap-1 text-kiviMuted">
        <span>Width / Endian</span>
        <div className="flex items-center gap-2">
          <select
            value={simBits}
            onChange={(e) => onSetSimBits(Number(e.target.value))}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
          >
            <option value={8}>8-bit</option>
            <option value={16}>16-bit</option>
            <option value={32}>32-bit</option>
          </select>
          <label className="flex items-center gap-1 text-kiviSecondary">
            <input
              type="checkbox"
              checked={simLittleEndian}
              onChange={(e) => onSetSimLittleEndian(e.target.checked)}
            />
            <span>LE</span>
          </label>
        </div>
      </label>
      <label className="flex flex-col gap-1 text-kiviMuted">
        <span>Min Pearson r</span>
        <input
          type="number"
          step="0.01"
          min={0}
          max={1}
          value={minPearsonR}
          onChange={(e) => onSetMinPearsonR(Number(e.target.value))}
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
        />
      </label>
      <label className="flex flex-col gap-1 text-kiviMuted">
        <span>Max NRMSE</span>
        <input
          type="number"
          step="0.01"
          min={0}
          value={maxNrmse}
          onChange={(e) => onSetMaxNrmse(Number(e.target.value))}
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
        />
      </label>
      {simMode === 'cross-rom' && (
        <label className="col-span-2 flex flex-col gap-1 text-kiviMuted">
          <span>Target ROM ({targetRomName ?? 'none selected'})</span>
          <input
            type="file"
            accept=".bin,.rom"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (!file) return;
              void file.arrayBuffer().then((buf) => {
                onSelectTargetRom(new Uint8Array(buf), file.name);
              });
            }}
            className="text-[11px] text-kiviSecondary"
          />
        </label>
      )}
    </div>

    <button
      type="button"
      disabled={busy}
      onClick={onRunSimilarity}
      className="rounded bg-kiviAccent/20 px-3 py-1.5 font-medium text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
    >
      {busy ? 'Matching...' : simMode === 'cross-rom' ? 'Find Migrated Map' : 'Find Similar Maps'}
    </button>

    {simHits.length > 0 && (
      <div className="overflow-x-auto rounded border border-kiviBorder">
        <table className="w-full text-left font-mono text-[11px]">
          <thead className="bg-kiviSurface text-kiviMuted">
            <tr>
              <th className="px-2 py-1.5">Address</th>
              <th className="px-2 py-1.5">Pearson r</th>
              <th className="px-2 py-1.5">NRMSE</th>
              <th className="px-2 py-1.5">Exact Copy</th>
              <th className="px-2 py-1.5">Action</th>
            </tr>
          </thead>
          <tbody className="text-kiviInk">
            {simHits.map((hit, i) => (
              <tr key={i} className="border-t border-kiviBorder">
                <td className="px-2 py-1 text-kiviAccent">
                  0x{hit.address.toString(16).toUpperCase()}
                </td>
                <td className="px-2 py-1">{hit.pearson_r.toFixed(4)}</td>
                <td className="px-2 py-1">{hit.nrmse.toFixed(4)}</td>
                <td className="px-2 py-1">{hit.is_exact_copy ? 'YES' : 'NO'}</td>
                <td className="px-2 py-1">
                  {simMode === 'in-rom' && (
                    <button
                      type="button"
                      onClick={() => {
                        const m = similarHitToMapDef(hit, i, 'sim');
                        onAddMaps([m], true);
                        onSelectMap(m.id);
                      }}
                      className="rounded border border-kiviBorderStrong bg-kiviElevated px-2 py-0.5 text-[10px]"
                    >
                      Open Map
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )}

    <CalibrationScriptCard rom={rom} onInfo={onInfo} />
  </div>
);
