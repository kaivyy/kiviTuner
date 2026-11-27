import React, { useState } from 'react';
import { FileCode2, Play, Sparkles } from 'lucide-react';
import {
  bridgeApplyCalibrationScriptWorkspace,
  bridgeDryRunCalibrationScript,
  bridgeSampleCalibrationScript,
  type CalibrationScriptDryRun,
} from '../../../lib/bridge/ecuToolsBridge';
import { useMapStore } from '../../../stores/useMapStore';

export interface CalibrationScriptCardProps {
  rom: Uint8Array | null;
  onInfo?: (msg: string) => void;
}

export const CalibrationScriptCard: React.FC<CalibrationScriptCardProps> = ({ rom, onInfo }) => {
  const romLoaded = useMapStore((s) => s.romLoaded);
  const refreshRomSession = useMapStore((s) => s.refreshRomSession);
  const [scriptJson, setScriptJson] = useState('');
  const [dryRun, setDryRun] = useState<CalibrationScriptDryRun | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const hasRom = romLoaded || Boolean(rom && rom.length > 0);

  const handleLoadSample = async () => {
    setError(null);
    const sample = await bridgeSampleCalibrationScript();
    if (!sample) {
      setError('Requires the kiviTuner desktop application.');
      return;
    }
    setScriptJson(sample);
    setDryRun(null);
  };

  const handleDryRun = async () => {
    if (!hasRom || !scriptJson.trim()) {
      setError('Load a ROM binary and paste or load a .kivi-script.json definition.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const report = await bridgeDryRunCalibrationScript(
        scriptJson,
        !romLoaded && rom ? rom : null
      );
      if (!report) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setDryRun(report);
      onInfo?.(
        report.can_apply
          ? `Dry-run passed: ${report.total_bytes_changed} byte(s) ready across ${report.rules.length} rule(s).`
          : 'Dry-run reported blocking signature mismatches or ambiguity.'
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleApplyWorkspace = async () => {
    if (!romLoaded) {
      setError('Open the ROM in the desktop workspace before applying an undoable script patch.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const res = await bridgeApplyCalibrationScriptWorkspace(scriptJson);
      if (!res) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setDryRun(res.dry_run);
      await refreshRomSession();
      onInfo?.(
        `Applied script '${res.dry_run.script_name}' atomically (${res.dry_run.total_bytes_changed} bytes changed, Undo ready).`
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
          <FileCode2 className="h-3.5 w-3.5 text-kiviAccent" />
          <span>Pattern Calibration &amp; Auto Map-Transfer Script (.kivi-script.json)</span>
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleLoadSample()}
            className="inline-flex items-center gap-1 rounded border border-kiviBorder/70 bg-kiviElevated px-2 py-1 text-[11px] text-kiviSecondary hover:text-kiviInk"
          >
            <Sparkles className="h-3 w-3 text-kiviAccent" />
            <span>Load Example</span>
          </button>
          <label className="cursor-pointer rounded border border-kiviBorder/70 bg-kiviElevated px-2 py-1 text-[11px] text-kiviSecondary hover:text-kiviInk">
            <span>Import JSON</span>
            <input
              type="file"
              accept=".json,.kivi-script.json"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                void file.text().then((text) => {
                  setScriptJson(text);
                  setDryRun(null);
                  setError(null);
                });
              }}
            />
          </label>
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleDryRun()}
            className="inline-flex items-center gap-1 rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-[11px] font-medium text-kiviAccent hover:bg-kiviAccent/25 disabled:opacity-50"
          >
            <Play className="h-3 w-3" />
            <span>Dry-Run Preview</span>
          </button>
          <button
            type="button"
            disabled={busy || !dryRun?.can_apply || !romLoaded}
            onClick={() => void handleApplyWorkspace()}
            className="rounded border border-kiviGreen/40 bg-kiviGreen/15 px-2.5 py-1 text-[11px] font-medium text-kiviGreen hover:bg-kiviGreen/25 disabled:opacity-40"
          >
            Apply Atomically (Undoable)
          </button>
        </div>
      </div>

      <textarea
        rows={4}
        placeholder="Paste or load declarative signature-anchored calibration script JSON..."
        value={scriptJson}
        onChange={(e) => {
          setScriptJson(e.target.value);
          setDryRun(null);
        }}
        className="w-full rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[10px] text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent"
      />

      {error && (
        <div className="rounded border border-kiviRed/40 bg-kiviRed/10 px-2.5 py-1.5 text-[11px] text-kiviRed">
          {error}
        </div>
      )}

      {dryRun && (
        <div className="overflow-x-auto rounded border border-kiviBorder bg-kiviBg">
          <table className="w-full text-left font-mono text-[10px] tabular-nums">
            <thead className="bg-kiviElevated text-kiviMuted">
              <tr>
                <th className="px-2 py-1">Rule</th>
                <th className="px-2 py-1">Status</th>
                <th className="px-2 py-1">Target Offset</th>
                <th className="px-2 py-1">Delta</th>
                <th className="px-2 py-1">Detail</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-kiviBorder text-kiviSecondary">
              {dryRun.rules.map((r, i) => (
                <tr key={i}>
                  <td className="px-2 py-1 font-semibold text-kiviInk">{r.rule_name}</td>
                  <td className="px-2 py-1">
                    <span
                      className={
                        r.status === 'Ready' ? 'text-kiviGreen' : 'text-kiviRed'
                      }
                    >
                      {r.status}
                    </span>
                  </td>
                  <td className="px-2 py-1 text-kiviAccent">
                    {r.target_offset !== null
                      ? `0x${r.target_offset.toString(16).toUpperCase()}`
                      : '-'}
                  </td>
                  <td className="px-2 py-1">{r.bytes_changed} B</td>
                  <td className="px-2 py-1">{r.detail}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
};
