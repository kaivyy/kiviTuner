import React, { useEffect, useState } from 'react';
import { Cpu, KeyRound, ShieldAlert, Search, FileCode2, ShieldCheck, Usb } from 'lucide-react';
import {
  bridgeCheckMotoWriteBoundaries,
  bridgeComputeOemSeedKey,
  bridgeExportIntelHex,
  bridgeExportSrec,
  bridgeFingerprintEcuRom,
  bridgeListMotoEcuProfiles,
  bridgeListSeedKeyAlgorithms,
  bridgeMatchStockRomRegistry,
  bridgeOstrichProbe,
  bridgeOstrichUploadActiveRom,
  bridgeOstrichVerifyActiveRom,
  bridgeScanDensoShDescriptors,
  bridgeScanDtcTables,
} from '../../../lib/bridge/ecuToolsBridge';
import type {
  DensoDescriptorMapHit,
  DtcTableCandidate,
  EcuFingerprint,
  MotoEcuProfile,
  OstrichTransferSummary,
  OstrichVerifyReport,
  OstrichVersion,
  SeedKeyEngineInfo,
  SeedKeyResult,
  StockRomMatchReport,
} from '../../../lib/bridge/ecuToolsBridge';
import { useMapStore } from '../../../stores/useMapStore';

export interface EcuToolsTabProps {
  rom: Uint8Array | null;
  onInfo: (msg: string) => void;
}

export const EcuToolsTab: React.FC<EcuToolsTabProps> = ({ rom, onInfo }) => {
  const romLoaded = useMapStore((s) => s.romLoaded);
  const hasRom = romLoaded || Boolean(rom && rom.length > 0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [fingerprint, setFingerprint] = useState<EcuFingerprint | null>(null);
  const [densoHits, setDensoHits] = useState<DensoDescriptorMapHit[]>([]);
  const [dtcTables, setDtcTables] = useState<DtcTableCandidate[]>([]);

  const [seedAlgos, setSeedAlgos] = useState<SeedKeyEngineInfo[]>([]);
  const [selectedAlgo, setSelectedAlgo] = useState('gm_vpw');
  const [seedHex, setSeedHex] = useState('1234');
  const [algoIndex, setAlgoIndex] = useState('0');
  const [extraHex, setExtraHex] = useState('');
  const [seedKeyResult, setSeedKeyResult] = useState<SeedKeyResult | null>(null);

  const [motoProfiles, setMotoProfiles] = useState<MotoEcuProfile[]>([]);
  const [motoQuery, setMotoQuery] = useState('');
  const [selectedProfileId, setSelectedProfileId] = useState('');
  const [writeStartHex, setWriteStartHex] = useState('0x1800');
  const [writeLenHex, setWriteLenHex] = useState('0x1000');
  const [boundaryStatus, setBoundaryStatus] = useState<string | null>(null);

  const [exportBaseHex, setExportBaseHex] = useState('0x0');
  const [exportedPreview, setExportedPreview] = useState<string | null>(null);

  const [stockPackJson, setStockPackJson] = useState('');
  const [stockReport, setStockReport] = useState<StockRomMatchReport | null>(null);

  const [ostrichPort, setOstrichPort] = useState('/dev/ttyUSB0');
  const [ostrichBank, setOstrichBank] = useState(0);
  const [ostrichBaseHex, setOstrichBaseHex] = useState('0x0000');
  const [ostrichVersion, setOstrichVersion] = useState<OstrichVersion | null>(null);
  const [ostrichTx, setOstrichTx] = useState<OstrichTransferSummary | null>(null);
  const [ostrichVerify, setOstrichVerify] = useState<OstrichVerifyReport | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [algos, profiles] = await Promise.all([
          bridgeListSeedKeyAlgorithms(),
          bridgeListMotoEcuProfiles(),
        ]);
        if (algos && algos.length > 0) {
          setSeedAlgos(algos);
          setSelectedAlgo(algos[0].id);
        }
        if (profiles && profiles.length > 0) {
          setMotoProfiles(profiles);
          setSelectedProfileId(profiles[0].id);
        }
      } catch (err) {
        setError(String(err));
      }
    })();
  }, []);

  const runAction = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleFingerprint = () =>
    runAction(async () => {
      if (!hasRom) {
        setError('Load a ROM binary first.');
        return;
      }
      const fp = await bridgeFingerprintEcuRom(rom);
      if (!fp) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setFingerprint(fp);
      onInfo(`Identified ${fp.manufacturer} ${fp.family} (${Math.round(fp.confidence * 100)}%).`);
    });

  const handleScanDensoAndDtc = () =>
    runAction(async () => {
      if (!hasRom) {
        setError('Load a ROM binary first.');
        return;
      }
      const [sh, dtcs] = await Promise.all([
        bridgeScanDensoShDescriptors(rom),
        bridgeScanDtcTables(6, rom),
      ]);
      if (!sh || !dtcs) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setDensoHits(sh);
      setDtcTables(dtcs);
      onInfo(`Found ${sh.length} Denso SH descriptor maps and ${dtcs.length} DTC table candidates.`);
    });

  const handleSolveSeedKey = () =>
    runAction(async () => {
      const idx = Number.parseInt(algoIndex, 10) || 0;
      const res = await bridgeComputeOemSeedKey(selectedAlgo, seedHex, idx, extraHex);
      if (!res) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setSeedKeyResult(res);
      onInfo(`Computed ${res.algorithm_id} key: 0x${res.key_hex}`);
    });

  const handleCheckBoundary = () =>
    runAction(async () => {
      const start = Number.parseInt(writeStartHex.replace(/^0x/i, ''), 16);
      const len = Number.parseInt(writeLenHex.replace(/^0x/i, ''), 16);
      if (Number.isNaN(start) || Number.isNaN(len)) {
        setError('Enter valid hexadecimal start address and length.');
        return;
      }
      const err = await bridgeCheckMotoWriteBoundaries(selectedProfileId, start, len);
      setBoundaryStatus(err ? `BLOCKED: ${err}` : 'SAFE: write range is within writable calibration bounds.');
    });

  const handleExportFormat = (kind: 'ihex' | 'srec') =>
    runAction(async () => {
      if (!hasRom) {
        setError('Load a ROM binary first.');
        return;
      }
      const base = Number.parseInt(exportBaseHex.replace(/^0x/i, ''), 16) || 0;
      const text =
        kind === 'ihex'
          ? await bridgeExportIntelHex(base, rom)
          : await bridgeExportSrec(base, 'KIVITUNER', rom);
      if (!text) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setExportedPreview(text.split('\n').slice(0, 12).join('\n'));
      onInfo(`Exported ${kind.toUpperCase()} (${text.length} chars).`);
    });

  const handleCheckStockRegistry = () =>
    runAction(async () => {
      if (!hasRom) {
        setError('Load a ROM binary first.');
        return;
      }
      const report = await bridgeMatchStockRomRegistry(
        stockPackJson.trim() ? stockPackJson : null,
        rom
      );
      if (!report) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setStockReport(report);
      onInfo(`Stock check: ${report.integrity_status} (${report.sha256.slice(0, 12)}...).`);
    });

  const handleOstrichProbe = () =>
    runAction(async () => {
      if (!ostrichPort.trim()) {
        setError('Enter the Ostrich 2.0 serial port (e.g. /dev/ttyUSB0 or COM3).');
        return;
      }
      const ver = await bridgeOstrichProbe(ostrichPort.trim());
      if (!ver) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setOstrichVersion(ver);
      onInfo(`Connected Ostrich 2.0 emulator: ${ver.display}`);
    });

  const handleOstrichUpload = () =>
    runAction(async () => {
      if (!romLoaded) {
        setError('Load a ROM into the desktop workspace first.');
        return;
      }
      const base = Number.parseInt(ostrichBaseHex.replace(/^0x/i, ''), 16) || 0;
      const res = await bridgeOstrichUploadActiveRom(ostrichPort.trim(), ostrichBank, base);
      if (!res) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setOstrichTx(res);
      setOstrichVerify(null);
      onInfo(`Uploaded ${res.bytes_written} bytes to Ostrich bank ${res.bank}.`);
    });

  const handleOstrichVerify = () =>
    runAction(async () => {
      if (!romLoaded) {
        setError('Load a ROM into the desktop workspace first.');
        return;
      }
      const base = Number.parseInt(ostrichBaseHex.replace(/^0x/i, ''), 16) || 0;
      const rep = await bridgeOstrichVerifyActiveRom(ostrichPort.trim(), ostrichBank, base);
      if (!rep) {
        setError('Requires the kiviTuner desktop application.');
        return;
      }
      setOstrichVerify(rep);
      onInfo(
        rep.verified
          ? `Ostrich bank ${rep.bank} verified OK (${rep.bytes_checked} bytes).`
          : `Ostrich verify mismatch at offset 0x${(rep.first_mismatch_offset ?? 0).toString(16).toUpperCase()}.`
      );
    });

  const filteredMoto = motoProfiles.filter((p) => {
    const q = motoQuery.trim().toLowerCase();
    if (!q) return true;
    return (
      p.make.toLowerCase().includes(q) ||
      p.model.toLowerCase().includes(q) ||
      p.part_number.toLowerCase().includes(q) ||
      p.mcu.toLowerCase().includes(q)
    );
  });

  return (
    <div className="space-y-3">
      {error && (
        <div className="rounded border border-kiviRed/40 bg-kiviRed/10 px-3 py-2 text-[11px] text-kiviRed">
          {error}
        </div>
      )}

      {/* ROM Fingerprint, Denso Descriptors & DTC Tables */}
      <section className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
            <Cpu className="h-3.5 w-3.5 text-kiviAccent" />
            <span>ECU Fingerprint, Denso SH Descriptors &amp; DTC Tables</span>
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={() => void handleFingerprint()}
              className="rounded bg-kiviAccent/20 px-2.5 py-1 text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
            >
              Identify ROM
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => void handleScanDensoAndDtc()}
              className="flex items-center gap-1 rounded border border-kiviAccent/40 bg-kiviAccent/10 px-2.5 py-1 text-kiviAccent disabled:opacity-50"
            >
              <Search className="h-3 w-3" />
              Scan Denso SH &amp; DTCs
            </button>
          </div>
        </div>

        {fingerprint && (
          <div className="rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[10px] text-kiviSecondary space-y-1">
            <div>
              ECU: <span className="text-kiviInk">{fingerprint.manufacturer} {fingerprint.family}</span>{' '}
              {fingerprint.variant ? `(${fingerprint.variant})` : ''} | confidence{' '}
              {Math.round(fingerprint.confidence * 100)}%
            </div>
            {fingerprint.hw_numbers.length > 0 && <div>HW: {fingerprint.hw_numbers.join(', ')}</div>}
            {fingerprint.sw_numbers.length > 0 && <div>SW: {fingerprint.sw_numbers.join(', ')}</div>}
            {fingerprint.cal_ids.length > 0 && <div>CAL: {fingerprint.cal_ids.join(', ')}</div>}
          </div>
        )}

        {(densoHits.length > 0 || dtcTables.length > 0) && (
          <div className="grid grid-cols-1 gap-2 md:grid-cols-2 font-mono text-[10px]">
            <div className="rounded border border-kiviBorder bg-kiviBg p-2">
              <div className="font-semibold text-kiviInk mb-1">Denso SH Maps ({densoHits.length})</div>
              {densoHits.slice(0, 8).map((h, i) => (
                <div key={i} className="text-kiviMuted">
                  @0x{h.descriptor_offset.toString(16).toUpperCase()} -&gt; Z:0x{h.ptr_z.toString(16).toUpperCase()}{' '}
                  ({h.rows}x{h.cols}, {h.element_bytes * 8}b)
                </div>
              ))}
            </div>
            <div className="rounded border border-kiviBorder bg-kiviBg p-2">
              <div className="font-semibold text-kiviInk mb-1">DTC Tables ({dtcTables.length})</div>
              {dtcTables.slice(0, 6).map((t, i) => (
                <div key={i} className="text-kiviMuted">
                  @0x{t.code_table_offset.toString(16).toUpperCase()} ({t.entry_count} codes,{' '}
                  {t.big_endian ? 'BE' : 'LE'}):{' '}
                  {t.entries.slice(0, 4).map((e) => e.formatted_code).join(', ')}
                </div>
              ))}
            </div>
          </div>
        )}
      </section>

      {/* Unified OEM Seed/Key Calculator */}
      <section className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
          <KeyRound className="h-3.5 w-3.5 text-kiviAccent" />
          <span>OEM SecurityAccess Seed/Key Calculator ({seedAlgos.length} engines)</span>
        </div>
        <div className="grid grid-cols-1 gap-2 md:grid-cols-4">
          <select
            aria-label="Seed-Key algorithm"
            value={selectedAlgo}
            onChange={(e) => setSelectedAlgo(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
          >
            {seedAlgos.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
                {a.verified_for_live_hardware === false
                  ? ' [Bench/Simulated Only - Blocked on Live ECU]'
                  : ''}
              </option>
            ))}
          </select>
          <input
            type="text"
            aria-label="Seed hex"
            placeholder="Seed hex (e.g. 1234)"
            value={seedHex}
            onChange={(e) => setSeedHex(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
          <input
            type="text"
            aria-label="Algorithm index"
            placeholder="Algo idx / level"
            value={algoIndex}
            onChange={(e) => setAlgoIndex(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
          <input
            type="text"
            aria-label="SA2 bytecode or extra hex"
            placeholder="Extra / SA2 hex (optional)"
            value={extraHex}
            onChange={(e) => setExtraHex(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
        </div>
        {(() => {
          const cur = seedAlgos.find((a) => a.id === selectedAlgo);
          if (!cur || cur.verified_for_live_hardware !== false) return null;
          return (
            <p className="text-[10px] text-kiviWarn">
              [Bench/Simulated Only - Blocked on Live ECU] {cur.notes}
            </p>
          );
        })()}
        <div className="flex items-center gap-3">
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleSolveSeedKey()}
            className="rounded bg-kiviAccent/20 px-3 py-1 text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
          >
            Compute Key
          </button>
          {seedKeyResult && (
            <span className="font-mono text-kiviGreen">
              Key: 0x{seedKeyResult.key_hex} ({seedKeyResult.key_bytes.length} bytes)
            </span>
          )}
        </div>
      </section>

      {/* Motorcycle ECU Write-Boundary Safety Guard */}
      <section className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
          <ShieldAlert className="h-3.5 w-3.5 text-kiviWarn" />
          <span>Motorcycle ECU Flash Write-Boundary Guard ({motoProfiles.length} profiles)</span>
        </div>
        <div className="grid grid-cols-1 gap-2 md:grid-cols-4">
          <input
            type="text"
            placeholder="Filter make/model/MCU..."
            value={motoQuery}
            onChange={(e) => setMotoQuery(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
          />
          <select
            aria-label="Motorcycle ECU profile"
            value={selectedProfileId}
            onChange={(e) => setSelectedProfileId(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
          >
            {filteredMoto.map((p) => (
              <option key={p.id} value={p.id}>
                {p.make} {p.model} ({p.part_number})
              </option>
            ))}
          </select>
          <input
            type="text"
            aria-label="Write start hex"
            value={writeStartHex}
            onChange={(e) => setWriteStartHex(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
          <input
            type="text"
            aria-label="Write length hex"
            value={writeLenHex}
            onChange={(e) => setWriteLenHex(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
        </div>
        <div className="flex items-center gap-3">
          <button
            type="button"
            disabled={busy || !selectedProfileId}
            onClick={() => void handleCheckBoundary()}
            className="rounded border border-kiviWarn/40 bg-kiviWarn/10 px-3 py-1 text-kiviWarn disabled:opacity-50"
          >
            Check Write Range
          </button>
          {boundaryStatus && (
            <span
              className={`font-mono text-[11px] ${
                boundaryStatus.startsWith('SAFE') ? 'text-kiviGreen' : 'text-kiviRed'
              }`}
            >
              {boundaryStatus}
            </span>
          )}
        </div>
      </section>

      {/* Intel HEX / Motorola S-Record Export */}
      <section className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
            <FileCode2 className="h-3.5 w-3.5 text-kiviAccent" />
            <span>Intel HEX / Motorola SREC Export</span>
          </div>
          <div className="flex items-center gap-2">
            <input
              type="text"
              aria-label="Export base address hex"
              value={exportBaseHex}
              onChange={(e) => setExportBaseHex(e.target.value)}
              className="w-28 rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
            />
            <button
              type="button"
              disabled={busy}
              onClick={() => void handleExportFormat('ihex')}
              className="rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-kiviInk"
            >
              Export Intel HEX
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => void handleExportFormat('srec')}
              className="rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-kiviInk"
            >
              Export Motorola S37
            </button>
          </div>
        </div>
        {exportedPreview && (
          <pre className="max-h-36 overflow-auto rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[10px] text-kiviSecondary">
            {exportedPreview}
          </pre>
        )}
      </section>

      {/* Stock ROM SHA-256 Hash & Signature Registry Verification */}
      <section className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
            <ShieldCheck className="h-3.5 w-3.5 text-kiviGreen" />
            <span>Stock ROM Hash &amp; Signature Registry Check</span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <label className="cursor-pointer rounded border border-kiviBorderStrong bg-kiviElevated px-2 py-1 text-[11px] text-kiviSecondary hover:text-kiviInk">
              <span>Load Pack JSON</span>
              <input
                type="file"
                accept=".json"
                className="hidden"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (!file) return;
                  void file.text().then((txt) => setStockPackJson(txt));
                }}
              />
            </label>
            <button
              type="button"
              disabled={busy}
              onClick={() => void handleCheckStockRegistry()}
              className="rounded border border-kiviGreen/40 bg-kiviGreen/15 px-2.5 py-1 text-[11px] font-medium text-kiviGreen hover:bg-kiviGreen/25 disabled:opacity-50"
            >
              Verify Stock Registry
            </button>
          </div>
        </div>
        {stockReport && (
          <div className="space-y-1 rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[10px] tabular-nums">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-kiviSecondary">
                Status:{' '}
                <strong
                  className={
                    stockReport.integrity_status === 'ExactStockMatch'
                      ? 'text-kiviGreen'
                      : stockReport.integrity_status === 'FamilyMatchModifiedOrUnknown'
                      ? 'text-kiviWarn'
                      : 'text-kiviMuted'
                  }
                >
                  {stockReport.integrity_status}
                </strong>
              </span>
              <span className="text-kiviMuted">
                {stockReport.rom_size.toLocaleString()} bytes ({stockReport.pack_name})
              </span>
            </div>
            <div className="break-all text-kiviInk">SHA-256: {stockReport.sha256}</div>
            {stockReport.matched_entry && (
              <div className="text-kiviAccent">
                Entry: {stockReport.matched_entry.label} ({stockReport.matched_entry.sw_version})
              </div>
            )}
            {stockReport.anchored_maps.length > 0 && (
              <div className="pt-1 text-kiviGreen">
                Signature-Anchored Maps: {stockReport.anchored_maps.length} matched (
                {stockReport.anchored_maps
                  .slice(0, 4)
                  .map((m) => `${m.name}@0x${m.address.toString(16).toUpperCase()}`)
                  .join(', ')}
                )
              </div>
            )}
          </div>
        )}
      </section>

      {/* Ostrich 2.0 Real-Time EPROM Emulator */}
      <section className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
          <Usb className="h-3.5 w-3.5 text-kiviAccent" />
          <span>Ostrich 2.0 Real-Time EPROM Emulator</span>
        </div>
        <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
          <input
            type="text"
            aria-label="Ostrich serial port"
            placeholder="/dev/ttyUSB0 or COM3"
            value={ostrichPort}
            onChange={(e) => setOstrichPort(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
          <select
            aria-label="Ostrich bank"
            value={ostrichBank}
            onChange={(e) => setOstrichBank(Number(e.target.value))}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk tabular-nums"
          >
            {[0, 1, 2, 3, 4, 5, 6, 7].map((b) => (
              <option key={b} value={b}>
                Bank {b} (64 KB)
              </option>
            ))}
          </select>
          <input
            type="text"
            aria-label="Ostrich base address hex"
            placeholder="0x0000"
            value={ostrichBaseHex}
            onChange={(e) => setOstrichBaseHex(e.target.value)}
            className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleOstrichProbe()}
            className="rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-kiviInk disabled:opacity-50"
          >
            Probe Emulator
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleOstrichUpload()}
            className="rounded bg-kiviAccent/20 px-2.5 py-1 text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
          >
            Upload Active ROM
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleOstrichVerify()}
            className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2.5 py-1 text-kiviGreen disabled:opacity-50"
          >
            Verify Bank
          </button>
          {ostrichVersion && (
            <span className="font-mono text-[10px] text-kiviGreen tabular-nums">
              {ostrichVersion.display}
            </span>
          )}
          {ostrichTx && (
            <span className="font-mono text-[10px] text-kiviAccent tabular-nums">
              Uploaded {ostrichTx.bytes_written} B ({ostrichTx.chunks} chunks)
            </span>
          )}
          {ostrichVerify && (
            <span
              className={`font-mono text-[10px] tabular-nums ${
                ostrichVerify.verified ? 'text-kiviGreen' : 'text-kiviRed'
              }`}
            >
              {ostrichVerify.verified
                ? `Verified ${ostrichVerify.bytes_checked} B OK`
                : `Mismatch @ 0x${(ostrichVerify.first_mismatch_offset ?? 0)
                    .toString(16)
                    .toUpperCase()}`}
            </span>
          )}
        </div>
      </section>
    </div>
  );
};
