import React, { useState } from 'react';
import { Download, Upload } from 'lucide-react';
import type { MapDef } from '../../../types/kivi';
import type {
  A2lImportSummary,
  DamosImportSummary,
  PatchDef,
  XdfFullDefinitions,
} from '../../../lib/workbenchBridge';
import {
  bridgeExportKpAr,
  bridgeExportEcuIni,
  bridgeExportXdfXml,
  bridgeParseKpFile,
  bridgeParseOlsProject,
  bridgeParseEcuXml,
  bridgeParseEcuIni,
  type DefinitionImportSummary,
  type OlsProjectSummary,
  type EcuIniSummary,
} from '../../../lib/bridge/ecuToolsBridge';
import { a2lEntryToMapDef, damosEntryToMapDef } from '../calibrationWorkbenchController';
import { useMapStore } from '../../../stores/useMapStore';

export type DefFormat = 'a2l' | 'damos' | 'xdf' | 'ecu_xml' | 'ini' | 'kp' | 'ols';

export interface DefinitionsTabProps {
  busy: boolean;
  rom: Uint8Array | null;
  defFormat: DefFormat;
  defText: string;
  baseOffsetText: string;
  baseOffset: number;
  defFilter: string;
  a2lSummary: A2lImportSummary | null;
  damosSummary: DamosImportSummary | null;
  xdfSummary: XdfFullDefinitions | null;
  onSetDefFormat: (fmt: DefFormat) => void;
  onSetDefText: (txt: string) => void;
  onSetBaseOffsetText: (txt: string) => void;
  onSetDefFilter: (txt: string) => void;
  onParseDef: () => void;
  onTogglePatch: (patch: PatchDef, revert: boolean) => void;
  onAddMaps: (newMaps: MapDef[], selectFirst?: boolean) => void;
  onInfo: (msg: string) => void;
}

export const DefinitionsTab: React.FC<DefinitionsTabProps> = ({
  busy,
  rom,
  defFormat,
  defText,
  baseOffsetText,
  baseOffset,
  defFilter,
  a2lSummary,
  damosSummary,
  xdfSummary,
  onSetDefFormat,
  onSetDefText,
  onSetBaseOffsetText,
  onSetDefFilter,
  onParseDef,
  onTogglePatch,
  onAddMaps,
  onInfo,
}) => {
  const filterLower = defFilter.trim().toLowerCase();
  const romLoaded = useMapStore((s) => s.romLoaded);
  const storeMaps = useMapStore((s) => s.maps);
  const hasRom = romLoaded || Boolean(rom && rom.length > 0);
  const [binaryFileBytes, setBinaryFileBytes] = useState<Uint8Array | null>(null);
  const [extraDefSummary, setExtraDefSummary] = useState<DefinitionImportSummary | null>(null);
  const [iniSummary, setIniSummary] = useState<EcuIniSummary | null>(null);
  const [olsSummary, setOlsSummary] = useState<OlsProjectSummary | null>(null);
  const [extraError, setExtraError] = useState<string | null>(null);
  const [extraBusy, setExtraBusy] = useState(false);

  const isExtendedFormat =
    defFormat === 'ecu_xml' ||
    defFormat === 'ini' ||
    defFormat === 'kp' ||
    defFormat === 'ols';

  const handleParseAction = async () => {
    if (!isExtendedFormat) {
      onParseDef();
      return;
    }
    setExtraBusy(true);
    setExtraError(null);
    try {
      if (defFormat === 'ecu_xml') {
        const res = await bridgeParseEcuXml(defText, false);
        if (!res) {
          setExtraError('Requires the kiviTuner desktop application.');
          return;
        }
        setExtraDefSummary(res);
        setIniSummary(null);
        setOlsSummary(null);
        onInfo(`Parsed ECU XML (${res.label}): ${res.table_count} tables.`);
      } else if (defFormat === 'ini') {
        const res = await bridgeParseEcuIni(defText, [], false);
        if (!res) {
          setExtraError('Requires the kiviTuner desktop application.');
          return;
        }
        setIniSummary(res);
        setExtraDefSummary(null);
        setOlsSummary(null);
        onInfo(`Parsed ECU INI (${res.signature}): ${res.table_count} tables.`);
      } else if (defFormat === 'kp') {
        const bytes = binaryFileBytes ?? new TextEncoder().encode(defText);
        const res = await bridgeParseKpFile(bytes, false);
        if (!res) {
          setExtraError('Requires the kiviTuner desktop application.');
          return;
        }
        setExtraDefSummary(res);
        setIniSummary(null);
        setOlsSummary(null);
        onInfo(`Parsed KP map pack (${res.format}): ${res.table_count} maps.`);
      } else if (defFormat === 'ols') {
        if (!binaryFileBytes) {
          setExtraError('Load a binary .ols project file first.');
          return;
        }
        const res = await bridgeParseOlsProject(binaryFileBytes);
        if (!res) {
          setExtraError('Requires the kiviTuner desktop application.');
          return;
        }
        setOlsSummary(res);
        setExtraDefSummary(null);
        setIniSummary(null);
        onInfo(`Parsed .ols project (${res.make} ${res.model}): ${res.maps.length} maps.`);
      }
    } catch (err) {
      setExtraError(err instanceof Error ? err.message : String(err));
    } finally {
      setExtraBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      {extraError && (
        <div className="rounded border border-kiviRed/40 bg-kiviRed/10 px-3 py-1.5 text-[11px] text-kiviRed">
          {extraError}
        </div>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviBorder bg-kiviSurface px-3 py-2 text-[11px]">
        <span className="text-kiviSecondary">Export Active Definitions ({storeMaps.length} maps)</span>
        <div className="flex flex-wrap items-center gap-1.5">
          {(['xdf', 'ini', 'kp'] as const).map((fmt) => (
            <button
              key={fmt}
              type="button"
              disabled={extraBusy || storeMaps.length === 0}
              onClick={() =>
                void (async () => {
                  const text =
                    fmt === 'xdf'
                      ? await bridgeExportXdfXml('kiviTuner Calibration Export', storeMaps)
                      : fmt === 'ini'
                      ? await bridgeExportEcuIni('kiviTuner Export 1.0', undefined, storeMaps)
                      : await bridgeExportKpAr('kiviTuner', storeMaps);
                  if (!text) {
                    setExtraError('Requires the kiviTuner desktop application.');
                    return;
                  }
                  const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
                  const url = URL.createObjectURL(blob);
                  const a = document.createElement('a');
                  a.href = url;
                  a.download =
                    fmt === 'xdf'
                      ? 'kivituner_export.xdf'
                      : fmt === 'ini'
                      ? 'kivituner_export.ini'
                      : 'kivituner_export.kp.txt';
                  a.click();
                  URL.revokeObjectURL(url);
                  onInfo(`Exported ${storeMaps.length} map definitions as ${fmt.toUpperCase()}.`);
                })()
              }
              className="flex items-center gap-1 rounded border border-kiviBorderStrong bg-kiviElevated px-2 py-1 text-[10px] font-medium text-kiviInk hover:border-kiviAccent disabled:opacity-40"
            >
              <Download className="h-3 w-3 text-kiviAccent" />
              <span>Export .{fmt}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 rounded border border-kiviBorder bg-kiviSurface p-3">
        <select
          aria-label="Definition format"
          value={defFormat}
          onChange={(e) => onSetDefFormat(e.target.value as DefFormat)}
          className="rounded border border-kiviBorder bg-kiviBg px-2 py-1 text-kiviInk"
        >
          <option value="a2l">ASAP2 / A2L</option>
          <option value="damos">DAMOS / OLS (.dam)</option>
          <option value="xdf">XDF Full (+Patches &amp; Checksums)</option>
          <option value="ecu_xml">ECU Definition XML</option>
          <option value="ini">Speeduino / MegaSquirt (.ini)</option>
          <option value="kp">KP Map-Pack (.kp binary / text)</option>
          <option value="ols">OLS Project (.ols binary)</option>
        </select>

        <label className="flex cursor-pointer items-center gap-1 rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-kiviInk hover:border-kiviBorderStrong">
          <Upload className="h-3.5 w-3.5" />
          <span>Load File</span>
          <input
            type="file"
            accept=".a2l,.dam,.ols,.kp,.xdf,.xml,.ini,.txt"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (!file) return;
              void file.arrayBuffer().then((buf) => {
                const bytes = new Uint8Array(buf);
                setBinaryFileBytes(bytes);
                if (!file.name.toLowerCase().endsWith('.ols') && !file.name.toLowerCase().endsWith('.kp')) {
                  onSetDefText(new TextDecoder().decode(bytes));
                } else {
                  onSetDefText(`[Binary file loaded: ${file.name} (${bytes.length} bytes)]`);
                }
              });
            }}
          />
        </label>

        <label className="flex items-center gap-1 text-kiviMuted">
          <span>Base Offset</span>
          <input
            type="text"
            value={baseOffsetText}
            onChange={(e) => onSetBaseOffsetText(e.target.value)}
            className="w-28 rounded border border-kiviBorder bg-kiviBg px-2 py-1 font-mono text-kiviInk"
          />
        </label>

        <button
          type="button"
          disabled={busy || extraBusy}
          onClick={() => void handleParseAction()}
          className="rounded bg-kiviAccent/20 px-3 py-1 font-medium text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
        >
          {busy || extraBusy ? 'Parsing...' : 'Parse Definition'}
        </button>
      </div>

      <textarea
        rows={4}
        aria-label="Definition text input"
        placeholder="Load a file above or paste A2L / DAMOS / XDF content..."
        value={defText}
        onChange={(e) => onSetDefText(e.target.value)}
        className="w-full rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[11px] text-kiviInk"
      />

      {(a2lSummary || damosSummary || xdfSummary) && (
        <input
          type="text"
          placeholder="Filter parsed entries by name..."
          value={defFilter}
          onChange={(e) => onSetDefFilter(e.target.value)}
          className="w-full rounded border border-kiviBorder bg-kiviBg px-2.5 py-1 font-mono text-[11px] text-kiviInk"
        />
      )}

      {a2lSummary && (
        <div className="space-y-2">
          <div className="flex flex-wrap items-center justify-between gap-2 text-kiviSecondary">
            <span>
              {a2lSummary.characteristic_count} entries | Suggested ROM base:{' '}
              {a2lSummary.ecu_base_suggestions.length > 0
                ? a2lSummary.ecu_base_suggestions
                    .map((a) => `0x${a.toString(16).toUpperCase()}`)
                    .join(', ')
                : 'None'}
            </span>
            <button
              type="button"
              onClick={() => {
                const imported = a2lSummary.entries
                  .filter((e) => e.name.toLowerCase().includes(filterLower))
                  .slice(0, 200)
                  .map((e) => a2lEntryToMapDef(e, baseOffset));
                onAddMaps(imported, true);
                onInfo(`Imported ${imported.length} A2L maps into sidebar.`);
              }}
              className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-kiviAccent"
            >
              Import Filtered (max 200)
            </button>
          </div>
          <div className="max-h-56 overflow-y-auto rounded border border-kiviBorder">
            <table className="w-full text-left font-mono text-[10px]">
              <thead className="bg-kiviSurface text-kiviMuted">
                <tr>
                  <th className="px-2 py-1">Name</th>
                  <th className="px-2 py-1">Kind</th>
                  <th className="px-2 py-1">Addr (Offset)</th>
                  <th className="px-2 py-1">Size</th>
                  <th className="px-2 py-1">Unit</th>
                </tr>
              </thead>
              <tbody className="text-kiviInk">
                {a2lSummary.entries
                  .filter((e) => e.name.toLowerCase().includes(filterLower))
                  .slice(0, 100)
                  .map((entry, idx) => (
                    <tr key={idx} className="border-t border-kiviBorder">
                      <td className="px-2 py-1">{entry.name}</td>
                      <td className="px-2 py-1">{entry.kind}</td>
                      <td className="px-2 py-1 text-kiviAccent">
                        0x
                        {(entry.address >= baseOffset
                          ? entry.address - baseOffset
                          : entry.address
                        )
                          .toString(16)
                          .toUpperCase()}
                      </td>
                      <td className="px-2 py-1">
                        {entry.rows}x{entry.cols}
                      </td>
                      <td className="px-2 py-1">{entry.unit || '-'}</td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {damosSummary && (
        <div className="space-y-2">
          <div className="flex flex-wrap items-center justify-between gap-2 text-kiviSecondary">
            <span>
              ECU: {damosSummary.ecu ?? 'Unknown'} | SPZ: {damosSummary.spz_total_count} | UMP:{' '}
              {damosSummary.ump_count}
            </span>
            <button
              type="button"
              onClick={() => {
                const imported = damosSummary.entries
                  .filter((e) => e.name.toLowerCase().includes(filterLower))
                  .slice(0, 200)
                  .map((e) => damosEntryToMapDef(e, baseOffset));
                onAddMaps(imported, true);
                onInfo(`Imported ${imported.length} DAMOS maps into sidebar.`);
              }}
              className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-kiviAccent"
            >
              Import Filtered (max 200)
            </button>
          </div>
          <div className="max-h-56 overflow-y-auto rounded border border-kiviBorder">
            <table className="w-full text-left font-mono text-[10px]">
              <thead className="bg-kiviSurface text-kiviMuted">
                <tr>
                  <th className="px-2 py-1">Name</th>
                  <th className="px-2 py-1">Source</th>
                  <th className="px-2 py-1">Offset</th>
                  <th className="px-2 py-1">Shape</th>
                </tr>
              </thead>
              <tbody className="text-kiviInk">
                {damosSummary.entries
                  .filter((e) => e.name.toLowerCase().includes(filterLower))
                  .slice(0, 100)
                  .map((entry, idx) => (
                    <tr key={idx} className="border-t border-kiviBorder">
                      <td className="px-2 py-1">{entry.name}</td>
                      <td className="px-2 py-1">{entry.source}</td>
                      <td className="px-2 py-1 text-kiviAccent">
                        0x
                        {(entry.address >= baseOffset
                          ? entry.address - baseOffset
                          : entry.address
                        )
                          .toString(16)
                          .toUpperCase()}
                      </td>
                      <td className="px-2 py-1">
                        {entry.rows}x{entry.cols}
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {xdfSummary && (
        <div className="space-y-3">
          <div className="flex items-center justify-between text-kiviSecondary">
            <span>
              XDF Full: {xdfSummary.maps.length} maps, {xdfSummary.constants.length} constants,{' '}
              {xdfSummary.flags.length} flags, {xdfSummary.patches.length} patches,{' '}
              {xdfSummary.checksums.length} checksums
            </span>
            {xdfSummary.maps.length > 0 && (
              <button
                type="button"
                onClick={() => onAddMaps(xdfSummary.maps, true)}
                className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-kiviAccent"
              >
                Load Maps ({xdfSummary.maps.length})
              </button>
            )}
          </div>

          {xdfSummary.patches.length > 0 && (
            <div className="rounded border border-kiviBorder">
              <div className="border-b border-kiviBorder bg-kiviSurface px-3 py-1.5 font-semibold text-kiviInk">
                XDF Patches ({xdfSummary.patches.length})
              </div>
              <div className="max-h-48 overflow-y-auto">
                <table className="w-full text-left font-mono text-[10px]">
                  <thead className="bg-kiviSurface text-kiviMuted">
                    <tr>
                      <th className="px-2 py-1">Patch</th>
                      <th className="px-2 py-1">Category</th>
                      <th className="px-2 py-1">Entries</th>
                      <th className="px-2 py-1">Action</th>
                    </tr>
                  </thead>
                  <tbody className="text-kiviInk">
                    {xdfSummary.patches
                      .filter((p) => p.name.toLowerCase().includes(filterLower))
                      .map((patch) => (
                        <tr key={patch.id} className="border-t border-kiviBorder">
                          <td className="px-2 py-1">{patch.name}</td>
                          <td className="px-2 py-1">{patch.category}</td>
                          <td className="px-2 py-1">{patch.entries.length}</td>
                          <td className="px-2 py-1">
                            <div className="flex gap-1">
                              <button
                                type="button"
                                disabled={busy || !hasRom}
                                onClick={() => onTogglePatch(patch, false)}
                                className="rounded border border-kiviGreen/40 bg-kiviGreen/10 px-2 py-0.5 text-kiviGreen disabled:opacity-40"
                              >
                                Apply
                              </button>
                              <button
                                type="button"
                                disabled={busy || !hasRom}
                                onClick={() => onTogglePatch(patch, true)}
                                className="rounded border border-kiviBorderStrong bg-kiviElevated px-2 py-0.5 text-kiviSecondary disabled:opacity-40"
                              >
                                Revert
                              </button>
                            </div>
                          </td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {xdfSummary.checksums.length > 0 && (
            <div className="rounded border border-kiviBorder p-2 font-mono text-[10px] text-kiviSecondary">
              <div className="mb-1 font-semibold text-kiviInk">
                XDF Checksum Definitions ({xdfSummary.checksums.length})
              </div>
              {xdfSummary.checksums.map((cs) => (
                <div key={cs.id} className="border-t border-kiviBorder py-1">
                  {cs.name}: 0x{cs.start_address.toString(16).toUpperCase()}..0x
                  {cs.end_address.toString(16).toUpperCase()} -&gt; store @ 0x
                  {cs.store_address.toString(16).toUpperCase()} ({cs.data_bits}b data /{' '}
                  {cs.sum_bits}b sum, method {cs.calculation_method || 'default'})
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {extraDefSummary && (
        <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
          <div className="flex items-center justify-between text-kiviSecondary">
            <span>
              {extraDefSummary.format} ({extraDefSummary.label}): {extraDefSummary.table_count} tables,{' '}
              {extraDefSummary.constant_count} constants, {extraDefSummary.flag_count} flags
            </span>
            {extraDefSummary.maps.length > 0 && (
              <button
                type="button"
                onClick={() => {
                  onAddMaps(extraDefSummary.maps, true);
                  onInfo(`Imported ${extraDefSummary.maps.length} maps.`);
                }}
                className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-kiviAccent"
              >
                Import Maps ({extraDefSummary.maps.length})
              </button>
            )}
          </div>
        </div>
      )}

      {iniSummary && (
        <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3">
          <div className="flex items-center justify-between text-kiviSecondary">
            <span>
              ECU INI ({iniSummary.signature || 'unnamed'}): {iniSummary.table_count} tables,{' '}
              {iniSummary.constant_count} constants, pages [{iniSummary.page_sizes.join(', ')}]
            </span>
            {iniSummary.maps.length > 0 && (
              <button
                type="button"
                onClick={() => {
                  onAddMaps(iniSummary.maps, true);
                  onInfo(`Imported ${iniSummary.maps.length} ECU INI tables.`);
                }}
                className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-kiviAccent"
              >
                Import Tables ({iniSummary.maps.length})
              </button>
            )}
          </div>
        </div>
      )}

      {olsSummary && (
        <div className="space-y-2 rounded border border-kiviBorder bg-kiviSurface p-3 font-mono text-[10px] text-kiviSecondary">
          <div>
            OLS v{olsSummary.format_version} | {olsSummary.make} {olsSummary.model} | ECU:{' '}
            {olsSummary.ecu_name} | HW: {olsSummary.hw_number} | SW: {olsSummary.sw_number}
          </div>
          <div>
            Versions: {olsSummary.versions.length} | Maps: {olsSummary.maps.length}
          </div>
        </div>
      )}
    </div>
  );
};
