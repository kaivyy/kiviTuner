import React, { useEffect, useRef, useState } from 'react';
import type { ChecksumEngineInfo, DetectedChecksumMatch } from '../../../lib/workbenchBridge';
import {
  bridgeAutoDetectOemChecksums,
  bridgeListOemChecksumEngines,
  bridgeRepairCustomChecksumPlugin,
  bridgeRepairEdc17Crc32c,
  bridgeRepairOemChecksum,
  bridgeRepairXdfChecksums,
  bridgeSampleCustomChecksumPlugin,
  bridgeSweepBlindChecksums,
  bridgeSweepBlindChecksumsStream,
  bridgeVerifyCustomChecksumPlugin,
  bridgeVerifyEdc17Crc32c,
  bridgeVerifyXdfChecksums,
  type CustomChecksumReport,
  type OemAlgorithmInfo,
  type OemChecksumReport,
  type OwnedTriCoreCrcBlockSpec,
  type SweepMatch,
  type XdfChecksumResult,
} from '../../../lib/bridge/ecuToolsBridge';
import type { ScanProgressEvent } from '../../../lib/bridge/windowBridge';
import { useMapStore } from '../../../stores/useMapStore';

export interface ChecksumsTabProps {
  busy: boolean;
  rom: Uint8Array | null;
  engines: ChecksumEngineInfo[];
  checksumMatches: DetectedChecksumMatch[];
  onDetectChecksums: () => void;
  onRepairChecksum: (
    algorithmId: string,
    mode: 'UpdateChecksum' | 'CompensateKeepCrc'
  ) => void;
  onRomUpdated: (nextRom: Uint8Array, checksumOk?: boolean) => Promise<void> | void;
  onInfo: (msg: string) => void;
}

export const ChecksumsTab: React.FC<ChecksumsTabProps> = ({
  busy,
  rom,
  engines,
  checksumMatches,
  onDetectChecksums,
  onRepairChecksum,
  onRomUpdated,
  onInfo,
}) => {
  const romLoaded = useMapStore((s) => s.romLoaded);
  const refreshRomSession = useMapStore((s) => s.refreshRomSession);
  const hasRom = romLoaded || Boolean(rom && rom.length > 0);
  const [oemEngines, setOemEngines] = useState<OemAlgorithmInfo[]>([]);
  const [oemReports, setOemReports] = useState<OemChecksumReport[]>([]);
  const [sweepMatches, setSweepMatches] = useState<SweepMatch[]>([]);
  const [sweepProgress, setSweepProgress] = useState<ScanProgressEvent | null>(null);
  const [xdfXml, setXdfXml] = useState('');
  const [xdfResults, setXdfResults] = useState<XdfChecksumResult[]>([]);
  const [customPluginJson, setCustomPluginJson] = useState('');
  const [customPluginReport, setCustomPluginReport] = useState<CustomChecksumReport | null>(null);
  const [edc17Label, setEdc17Label] = useState('TriCore Main Block');
  const [edc17StartHex, setEdc17StartHex] = useState('0x10000');
  const [edc17EndHex, setEdc17EndHex] = useState('0x1FFFB');
  const [edc17StoreHex, setEdc17StoreHex] = useState('0x1FFFC');
  const [edc17LittleEndian, setEdc17LittleEndian] = useState(true);
  const [edc17Report, setEdc17Report] = useState<OemChecksumReport | null>(null);
  const [localBusy, setLocalBusy] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const opSeqRef = useRef(0);

  useEffect(() => {
    void bridgeListOemChecksumEngines().then((list) => {
      if (list) setOemEngines(list);
    });
  }, []);

  const runLocal = async (fn: (isStale: () => boolean) => Promise<void>) => {
    const seq = ++opSeqRef.current;
    const isStale = () => seq !== opSeqRef.current;
    setLocalBusy(true);
    setLocalError(null);
    try {
      await fn(isStale);
    } catch (err) {
      if (!isStale()) setLocalError(err instanceof Error ? err.message : String(err));
    } finally {
      if (!isStale()) setLocalBusy(false);
    }
  };

  const handleDetectOem = () =>
    runLocal(async (isStale) => {
      if (!hasRom) {
        setLocalError('Load a ROM binary first.');
        return;
      }
      const reports = await bridgeAutoDetectOemChecksums(rom);
      if (isStale()) return;
      if (!reports) {
        setLocalError('Requires the kiviTuner desktop application.');
        return;
      }
      setOemReports(reports);
      onInfo(`OEM Checksum Scan matched ${reports.length} algorithm(s).`);
    });

  const handleRepairOem = (algorithmId: string) =>
    runLocal(async (isStale) => {
      if (!hasRom) return;
      const applyWorkspace = !rom && romLoaded;
      const res = await bridgeRepairOemChecksum(algorithmId, rom, applyWorkspace);
      if (isStale()) return;
      if (!res) {
        setLocalError('Requires the kiviTuner desktop application.');
        return;
      }
      if (applyWorkspace) {
        await refreshRomSession();
        if (isStale()) return;
        useMapStore.getState().setChecksumStatus(res.report.all_ok ? 'valid' : 'invalid');
      } else if (res.repaired_bytes) {
        await onRomUpdated(new Uint8Array(res.repaired_bytes), res.report.all_ok);
        if (isStale()) return;
      }
      setOemReports((prev) =>
        prev.map((r) => (r.algorithm_id === algorithmId ? res.report : r))
      );
      onInfo(`Repaired ${res.report.algorithm_name} (${res.report.all_ok ? 'ALL OK' : 'CHECK'}).`);
    });

  const handleBlindSweep = () =>
    runLocal(async (isStale) => {
      if (!hasRom) {
        setLocalError('Load a ROM binary first.');
        return;
      }
      setSweepProgress({ stage: 'starting', percent: 5, message: 'Running blind checksum sweep...' });
      const hits =
        !rom && romLoaded
          ? await bridgeSweepBlindChecksumsStream((ev) => {
              if (!isStale()) setSweepProgress(ev);
            })
          : await bridgeSweepBlindChecksums(rom);
      if (isStale()) return;
      setSweepProgress(null);
      if (!hits) {
        setLocalError('Requires the kiviTuner desktop application.');
        return;
      }
      setSweepMatches(hits);
      onInfo(`Blind checksum sweep found ${hits.length} candidate(s).`);
    });

  const handleEdc17 = (repair: boolean) =>
    runLocal(async (isStale) => {
      if (!hasRom) {
        setLocalError('Load a ROM binary first.');
        return;
      }
      const start = Number.parseInt(edc17StartHex.replace(/^0x/i, ''), 16);
      const end_incl = Number.parseInt(edc17EndHex.replace(/^0x/i, ''), 16);
      const store_offset = Number.parseInt(edc17StoreHex.replace(/^0x/i, ''), 16);
      if (![start, end_incl, store_offset].every((n) => Number.isInteger(n) && n >= 0)) {
        setLocalError('Enter valid hex offsets for EDC17 start, end, and store.');
        return;
      }
      const specs: OwnedTriCoreCrcBlockSpec[] = [
        {
          label: edc17Label.trim() || 'TriCore CRC-32C',
          start,
          end_incl,
          store_offset,
          little_endian: edc17LittleEndian,
        },
      ];
      if (repair) {
        const applyWorkspace = !rom && romLoaded;
        const res = await bridgeRepairEdc17Crc32c(specs, rom, applyWorkspace);
        if (isStale()) return;
        if (!res) {
          setLocalError('Requires the kiviTuner desktop application.');
          return;
        }
        setEdc17Report(res.report);
        if (applyWorkspace) {
          await refreshRomSession();
          if (isStale()) return;
        } else if (res.repaired_bytes) {
          await onRomUpdated(new Uint8Array(res.repaired_bytes), res.report.all_ok);
          if (isStale()) return;
        }
        onInfo(`EDC17 CRC-32C repaired (${res.report.all_ok ? 'ALL OK' : 'CHECK'}).`);
      } else {
        const rep = await bridgeVerifyEdc17Crc32c(specs, rom);
        if (isStale()) return;
        if (!rep) {
          setLocalError('Requires the kiviTuner desktop application.');
          return;
        }
        setEdc17Report(rep);
        onInfo(`EDC17 CRC-32C verified (${rep.all_ok ? 'VALID' : 'STALE'}).`);
      }
    });

  const handleXdfChecksums = (repair: boolean) =>
    runLocal(async () => {
      if (!hasRom || !xdfXml.trim()) {
        setLocalError('Load a ROM and paste/load an XDF XML containing <XDFCHECKSUM>.');
        return;
      }
      if (repair) {
        const applyWorkspace = !rom && romLoaded;
        const res = await bridgeRepairXdfChecksums(xdfXml, rom, applyWorkspace);
        if (!res) {
          setLocalError('Requires the kiviTuner desktop application.');
          return;
        }
        setXdfResults(res.results);
        if (applyWorkspace) {
          await refreshRomSession();
        } else if (res.repaired_bytes) {
          await onRomUpdated(
            new Uint8Array(res.repaired_bytes),
            res.results.every((r) => r.ok)
          );
        }
        onInfo(`Repaired ${res.results.length} XDF checksum(s).`);
      } else {
        const res = await bridgeVerifyXdfChecksums(xdfXml, rom);
        if (!res) {
          setLocalError('Requires the kiviTuner desktop application.');
          return;
        }
        setXdfResults(res);
        onInfo(`Verified ${res.length} XDF checksum(s).`);
      }
    });

  const handleLoadSampleCustomPlugin = () =>
    runLocal(async (isStale) => {
      const sample = await bridgeSampleCustomChecksumPlugin();
      if (isStale()) return;
      if (!sample) {
        setLocalError('Requires the kiviTuner desktop application.');
        return;
      }
      setCustomPluginJson(sample);
      setCustomPluginReport(null);
    });

  const handleCustomPlugin = (repair: boolean) =>
    runLocal(async (isStale) => {
      if (!hasRom || !customPluginJson.trim()) {
        setLocalError('Load a ROM binary and paste or load a custom checksum JSON plugin.');
        return;
      }
      if (repair) {
        const applyWorkspace = !rom && romLoaded;
        const res = await bridgeRepairCustomChecksumPlugin(customPluginJson, rom, applyWorkspace);
        if (isStale()) return;
        if (!res) {
          setLocalError('Requires the kiviTuner desktop application.');
          return;
        }
        setCustomPluginReport(res.report);
        if (applyWorkspace) {
          await refreshRomSession();
        } else if (res.repaired_bytes) {
          await onRomUpdated(new Uint8Array(res.repaired_bytes), res.report.all_valid);
        }
        onInfo(
          `Custom plugin '${res.report.plugin_name}': ${res.report.all_valid ? 'REPAIRED' : 'CHECK'}.`
        );
      } else {
        const rep = await bridgeVerifyCustomChecksumPlugin(customPluginJson, rom);
        if (isStale()) return;
        if (!rep) {
          setLocalError('Requires the kiviTuner desktop application.');
          return;
        }
        setCustomPluginReport(rep);
        onInfo(
          `Custom plugin '${rep.plugin_name}': ${rep.all_valid ? 'ALL VALID' : 'STALE'}.`
        );
      }
    });

  const isBusy = busy || localBusy;

  return (
    <div className="space-y-3">
      {localError && (
        <div className="rounded border border-kiviRed/40 bg-kiviRed/10 px-3 py-2 text-[11px] text-kiviRed">
          {localError}
        </div>
      )}

      <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="text-kiviSecondary">
          Registered Engines: <span className="font-mono">{engines.length}</span> Moto /{' '}
          <span className="font-mono">{oemEngines.length}</span> Total OEM
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            disabled={isBusy}
            onClick={onDetectChecksums}
            className="rounded bg-kiviAccent/20 px-3 py-1 font-medium text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
          >
            {isBusy ? 'Inspecting...' : 'Auto-Detect Moto'}
          </button>
          <button
            type="button"
            disabled={isBusy}
            onClick={() => void handleDetectOem()}
            className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-3 py-1 font-medium text-kiviGreen disabled:opacity-50"
          >
            Auto-Detect All OEM (22+)
          </button>
          <button
            type="button"
            disabled={isBusy}
            onClick={() => void handleBlindSweep()}
            className="rounded border border-kiviAccent/40 bg-kiviAccent/10 px-3 py-1 font-medium text-kiviAccent disabled:opacity-50"
          >
            Blind Checksum Sweep
          </button>
        </div>
      </div>

      {oemReports.length > 0 && (
        <div className="space-y-2">
          {oemReports.map((rep) => (
            <div
              key={`${rep.algorithm_id}-${rep.variant}`}
              className="space-y-1.5 rounded border border-kiviBorder bg-kiviSurface p-3"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <span className="font-semibold text-kiviInk">{rep.algorithm_name}</span>{' '}
                  <span className="text-[10px] text-kiviMuted">({rep.family} / {rep.variant})</span>{' '}
                  <span
                    className={`rounded px-1.5 py-0.5 font-mono text-[10px] ${
                      rep.all_ok
                        ? 'border border-kiviGreen/40 bg-kiviGreen/10 text-kiviGreen'
                        : 'border border-kiviRed/40 bg-kiviRed/10 text-kiviRed'
                    }`}
                  >
                    {rep.all_ok ? 'VALID' : 'NEEDS REPAIR'}
                  </span>
                </div>
                <button
                  type="button"
                  disabled={isBusy}
                  onClick={() => void handleRepairOem(rep.algorithm_id)}
                  className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2.5 py-1 text-[11px] text-kiviGreen"
                >
                  Repair OEM Checksum
                </button>
              </div>
              <div className="space-y-1 font-mono text-[10px]">
                {rep.blocks.map((blk, i) => (
                  <div key={i} className="flex justify-between border-t border-kiviBorder pt-1">
                    <span className="text-kiviSecondary">{blk.label}</span>
                    <span className={blk.ok ? 'text-kiviGreen' : 'text-kiviRed'}>
                      {blk.stored !== null ? `0x${blk.stored.toString(16).toUpperCase()}` : '-'} /{' '}
                      {blk.computed !== null ? `0x${blk.computed.toString(16).toUpperCase()}` : '-'}{' '}
                      ({blk.status})
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {sweepMatches.length > 0 && (
        <div className="rounded border border-kiviAccent/40 bg-kiviBg p-2.5 font-mono text-[10px]">
          <div className="mb-1 font-semibold text-kiviAccent">
            Blind Checksum Sweep Matches ({sweepMatches.length})
          </div>
          {sweepMatches.slice(0, 10).map((s, i) => (
            <div key={i} className="border-t border-kiviBorder py-1 text-kiviSecondary">
              {s.algo} | region={s.region} | store={s.store_location} ({s.store_le ? 'LE' : 'BE'}) |
              pages={s.pages_matched}/{s.pages_total}
            </div>
          ))}
        </div>
      )}

      {checksumMatches.length > 0 && (
        <div className="space-y-2">
          {checksumMatches.map((m) => (
            <div
              key={m.algorithm_id}
              className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <span className="font-semibold text-kiviInk">{m.algorithm_name}</span>{' '}
                  <span
                    className={`rounded px-1.5 py-0.5 font-mono text-[10px] ${
                      m.all_ok
                        ? 'border border-kiviGreen/40 bg-kiviGreen/10 text-kiviGreen'
                        : 'border border-kiviRed/40 bg-kiviRed/10 text-kiviRed'
                    }`}
                  >
                    {m.all_ok ? 'VALID' : 'NEEDS REPAIR'}
                  </span>
                  <p className="mt-0.5 text-[11px] text-kiviMuted">{m.notes}</p>
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() => onRepairChecksum(m.algorithm_id, 'UpdateChecksum')}
                    className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2.5 py-1 text-[11px] text-kiviGreen"
                  >
                    Repair (Update Checksum)
                  </button>
                  {m.supports_keep_crc_mode && (
                    <button
                      type="button"
                      disabled={isBusy}
                      onClick={() => onRepairChecksum(m.algorithm_id, 'CompensateKeepCrc')}
                      className="rounded border border-kiviAccent/40 bg-kiviAccent/10 px-2.5 py-1 text-[11px] text-kiviAccent"
                    >
                      Repair (GF(2) Keep-CRC)
                    </button>
                  )}
                </div>
              </div>
              <div className="space-y-1 font-mono text-[10px]">
                {m.blocks.map((blk, bi) => (
                  <div key={bi} className="flex justify-between border-t border-kiviBorder pt-1">
                    <span className="text-kiviSecondary">{blk.label}</span>
                    <span className={blk.ok ? 'text-kiviGreen' : 'text-kiviRed'}>
                      stored 0x{blk.stored.toString(16).toUpperCase()} / computed 0x
                      {blk.computed.toString(16).toUpperCase()}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Bosch EDC17 TriCore CRC-32C Castagnoli Block Verifier & Repair */}
      <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="font-semibold text-kiviInk">Bosch EDC17 TriCore CRC-32C</span>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={isBusy}
              onClick={() => void handleEdc17(false)}
              className="rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-[11px] text-kiviInk"
            >
              Verify CRC-32C
            </button>
            <button
              type="button"
              disabled={isBusy}
              onClick={() => void handleEdc17(true)}
              className="rounded border border-kiviGreen/40 bg-kiviGreen/15 px-2.5 py-1 text-[11px] text-kiviGreen"
            >
              Repair CRC-32C
            </button>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2 md:grid-cols-5 font-mono text-[10px]">
          <input
            type="text"
            aria-label="EDC17 block label"
            value={edc17Label}
            onChange={(e) => setEdc17Label(e.target.value)}
            className="rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1 text-kiviInk"
          />
          <input
            type="text"
            aria-label="EDC17 start hex"
            placeholder="Start hex"
            value={edc17StartHex}
            onChange={(e) => setEdc17StartHex(e.target.value)}
            className="rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1 text-kiviInk"
          />
          <input
            type="text"
            aria-label="EDC17 end hex"
            placeholder="End incl hex"
            value={edc17EndHex}
            onChange={(e) => setEdc17EndHex(e.target.value)}
            className="rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1 text-kiviInk"
          />
          <input
            type="text"
            aria-label="EDC17 store offset hex"
            placeholder="Store offset hex"
            value={edc17StoreHex}
            onChange={(e) => setEdc17StoreHex(e.target.value)}
            className="rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1 text-kiviInk"
          />
          <label className="flex items-center gap-1.5 text-kiviSecondary">
            <input
              type="checkbox"
              checked={edc17LittleEndian}
              onChange={(e) => setEdc17LittleEndian(e.target.checked)}
            />
            <span>Little Endian</span>
          </label>
        </div>
        {edc17Report && (
          <div className="space-y-1 font-mono text-[10px]">
            {edc17Report.blocks.map((blk, idx) => (
              <div key={idx} className="flex justify-between border-t border-kiviBorder pt-1">
                <span className="text-kiviSecondary">{blk.label}</span>
                <span className={blk.ok ? 'text-kiviGreen' : 'text-kiviRed'}>
                  stored {blk.stored !== null ? `0x${blk.stored.toString(16).toUpperCase()}` : '-'} /
                  computed {blk.computed !== null ? `0x${blk.computed.toString(16).toUpperCase()}` : '-'} ({blk.status})
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* XDF Checksum Engine */}
      <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="font-semibold text-kiviInk">XDF &lt;XDFCHECKSUM&gt; Engine</span>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={isBusy}
              onClick={() => void handleXdfChecksums(false)}
              className="rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-[11px] text-kiviInk"
            >
              Verify XDF Checksums
            </button>
            <button
              type="button"
              disabled={isBusy}
              onClick={() => void handleXdfChecksums(true)}
              className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2.5 py-1 text-[11px] text-kiviGreen"
            >
              Repair XDF Checksums
            </button>
          </div>
        </div>
        <textarea
          rows={3}
          placeholder="Paste XDF XML containing <XDFCHECKSUM> definitions..."
          value={xdfXml}
          onChange={(e) => setXdfXml(e.target.value)}
          className="w-full rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[10px] text-kiviInk"
        />
        {xdfResults.length > 0 && (
          <div className="space-y-1 font-mono text-[10px]">
            {xdfResults.map((r) => (
              <div key={r.id} className="flex justify-between border-t border-kiviBorder pt-1">
                <span className="text-kiviSecondary">
                  {r.name} @0x{r.store_address.toString(16).toUpperCase()}
                </span>
                <span className={r.ok ? 'text-kiviGreen' : 'text-kiviRed'}>
                  stored 0x{r.stored.toString(16).toUpperCase()} / computed 0x
                  {r.computed.toString(16).toUpperCase()} ({r.ok ? 'OK' : 'STALE'})
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Custom JSON Checksum Plugin Engine */}
      <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="font-semibold text-kiviInk">Custom JSON Checksum Plugin</span>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              disabled={isBusy}
              onClick={() => void handleLoadSampleCustomPlugin()}
              className="rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-[11px] text-kiviSecondary hover:text-kiviInk"
            >
              Load Example
            </button>
            <button
              type="button"
              disabled={isBusy}
              onClick={() => void handleCustomPlugin(false)}
              className="rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-[11px] text-kiviInk"
            >
              Verify Plugin
            </button>
            <button
              type="button"
              disabled={isBusy}
              onClick={() => void handleCustomPlugin(true)}
              className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2.5 py-1 text-[11px] text-kiviGreen"
            >
              Repair Plugin
            </button>
          </div>
        </div>
        <textarea
          rows={3}
          placeholder="Paste declarative custom checksum plugin JSON (Sum, TwosComplement, CRC16, CRC32)..."
          value={customPluginJson}
          onChange={(e) => setCustomPluginJson(e.target.value)}
          className="w-full rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[10px] text-kiviInk"
        />
        {customPluginReport && (
          <div className="space-y-1 font-mono text-[10px] tabular-nums">
            {customPluginReport.blocks.map((blk, idx) => (
              <div key={idx} className="flex justify-between border-t border-kiviBorder pt-1">
                <span className="text-kiviSecondary">
                  {blk.name} @0x{blk.store_offset.toString(16).toUpperCase()}
                </span>
                <span className={blk.valid ? 'text-kiviGreen' : 'text-kiviRed'}>
                  stored {blk.stored_hex} / computed {blk.computed_hex} (
                  {blk.valid ? 'OK' : 'STALE'})
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {sweepProgress && (
        <div className="space-y-1 rounded border border-kiviAccent/30 bg-kiviSurface p-2 font-mono text-[10px] text-kiviSecondary tabular-nums">
          <div className="flex justify-between">
            <span>{sweepProgress.message}</span>
            <span>{sweepProgress.percent}%</span>
          </div>
          <div className="h-1 w-full overflow-hidden rounded bg-kiviBorder">
            <div
              className="h-full bg-kiviAccent"
              style={{ width: `${Math.min(100, Math.max(0, sweepProgress.percent))}%` }}
            />
          </div>
        </div>
      )}

      {oemEngines.length > 0 && (
        <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
          {oemEngines.map((eng) => (
            <div key={eng.id} className="rounded border border-kiviBorder bg-kiviBg p-2.5">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-kiviInk">{eng.name}</span>
                <span className="font-mono text-[10px] text-kiviMuted">{eng.id}</span>
              </div>
              <p className="mt-1 text-[11px] text-kiviMuted">{eng.description}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
