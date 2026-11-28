/**
 * Unified 15,752-entry Reference Super-Catalogue Panel.
 *
 * Distinguishes identity catalogues (ECU families, vehicles, memory chips,
 * reference modules) cleanly from reference capability rows so nothing implies
 * unverified flashing hardware support.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { BookOpen, Cpu, Search } from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { HorizontalScrollStrip } from '../layout/HorizontalScrollStrip';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import {
  MIN_QUERY_LEN,
  describeSuperCatalogueScope,
  runChipSearch,
  runEcuSearch,
  runFlasherCapabilitySearch,
  runJedecLookup,
  runReferenceModuleSearch,
  runSuperCatalogueStats,
  runVehicleSearch,
  summariseSearch,
  type CatalogueState,
} from './catalogueController';
import type {
  ChipCatalogueRow,
  EcuFamilyRow,
  FlasherCapabilityRow,
  ReferenceModuleRow,
  SuperCatalogueStats,
  VehicleModelRow,
} from '../../lib/catalogueBridge';

export interface CataloguePanelProps {
  isOpen: boolean;
  onClose: () => void;
  standalone?: boolean;
}

type Tab = 'ecu' | 'vehicle' | 'chip' | 'capability' | 'module';

function StateLine<T>({
  state,
  idleHint,
  render,
}: {
  state: CatalogueState<T> | null;
  idleHint: string;
  render: (d: T) => React.ReactNode;
}) {
  if (!state) return <p className="text-[11px] text-kiviMuted">{idleHint}</p>;
  if (state.kind === 'no-backend') {
    return <p className="text-[11px] text-kiviWarn">{state.message}</p>;
  }
  if (state.kind === 'error') return <p className="text-[11px] text-kiviRed">{state.message}</p>;
  return <div className="text-[11px] text-kiviInk">{render(state.data)}</div>;
}

function BoolFlag({ on, label }: { on: boolean; label: string }) {
  return (
    <span
      className={`rounded px-1.5 py-0.5 text-[10px] font-mono ${
        on
          ? 'bg-kiviGreen/10 text-kiviGreen border border-kiviGreen/40'
          : 'bg-kiviSurface text-kiviMuted border border-kiviBorder'
      }`}
    >
      {label}
    </span>
  );
}

const PLACEHOLDERS: Record<Tab, string> = {
  ecu: `At least ${MIN_QUERY_LEN} characters, e.g. BMS46, ME17, Keihin`,
  vehicle: `At least ${MIN_QUERY_LEN} characters, e.g. BMW, Honda, Civic`,
  chip: `At least ${MIN_QUERY_LEN} characters, e.g. 29F400, Winbond, Infineon`,
  capability: `At least ${MIN_QUERY_LEN} characters, e.g. Honda, Bosch, Toyota`,
  module: `At least ${MIN_QUERY_LEN} characters, e.g. Bosch, Denso, Honda`,
};

export const CataloguePanel: React.FC<CataloguePanelProps> = ({
  isOpen,
  onClose,
  standalone = false,
}) => {
  const [tab, setTab] = useState<Tab>('ecu');
  const [query, setQuery] = useState('');
  const [jedecMfr, setJedecMfr] = useState('EF');
  const [jedecDev, setJedecDev] = useState('4018');
  const [isLoading, setIsLoading] = useState(false);

  const [stats, setStats] = useState<CatalogueState<SuperCatalogueStats> | null>(null);
  const [ecuRows, setEcuRows] = useState<CatalogueState<EcuFamilyRow[]> | null>(null);
  const [vehicleRows, setVehicleRows] = useState<CatalogueState<VehicleModelRow[]> | null>(null);
  const [chipRows, setChipRows] = useState<CatalogueState<ChipCatalogueRow[]> | null>(null);
  const [capabilityRows, setCapabilityRows] =
    useState<CatalogueState<FlasherCapabilityRow[]> | null>(null);
  const [moduleRows, setModuleRows] = useState<CatalogueState<ReferenceModuleRow[]> | null>(null);
  const reqSeqRef = useRef(0);
  const statsSeqRef = useRef(0);

  useEffect(() => {
    if (!isOpen) return;
    const seq = ++statsSeqRef.current;
    void (async () => {
      const s = await runSuperCatalogueStats();
      if (seq === statsSeqRef.current) setStats(s);
    })();
  }, [isOpen]);

  const executeSearchForTab = useCallback(async (targetTab: Tab, rawQuery: string) => {
    const seq = ++reqSeqRef.current;
    setIsLoading(true);
    try {
      switch (targetTab) {
        case 'ecu': {
          const rows = await runEcuSearch(rawQuery);
          if (seq === reqSeqRef.current) setEcuRows(rows);
          break;
        }
        case 'vehicle': {
          const rows = await runVehicleSearch(rawQuery);
          if (seq === reqSeqRef.current) setVehicleRows(rows);
          break;
        }
        case 'chip': {
          const rows = await runChipSearch(rawQuery);
          if (seq === reqSeqRef.current) setChipRows(rows);
          break;
        }
        case 'capability': {
          const rows = await runFlasherCapabilitySearch(rawQuery);
          if (seq === reqSeqRef.current) setCapabilityRows(rows);
          break;
        }
        case 'module': {
          const rows = await runReferenceModuleSearch(rawQuery);
          if (seq === reqSeqRef.current) setModuleRows(rows);
          break;
        }
      }
    } finally {
      if (seq === reqSeqRef.current) setIsLoading(false);
    }
  }, []);

  const handleJedecLookup = useCallback(async () => {
    const seq = ++reqSeqRef.current;
    setIsLoading(true);
    try {
      const rows = await runJedecLookup(jedecMfr, jedecDev);
      if (seq === reqSeqRef.current) setChipRows(rows);
    } finally {
      if (seq === reqSeqRef.current) setIsLoading(false);
    }
  }, [jedecMfr, jedecDev]);

  const sData = stats?.kind === 'ok' ? stats.data : null;
  const tabs: Array<{ id: Tab; label: string; count?: number }> = [
    { id: 'ecu', label: 'ECU Families', count: sData?.ecu_family_count },
    { id: 'vehicle', label: 'Vehicles', count: sData?.vehicle_model_count },
    { id: 'chip', label: 'Memory Chips', count: sData?.chip_catalogue_count },
    { id: 'capability', label: 'Flasher Capabilities', count: sData?.flasher_capability_count },
    { id: 'module', label: 'Reference Modules', count: sData?.reference_module_count },
  ];
  const spec = TOOL_WINDOW_SPECS.catalogue;

  return (
    <FloatingWindow
      windowId="catalogue"
      isOpen={isOpen}
      onClose={onClose}
      title="Reference Super-Catalogue"
      badge={sData ? `${sData.total_entries.toLocaleString()} entries` : undefined}
      icon={<BookOpen className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="catalogue"
      titlebarTone="sidebar"
      closeLabel="Close reference catalogue"
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div className="shrink-0 border-b border-kiviBorder bg-kiviBg px-4 py-2">
          <StateLine
            state={stats}
            idleHint="Loading super-catalogue index..."
            render={(s) => <span className="text-kiviSecondary">{describeSuperCatalogueScope(s)}</span>}
          />
        </div>

        <HorizontalScrollStrip
          role="tablist"
          ariaLabel="Reference catalogue sections"
          stripTestId="catalogue-tab-strip"
          depKey={`${tab}:${sData?.total_entries ?? 0}`}
          activeSelector='[aria-selected="true"]'
          className="h-10 border-b border-kiviBorder bg-kiviBg px-2.5"
          stripClassName="gap-1"
          arrowSize="xs"
          scrollLeftLabel="Scroll catalogue tabs left"
          scrollRightLabel="Scroll catalogue tabs right"
        >
          {tabs.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setTab(t.id)}
              className={`flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-2.5 text-[11px] font-medium transition-colors ${
                tab === t.id
                  ? 'bg-kiviAccent/20 text-kiviAccent border border-kiviAccent/30'
                  : 'text-kiviMuted hover:text-kiviInk border border-transparent'
              }`}
            >
              <span>{t.label}</span>
              {t.count !== undefined && (
                <span className="font-mono text-[10px] opacity-80">
                  ({t.count.toLocaleString()})
                </span>
              )}
            </button>
          ))}
        </HorizontalScrollStrip>

        <div className="flex flex-wrap items-center gap-2 border-b border-kiviBorder px-3 py-2">
          <input
            type="text"
            aria-label="Catalogue search query"
            value={query}
            placeholder={PLACEHOLDERS[tab]}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void executeSearchForTab(tab, query);
            }}
            className="min-w-[160px] flex-1 rounded border border-kiviBorder bg-kiviBg px-2.5 py-1 font-mono text-[11px] text-kiviInk"
          />
          <button
            type="button"
            disabled={isLoading}
            onClick={() => void executeSearchForTab(tab, query)}
            className="flex min-w-[104px] shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded bg-kiviAccent/20 px-3 py-1 text-[11px] font-medium text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
          >
            <Search className="h-3 w-3" />
            <span>{isLoading ? 'Searching...' : 'Search'}</span>
          </button>

          {tab === 'chip' && (
            <div className="flex items-center gap-1.5 border-l border-kiviBorder pl-2">
              <Cpu className="h-3.5 w-3.5 text-kiviMuted" />
              <span className="text-[10px] text-kiviMuted">JEDEC Hex</span>
              <input
                type="text"
                aria-label="JEDEC manufacturer hex"
                value={jedecMfr}
                onChange={(e) => setJedecMfr(e.target.value)}
                placeholder="MFR"
                className="w-14 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
              />
              <input
                type="text"
                aria-label="JEDEC device hex"
                value={jedecDev}
                onChange={(e) => setJedecDev(e.target.value)}
                placeholder="DEV"
                className="w-16 rounded border border-kiviBorder bg-kiviBg px-1.5 py-1 font-mono text-[11px] text-kiviInk"
              />
              <button
                type="button"
                disabled={isLoading}
                onClick={() => void handleJedecLookup()}
                className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-[11px] font-medium text-kiviAccent hover:bg-kiviAccent/15 disabled:opacity-50"
              >
                Lookup JEDEC
              </button>
            </div>
          )}

          {tab === 'module' && (
            <div className="flex items-center gap-1 text-[10px]">
              {['Bosch', 'Honda', 'Denso', 'Toyota'].map((preset) => (
                <button
                  key={preset}
                  type="button"
                  onClick={() => {
                    setQuery(preset);
                    void executeSearchForTab('module', preset);
                  }}
                  className="rounded border border-kiviBorder/70 bg-kiviSurface px-2 py-0.5 text-kiviSecondary hover:border-kiviAccent/50"
                >
                  {preset}
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="flex-1 space-y-2 overflow-y-auto p-4">
          {tab === 'ecu' && (
            <StateLine
              state={ecuRows}
              idleHint="Type an ECU family or variant and search (e.g. BMS46, ME17, Keihin)."
              render={(list) =>
                list.length === 0 ? (
                  <span className="text-kiviMuted">{summariseSearch(0, 'ecu')}</span>
                ) : (
                  <>
                    <p className="mb-2 text-kiviMuted">
                      {summariseSearch(list.length, 'ecu')}
                      {list.length >= 500 && (
                        <span className="ml-1 text-kiviWarn">
                          Showing first {list.length} only - narrow the query.
                        </span>
                      )}
                    </p>
                    <table className="w-full text-left font-mono text-[10px]">
                      <thead className="text-kiviMuted">
                        <tr>
                          <th className="pr-2 py-1">Category</th>
                          <th className="pr-2 py-1">Manufacturer</th>
                          <th className="pr-2 py-1">Family</th>
                          <th className="py-1">Variant</th>
                        </tr>
                      </thead>
                      <tbody className="text-kiviInk">
                        {list.map((r, i) => (
                          <tr key={i} className="border-t border-kiviBorder">
                            <td className="pr-2 py-1">{r.category}</td>
                            <td className="pr-2 py-1">{r.manufacturer}</td>
                            <td className="pr-2 py-1">{r.family}</td>
                            <td className="py-1">{r.variant}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </>
                )
              }
            />
          )}

          {tab === 'vehicle' && (
            <StateLine
              state={vehicleRows}
              idleHint="Type a vehicle make, model, or engine and search (e.g. BMW, Honda)."
              render={(list) =>
                list.length === 0 ? (
                  <span className="text-kiviMuted">{summariseSearch(0, 'vehicle')}</span>
                ) : (
                  <>
                    <p className="mb-2 text-kiviMuted">
                      {summariseSearch(list.length, 'vehicle')}
                      {list.length >= 500 && (
                        <span className="ml-1 text-kiviWarn">
                          Showing first {list.length} only - narrow the query.
                        </span>
                      )}
                    </p>
                    <table className="w-full text-left font-mono text-[10px]">
                      <thead className="text-kiviMuted">
                        <tr>
                          <th className="pr-2 py-1">Class</th>
                          <th className="pr-2 py-1">Make</th>
                          <th className="pr-2 py-1">Model</th>
                          <th className="pr-2 py-1">Variant</th>
                          <th className="py-1">Engine</th>
                        </tr>
                      </thead>
                      <tbody className="text-kiviInk">
                        {list.map((r, i) => (
                          <tr key={i} className="border-t border-kiviBorder">
                            <td className="pr-2 py-1">{r.class}</td>
                            <td className="pr-2 py-1">{r.make}</td>
                            <td className="pr-2 py-1">{r.model}</td>
                            <td className="pr-2 py-1">{r.variant}</td>
                            <td className="py-1">{r.engine}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </>
                )
              }
            />
          )}

          {tab === 'chip' && (
            <StateLine
              state={chipRows}
              idleHint="Search by chip part number/vendor or run a hexadecimal JEDEC MFR/DEV lookup."
              render={(list) =>
                list.length === 0 ? (
                  <span className="text-kiviMuted">{summariseSearch(0, 'chip')}</span>
                ) : (
                  <>
                    <p className="mb-2 text-kiviMuted">{summariseSearch(list.length, 'chip')}</p>
                    <table className="w-full text-left font-mono text-[10px]">
                      <thead className="text-kiviMuted">
                        <tr>
                          <th className="pr-2 py-1">Manufacturer</th>
                          <th className="pr-2 py-1">Chip</th>
                          <th className="pr-2 py-1">Package</th>
                          <th className="pr-2 py-1">Prog ID</th>
                          <th className="py-1">JEDEC Pairs</th>
                        </tr>
                      </thead>
                      <tbody className="text-kiviInk">
                        {list.map((r, i) => (
                          <tr key={i} className="border-t border-kiviBorder">
                            <td className="pr-2 py-1">{r.manufacturer}</td>
                            <td className="pr-2 py-1 text-kiviAccent">{r.chip_name}</td>
                            <td className="pr-2 py-1">{r.package}</td>
                            <td className="pr-2 py-1">
                              {r.programmer_id !== null
                                ? `0x${r.programmer_id.toString(16).toUpperCase()}`
                                : '-'}
                            </td>
                            <td className="py-1">
                              {r.jedec_ids.length > 0
                                ? r.jedec_ids
                                    .map(
                                      ([m, d]) =>
                                        `0x${m.toString(16).toUpperCase()}:0x${d
                                          .toString(16)
                                          .toUpperCase()}`
                                    )
                                    .join(', ')
                                : '-'}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </>
                )
              }
            />
          )}

          {tab === 'capability' && (
            <StateLine
              state={capabilityRows}
              idleHint="Search protocol capability flags (RD/VR/WR/CK/No-CK)."
              render={(list) =>
                list.length === 0 ? (
                  <span className="text-kiviMuted">{summariseSearch(0, 'capability')}</span>
                ) : (
                  <>
                    <p className="mb-2 text-kiviMuted">{summariseSearch(list.length, 'capability')}</p>
                    <table className="w-full text-left font-mono text-[10px]">
                      <thead className="text-kiviMuted">
                        <tr>
                          <th className="pr-3 py-1">Description</th>
                          <th className="py-1">Published Flags</th>
                        </tr>
                      </thead>
                      <tbody className="text-kiviInk">
                        {list.map((r, i) => (
                          <tr key={i} className="border-t border-kiviBorder">
                            <td className="pr-3 py-1.5">{r.description}</td>
                            <td className="py-1.5">
                              <div className="flex flex-wrap gap-1">
                                <BoolFlag on={r.direct_read} label="RD" />
                                <BoolFlag on={r.virtual_read} label="VR" />
                                <BoolFlag on={r.write_flash} label="WR" />
                                <BoolFlag on={r.checksum_correct} label="CK" />
                                <BoolFlag on={r.no_checksum} label="No-CK" />
                              </div>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </>
                )
              }
            />
          )}

          {tab === 'module' && (
            <StateLine
              state={moduleRows}
              idleHint="Search reference binary modules or use a preset tag (Bosch, Honda, Denso, Toyota)."
              render={(list) =>
                list.length === 0 ? (
                  <span className="text-kiviMuted">{summariseSearch(0, 'module')}</span>
                ) : (
                  <>
                    <p className="mb-2 text-kiviMuted">{summariseSearch(list.length, 'module')}</p>
                    <table className="w-full text-left font-mono text-[10px]">
                      <thead className="text-kiviMuted">
                        <tr>
                          <th className="pr-2 py-1">File #</th>
                          <th className="pr-2 py-1">Vendor #</th>
                          <th className="pr-2 py-1">Coverage</th>
                          <th className="pr-2 py-1">Size</th>
                          <th className="py-1">Blobs</th>
                        </tr>
                      </thead>
                      <tbody className="text-kiviInk">
                        {list.map((r, i) => (
                          <tr key={i} className="border-t border-kiviBorder">
                            <td className="pr-2 py-1">module{r.file_index}.dll</td>
                            <td className="pr-2 py-1">#{r.vendor_number}</td>
                            <td className="pr-2 py-1">{r.coverage}</td>
                            <td className="pr-2 py-1">
                              {(r.file_size / 1024).toFixed(1)} KiB
                            </td>
                            <td className="py-1">{r.encrypted_blobs}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </>
                )
              }
            />
          )}
        </div>
      </div>
    </FloatingWindow>
  );
};
